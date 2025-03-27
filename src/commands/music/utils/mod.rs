use std::time::Duration;

// Export music utilities
pub mod audio_sources;
pub mod autoplay_manager;
pub mod button_controls;
pub mod button_handlers;
pub mod embedded_messages;
pub mod event_handlers;
pub mod music_manager;
pub mod queue_manager;
pub mod song_fetchers;
pub mod spotify_api;

/// Format a duration into a human-readable string (e.g., "3:45" or "1:23:45")
pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}
