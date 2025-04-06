//! Manages the autoplay state for guilds.
//! Uses an in-memory cache (`HashMap`) and persists settings to a database.
//! Provides a globally accessible, thread-safe manager instance.

use serenity::model::id::GuildId;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

use crate::utils::database;

/// Manages autoplay settings for multiple guilds.
pub struct AutoplayManager {
    /// In-memory cache mapping GuildId to its autoplay status (true = enabled).
    autoplay_settings: HashMap<GuildId, bool>,
}

impl AutoplayManager {
    /// Creates a new, empty `AutoplayManager`.
    pub fn new() -> Self {
        Self {
            autoplay_settings: HashMap::new(),
        }
    }

    /// Sets the autoplay status for a specific guild.
    /// Updates both the in-memory cache and the persistent database setting.
    pub fn set_autoplay(&mut self, guild_id: GuildId, enabled: bool) {
        // Update the cache.
        self.autoplay_settings.insert(guild_id, enabled);

        // Attempt to save the setting to the database, logging any errors.
        if let Err(e) = database::set_autoplay_setting(guild_id, enabled) {
            eprintln!("Failed to save autoplay setting to database: {}", e);
        }
    }

    /// Checks if autoplay is enabled for a specific guild.
    ///
    /// Checks the in-memory cache first. If the guild is not found in the cache,
    /// it queries the database, updates the cache, and then returns the status.
    pub fn is_autoplay_enabled(&mut self, guild_id: GuildId) -> bool {
        // Check cache first.
        if let Some(&enabled) = self.autoplay_settings.get(&guild_id) {
            return enabled;
        }

        // Not in cache, fetch from database.
        let enabled = database::get_autoplay_setting(guild_id);

        // Update cache with the value fetched from the database.
        self.autoplay_settings.insert(guild_id, enabled);

        enabled
    }
}

/// Global, thread-safe instance of the `AutoplayManager`.
/// Lazily initialized and wrapped in `Arc<Mutex>` for safe concurrent access.
pub static AUTOPLAY_MANAGER: LazyLock<Arc<Mutex<AutoplayManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(AutoplayManager::new())));

/// Asynchronously sets the autoplay status for a guild using the global manager.
pub async fn set_autoplay(guild_id: GuildId, enabled: bool) {
    // Lock the global manager.
    let mut manager = AUTOPLAY_MANAGER.lock().await;
    // Call the manager's method.
    manager.set_autoplay(guild_id, enabled);
}

/// Asynchronously checks if autoplay is enabled for a guild using the global manager.
pub async fn is_autoplay_enabled(guild_id: GuildId) -> bool {
    // Lock the global manager.
    let mut manager = AUTOPLAY_MANAGER.lock().await;
    // Call the manager's method.
    manager.is_autoplay_enabled(guild_id)
}
