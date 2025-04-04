use std::time::Duration;

pub(crate) mod autoplay_manager;
pub(crate) mod button_controls;
pub(crate) mod component_handlers;
pub(crate) mod embedded_messages;
pub(crate) mod event_handlers;
pub(crate) mod music_manager;
// pub(crate) mod queue_manager;

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
