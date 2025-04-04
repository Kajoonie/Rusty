use rusqlite::{params, Connection, Result as SqlResult};
use serenity::all::User;
use serenity::model::id::GuildId;
use std::sync::Once;

use crate::utils::ollama_client::OLLAMA_CLIENT;

pub const APPDATA_DB: &str = "application_data.db";
static DB_INIT: Once = Once::new();

pub struct UserPreference {
    pub user_id: String,
    pub username: String,
    pub model: String,
}

pub fn init_db() -> SqlResult<()> {
    DB_INIT.call_once(|| {
        if let Err(e) = create_tables() {
            eprintln!("Failed to create database tables: {}", e);
        }
    });
    Ok(())
}

fn create_tables() -> SqlResult<()> {
    let conn = Connection::open(APPDATA_DB)?;

    // User preferences table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_preferences (
            user_id TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            model TEXT NOT NULL
        )",
        [],
    )?;

    // Autoplay settings table with BOOLEAN type
    conn.execute(
        "CREATE TABLE IF NOT EXISTS autoplay_settings (
            guild_id INTEGER PRIMARY KEY,
            enabled BOOLEAN NOT NULL
        )",
        [],
    )?;

    Ok(())
}

pub fn get_user_model(user: &User) -> Option<String> {
    if let Ok(conn) = Connection::open(APPDATA_DB) {
        if let Ok(mut statement) =
            conn.prepare("SELECT model FROM user_preferences WHERE user_id = ?1")
        {
            if let Ok(mut rows) = statement.query([user.id.to_string()]) {
                if let Ok(Some(row)) = rows.next() {
                    if let Ok(model) = row.get(0) {
                        return model;
                    }
                }
            }
        }
    }

    OLLAMA_CLIENT.clone().get_default_model() // Default model
}

pub fn set_user_preference(pref: &UserPreference) -> SqlResult<()> {
    let conn = Connection::open(APPDATA_DB)?;
    conn.execute(
        "INSERT OR REPLACE INTO user_preferences (user_id, username, model) VALUES (?1, ?2, ?3)",
        (&pref.user_id, &pref.username, &pref.model),
    )?;
    Ok(())
}

// New functions for autoplay settings

pub fn set_autoplay_setting(guild_id: GuildId, enabled: bool) -> SqlResult<()> {
    let conn = Connection::open(APPDATA_DB)?;
    conn.execute(
        "INSERT OR REPLACE INTO autoplay_settings (guild_id, enabled) VALUES (?1, ?2)",
        params![guild_id.get(), enabled],
    )?;
    Ok(())
}

pub fn get_autoplay_setting(guild_id: GuildId) -> bool {
    if let Ok(conn) = Connection::open(APPDATA_DB) {
        if let Ok(mut statement) =
            conn.prepare("SELECT enabled FROM autoplay_settings WHERE guild_id = ?1")
        {
            if let Ok(mut rows) = statement.query(params![guild_id.get()]) {
                if let Ok(Some(row)) = rows.next() {
                    if let Ok(enabled) = row.get(0) {
                        return enabled;
                    }
                }
            }
        }
    }

    false // Default: autoplay disabled
}
