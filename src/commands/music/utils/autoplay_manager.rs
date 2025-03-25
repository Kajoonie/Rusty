use rusqlite::{params, Connection, Result as SqliteResult};
use serenity::model::id::GuildId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AutoplayManager {
    // Map of guild ID to autoplay enabled status (in-memory cache)
    autoplay_settings: HashMap<u64, bool>,
    // Database connection
    connection: Connection,
}

impl AutoplayManager {
    pub fn new() -> Self {
        // Initialize database connection
        let db_connection = match Self::initialize_db() {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Error initializing database: {}. Using in-memory only.", e);
                Connection::open_in_memory().expect("Failed to open in-memory database")
            }
        };

        let mut manager = Self {
            autoplay_settings: HashMap::new(),
            connection: db_connection,
        };

        // Load settings from database
        if let Err(e) = manager.load_settings() {
            eprintln!("Error loading autoplay settings: {}. Using defaults.", e);
        }

        manager
    }

    // Initialize the database and create tables if they don't exist
    fn initialize_db() -> SqliteResult<Connection> {
        let conn = Connection::open("user_preferences.db")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS autoplay_settings (
                guild_id INTEGER PRIMARY KEY,
                enabled INTEGER NOT NULL
            )",
            [],
        )?;

        Ok(conn)
    }

    // Load settings from database into memory
    fn load_settings(&mut self) -> SqliteResult<()> {
        let mut stmt = self
            .connection
            .prepare("SELECT guild_id, enabled FROM autoplay_settings")?;

        let rows = stmt.query_map([], |row| {
            let guild_id: u64 = row.get(0)?;
            let enabled: bool = row.get(1)?;
            Ok((guild_id, enabled))
        })?;

        for (guild_id, enabled) in rows.flatten() {
            self.autoplay_settings.insert(guild_id, enabled);
        }

        Ok(())
    }

    // Save a single setting to the database
    fn save_setting(&self, guild_id: u64, enabled: bool) -> SqliteResult<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO autoplay_settings (guild_id, enabled) VALUES (?1, ?2)",
            params![guild_id, enabled],
        )?;

        Ok(())
    }

    pub fn set_autoplay(&mut self, guild_id: GuildId, enabled: bool) {
        let guild_id_u64 = guild_id.get();
        self.autoplay_settings.insert(guild_id_u64, enabled);

        // Save setting to database
        if let Err(e) = self.save_setting(guild_id_u64, enabled) {
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
