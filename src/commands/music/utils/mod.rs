//! This module aggregates various utility functions and submodules used by the music commands.

use std::time::Duration;

/// Manages the autoplay state for guilds.
pub(crate) mod autoplay_manager;
/// Defines the button components used for music controls.
pub(crate) mod button_controls;
/// Handles interactions with music control components (buttons).
pub(crate) mod component_handlers;
/// Provides functions to create standardized embed messages for music commands.
pub(crate) mod embedded_messages;
/// Contains event handlers specific to the music feature (e.g., Songbird events).
pub(crate) mod event_handlers;
/// The core manager for music playback, handling queues, voice connections, and Songbird integration.
pub(crate) mod music_manager;

/// Formats a `std::time::Duration` into a human-readable string.
///
/// Examples:
/// * `Duration::from_secs(225)` -> "3:45"
/// * `Duration::from_secs(3723)` -> "1:02:03"
pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        // Format as H:MM:SS if hours > 0.
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        // Format as M:SS otherwise.
        format!("{}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration_zero_seconds() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0:00");
    }

    #[test]
    fn test_format_duration_seconds_only() {
        assert_eq!(format_duration(Duration::from_secs(9)), "0:09");
        assert_eq!(format_duration(Duration::from_secs(59)), "0:59");
    }

    #[test]
    fn test_format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1:00");
        assert_eq!(format_duration(Duration::from_secs(225)), "3:45");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59:59");
    }

    #[test]
    fn test_format_duration_hours_minutes_seconds() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(format_duration(Duration::from_secs(3723)), "1:02:03");
        assert_eq!(format_duration(Duration::from_secs(36000)), "10:00:00");
    }
}
