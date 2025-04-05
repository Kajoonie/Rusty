use serenity::model::id::GuildId;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

use crate::utils::database;

pub struct AutoplayManager {
    // Map of guild ID to autoplay enabled status (in-memory cache)
    autoplay_settings: HashMap<GuildId, bool>,
}

impl AutoplayManager {
    pub fn new() -> Self {
        Self {
            autoplay_settings: HashMap::new(),
        }
    }

    pub fn set_autoplay(&mut self, guild_id: GuildId, enabled: bool) {
        self.autoplay_settings.insert(guild_id, enabled);

        // Save setting to database
        if let Err(e) = database::set_autoplay_setting(guild_id, enabled) {
            eprintln!("Failed to save autoplay setting to database: {}", e);
        }
    }

    pub fn is_autoplay_enabled(&mut self, guild_id: GuildId) -> bool {
        // Check if the setting is in the cache
        if let Some(&enabled) = self.autoplay_settings.get(&guild_id) {
            return enabled;
        }

        // If not in cache, query from database
        let enabled = database::get_autoplay_setting(guild_id);

        // Store in cache for future use
        self.autoplay_settings.insert(guild_id, enabled);

        enabled
    }
}

// Create a global autoplay manager wrapped in a mutex for thread safety
pub static AUTOPLAY_MANAGER: LazyLock<Arc<Mutex<AutoplayManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(AutoplayManager::new())));

// Helper functions for working with the global autoplay manager
pub async fn set_autoplay(guild_id: GuildId, enabled: bool) {
    let mut manager = AUTOPLAY_MANAGER.lock().await;
    manager.set_autoplay(guild_id, enabled);
}

pub async fn is_autoplay_enabled(guild_id: GuildId) -> bool {
    let mut manager = AUTOPLAY_MANAGER.lock().await;
    manager.is_autoplay_enabled(guild_id)
}
