use rusqlite::{Connection, Result as SqlResult, params};
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, Error as SqliteError};
    use serenity::model::id::GuildId;

    // Helper to set up an in-memory database and create tables using the logic
    // similar to the main create_tables function.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("Failed to open in-memory database");
        // Replicate table creation logic for the test connection
        conn.execute(
            "CREATE TABLE user_preferences (
                user_id TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                model TEXT NOT NULL
            )",
            [],
        )
        .expect("Failed to create user_preferences table");
        conn.execute(
            "CREATE TABLE autoplay_settings (
                guild_id INTEGER PRIMARY KEY,
                enabled BOOLEAN NOT NULL
            )",
            [],
        )
        .expect("Failed to create autoplay_settings table");
        conn
    }

    #[test]
    fn test_set_and_get_user_preference() {
        let conn = setup_db();
        let user_id = "123456789".to_string();
        let username = "testuser".to_string();
        let model = "test-model-v1".to_string();

        // Simulate set_user_preference logic on the test DB
        conn.execute(
            "INSERT OR REPLACE INTO user_preferences (user_id, username, model) VALUES (?1, ?2, ?3)",
            params![&user_id, &username, &model],
        )
        .expect("Failed to insert user preference");

        // Simulate get_user_model's query logic on the test DB
        let mut stmt = conn
            .prepare("SELECT model FROM user_preferences WHERE user_id = ?1")
            .unwrap();
        let retrieved_model: Option<String> = stmt.query_row([&user_id], |row| row.get(0)).ok();

        assert_eq!(retrieved_model, Some(model));

        // Test replacement
        let new_model = "test-model-v2".to_string();
        conn.execute(
            "INSERT OR REPLACE INTO user_preferences (user_id, username, model) VALUES (?1, ?2, ?3)",
            params![&user_id, &username, &new_model], // Same user_id, new model
        )
        .expect("Failed to replace user preference");

        // Verify replacement
        let mut stmt_replace = conn
            .prepare("SELECT model FROM user_preferences WHERE user_id = ?1")
            .unwrap();
        let updated_model: Option<String> =
            stmt_replace.query_row([&user_id], |row| row.get(0)).ok();
        assert_eq!(updated_model, Some(new_model));
    }

    #[test]
    fn test_get_user_model_non_existent() {
        let conn = setup_db();
        let user_id = "987654321".to_string(); // An ID not inserted

        // Simulate get_user_model's query logic for a non-existent user
        let mut stmt = conn
            .prepare("SELECT model FROM user_preferences WHERE user_id = ?1")
            .unwrap();
        let retrieved_model_result: Result<String, SqliteError> =
            stmt.query_row([&user_id], |row| row.get(0));

        // Expect an error because no row should be found
        assert!(retrieved_model_result.is_err());
        // Specifically, expect QueryReturnedNoRows
        assert!(matches!(
            retrieved_model_result.err().unwrap(),
            SqliteError::QueryReturnedNoRows
        ));
        // Note: The actual get_user_model function handles this error and returns a default.
        // This test verifies the database interaction part correctly identifies no record found.
    }

    #[test]
    fn test_set_and_get_autoplay_setting() {
        let conn = setup_db();
        let guild_id = GuildId::new(111222333);

        // Test setting true
        conn.execute(
            "INSERT OR REPLACE INTO autoplay_settings (guild_id, enabled) VALUES (?1, ?2)",
            params![guild_id.get(), true],
        )
        .expect("Failed to set autoplay to true");

        // Simulate get_autoplay_setting's query logic
        let mut stmt_true = conn
            .prepare("SELECT enabled FROM autoplay_settings WHERE guild_id = ?1")
            .unwrap();
        let retrieved_setting_true: Option<bool> =
            stmt_true.query_row([guild_id.get()], |row| row.get(0)).ok();
        assert_eq!(retrieved_setting_true, Some(true));

        // Test setting false (replacement)
        conn.execute(
            "INSERT OR REPLACE INTO autoplay_settings (guild_id, enabled) VALUES (?1, ?2)",
            params![guild_id.get(), false],
        )
        .expect("Failed to set autoplay to false");

        // Verify replacement
        let mut stmt_false = conn
            .prepare("SELECT enabled FROM autoplay_settings WHERE guild_id = ?1")
            .unwrap();
        let updated_setting: Option<bool> = stmt_false
            .query_row([guild_id.get()], |row| row.get(0))
            .ok();
        assert_eq!(updated_setting, Some(false));
    }

    #[test]
    fn test_get_autoplay_setting_non_existent() {
        let conn = setup_db();
        let guild_id = GuildId::new(444555666); // An ID not inserted

        // Simulate get_autoplay_setting's query logic for non-existent guild
        let mut stmt = conn
            .prepare("SELECT enabled FROM autoplay_settings WHERE guild_id = ?1")
            .unwrap();
        let retrieved_setting_result: Result<bool, SqliteError> =
            stmt.query_row([guild_id.get()], |row| row.get(0));

        // Expect an error because no row should be found
        assert!(retrieved_setting_result.is_err());
        // Specifically, expect QueryReturnedNoRows
        assert!(matches!(
            retrieved_setting_result.err().unwrap(),
            SqliteError::QueryReturnedNoRows
        ));
        // Note: The actual get_autoplay_setting function handles this error and returns false.
        // This test verifies the database interaction part correctly identifies no record found.
    }

    // Note: Testing init_db() directly is complex due to std::sync::Once.
    // The setup_db helper effectively tests the table creation SQL.
    // Testing the actual public functions' interaction with the test DB is limited
    // because they hardcode the production DB path (APPDATA_DB).
    // These tests verify that the SQL logic used within those functions works correctly
    // against the expected schema in an isolated in-memory environment.
}
