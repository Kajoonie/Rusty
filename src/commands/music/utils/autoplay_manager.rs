use crate::database;
use serenity::model::id::GuildId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AutoplayManager {
    // Map of guild ID to autoplay enabled status (in-memory cache)
    autoplay_settings: HashMap<u64, bool>,
}

impl AutoplayManager {
    pub fn new() -> Self {
        let mut manager = Self {
            autoplay_settings: HashMap::new(),
        };

        // Load settings from database
        if let Err(e) = manager.load_settings() {
            eprintln!("Error loading autoplay settings: {}. Using defaults.", e);
        }

        manager
    }

    // Load settings from database into memory
    fn load_settings(&mut self) -> Result<(), rusqlite::Error> {
        match database::load_all_autoplay_settings() {
            Ok(settings) => {
                for (guild_id, enabled) in settings {
                    self.autoplay_settings.insert(guild_id, enabled);
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn set_autoplay(&mut self, guild_id: GuildId, enabled: bool) {
        let guild_id_u64 = guild_id.get();
        self.autoplay_settings.insert(guild_id_u64, enabled);

        // Save setting to database
        if let Err(e) = database::set_autoplay_setting(guild_id, enabled) {
            eprintln!("Failed to save autoplay setting to database: {}", e);
        }
    }

    pub fn is_autoplay_enabled(&self, guild_id: GuildId) -> bool {
        *self
            .autoplay_settings
            .get(&guild_id.get())
            .unwrap_or(&false)
    }
}

// Create a global autoplay manager wrapped in a mutex for thread safety
lazy_static::lazy_static! {
    pub static ref AUTOPLAY_MANAGER: Arc<Mutex<AutoplayManager>> = Arc::new(Mutex::new(AutoplayManager::new()));
}

// Helper functions for working with the global autoplay manager
pub async fn set_autoplay(guild_id: GuildId, enabled: bool) {
    let mut manager = AUTOPLAY_MANAGER.lock().await;
    manager.set_autoplay(guild_id, enabled);
}

pub async fn is_autoplay_enabled(guild_id: GuildId) -> bool {
    let manager = AUTOPLAY_MANAGER.lock().await;
    manager.is_autoplay_enabled(guild_id)
}
