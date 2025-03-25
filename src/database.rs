use rusqlite::{Connection, Result as SqlResult};
use serenity::all::User;
use std::sync::Once;

pub const DB_PATH: &str = "user_preferences.db";
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
    let conn = Connection::open(DB_PATH)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_preferences (
            user_id TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            model TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

pub fn get_user_model(user: &User) -> String {
    if let Ok(conn) = Connection::open(DB_PATH) {
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

    "llama3.1:8b".to_string() // Default model
}

pub fn set_user_preference(pref: &UserPreference) -> SqlResult<()> {
    let conn = Connection::open(DB_PATH)?;
    conn.execute(
        "INSERT OR REPLACE INTO user_preferences (user_id, username, model) VALUES (?1, ?2, ?3)",
        (&pref.user_id, &pref.username, &pref.model),
    )?;
    Ok(())
}
