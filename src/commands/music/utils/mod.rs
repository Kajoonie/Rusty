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
