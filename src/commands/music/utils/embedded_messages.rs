//! Provides functions to generate standardized embeds and replies for music commands.
//! Includes the main player message, status updates, and error messages.

use ::serenity::all::GuildId;
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::all::CreateEmbed;
use songbird::tracks::{PlayMode, TrackQueue};
use std::{sync::Arc, time::Duration};

use crate::commands::music::{
    audio_sources::track_metadata::TrackMetadata,
    utils::{button_controls, format_duration, music_manager::MusicError},
};

use super::{
    button_controls::{ButtonData, RepeatState},
    music_manager::MusicManager,
};

/// Data required to construct the main music player message.
pub struct PlayerMessageData {
    /// The current track queue (optional).
    pub queue: Option<TrackQueue>,
    /// Whether the detailed queue view is currently enabled.
    pub show_queue: bool,
    /// The current repeat state.
    pub repeat_state: RepeatState,
}

/// Generates a simple text-based progress bar string.
/// Example: `▬▬▬▬🔘▬▬▬▬▬▬▬`
fn format_progress_bar(position: Duration, total: Duration) -> String {
    const BAR_LENGTH: usize = 15;

    // Prevent division by zero if total duration is zero.
    if total.as_secs() == 0 {
        return format!("{}🔘{}", "", "▬".repeat(BAR_LENGTH)); // Return empty progress bar
    }

    // Ensure position wraps around for looped tracks.
    let wrapped_position = Duration::from_secs(position.as_secs() % total.as_secs());
    // Calculate the progress ratio.
    let progress = wrapped_position.as_secs_f64() / total.as_secs_f64();

    // Determine number of filled and empty segments.
    let filled = (progress * BAR_LENGTH as f64).round() as usize;
    let empty = BAR_LENGTH - filled;

    // Construct the bar string.
    format!("{}🔘{}", "▬".repeat(filled), "▬".repeat(empty))
}

/// Extracts common metadata fields (title, URL, formatted duration) from `TrackMetadata`.
fn parse_metadata(metadata: &TrackMetadata) -> (String, String, String) {
    let title = metadata.title.clone();
    let url = metadata.url.clone().unwrap_or_else(|| "#".to_string());
    // Format duration or use default text.
    let duration_str = metadata
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "Unknown duration".to_string());

    (title, url, duration_str)
}

/// Asynchronously generates the main music player `CreateReply` (embed + components).
/// Fetches current state (queue, track info, repeat state, etc.) from `MusicManager`
/// and constructs the appropriate embed and buttons.
pub async fn music_player_message(guild_id: GuildId) -> Result<CreateReply, MusicError> {
    // Initialize the embed with a default color.
    let mut embed = CreateEmbed::new().color(0x00ff00); // Green color

    // Fetch necessary data from the MusicManager.
    let data = MusicManager::get_player_message_data(&guild_id).await;

    // Ensure the queue exists.
    let queue = data.queue.ok_or(MusicError::NoQueue)?;

    // Get the currently playing track handle (if any).
    let current_track_opt = queue.current();

    // Get queue display and content status.
    let show_queue = data.show_queue;
    let has_queue = !queue.is_empty();

    // Initialize state variables for button logic.
    let mut is_playing = false;
    let mut no_track = false;

    // Process if a track is currently loaded.
    if let Some(track_handle) = &current_track_opt {
        // Get metadata attached to the track handle.
        let metadata: Arc<TrackMetadata> = track_handle.data();

        // Get playback info (position, playing state).
        match track_handle.get_info().await {
            Ok(track_info) => {
                // Update playing state.
                is_playing = track_info.playing == PlayMode::Play;

                // Build the main embed content.
                embed = embed.title("🎵 Music Player");

                // Add thumbnail if available
                if let Some(thumbnail) = &metadata.thumbnail {
                    embed = embed.thumbnail(thumbnail);
                }

                // Extract common metadata.
                let (title, url, _) = parse_metadata(&metadata);
                // Start description with 'Now Playing'.
                let mut description = format!("**Now Playing:** [{}]({})\n", title, url);

                // Calculate and format progress bar and time.
                let duration = metadata.duration.unwrap_or(Duration::from_secs(0));
                let position = track_info.position;
                let progress = format_progress_bar(position, duration);

                // Position / Duration
                let wrapped_position = Duration::from_secs(position.as_secs() % duration.as_secs());
                let pos_str = format_duration(wrapped_position);
                let dur_str = format_duration(duration);
                // Add progress bar and time to description.
                description.push_str(&format!("{} `{}/{}`\n\n", progress, pos_str, dur_str));

                // Add queue summary if not empty.
                if !queue.is_empty() {
                    // Calculate remaining time in current track.
                    let remaining_in_current_track = duration.saturating_sub(position);

                    // Calculate total duration of the rest of the queue.
                    let queue_duration: Duration = queue
                        .current_queue()
                        .iter()
                        .skip(1) // Ignore head of queue, currently-playing song
                        .filter_map(|track| {
                            let metadata: Arc<TrackMetadata> = track.data();
                            metadata.duration
                        })
                        .sum();

                    // Format total remaining duration.
                    let total_duration_str =
                        format_duration(queue_duration + remaining_in_current_track);
                    // Add queue summary line.
                    description.push_str(&format!(
                        "**Queue:** {} tracks (`{}` remaining)\n",
                        queue.len() - 1, // Ignore head of queue, currently-playing song
                        total_duration_str
                    ));
                } else {
                    // Add empty queue message.
                    description.push_str("**Queue:** Empty\n");
                }

                // Add detailed upcoming tracks if toggled and queue has items.
                if show_queue && queue.len() > 1 {
                    // Again, > 1 rather than !is_empty() to ignore head of queue
                    description.push_str("\n**Upcoming Tracks:**\n");
                    // Iterate through the next 10 tracks in the queue.
                    for (index, track) in queue
                        .current_queue()
                        .iter()
                        .skip(1) // Ignore head of queue
                        .take(10) // Display only the first 10 tracks
                        .enumerate()
                    {
                        // Get metadata for the queued track.
                        let metadata: Arc<TrackMetadata> = track.data();
                        // Format the queue entry line.
                        description.push_str(&format!(
                            "{}. [{}]({})",
                            index + 1,
                            metadata.title,
                            metadata.url.as_deref().unwrap_or("#")
                        ));
                            // Add duration if available.
                        if let Some(dur) = metadata.duration {
                            description.push_str(&format!(" `{}`", format_duration(dur)));
                        }
                        description.push('\n');
                    }
                    // Indicate if there are more tracks beyond the displayed 10.
                    if queue.len() > 11 {
                        description.push_str(&format!("... and {} more\n", queue.len() - 11));
                    }
                }

                // Set the constructed description on the embed.
                embed = embed.description(description);
            }
            Err(songbird::error::ControlError::Finished) => {
                // Handle the case where the track just finished during the update.
                no_track = true;
            }
            Err(e) => {
                // Handle other track info errors.
                return Err(MusicError::AudioSourceError(e.to_string()));
            }
        }
    } else {
        // No track handle exists.
        no_track = true;
    }

    if no_track {
        // Set description for when nothing is playing.
        embed = embed.description("**🔇 Nothing playing or queued.**");
    }

    // Get the current repeat state.
    let repeat_state = data.repeat_state;

    // Prepare data for button state generation.
    let button_data = ButtonData {
        is_playing,
        has_queue,
        show_queue,
        no_track,
        repeat_state,
    };

    // Build the final reply with the embed and stateful buttons.
    let reply = CreateReply::default()
        .embed(embed)
        .components(button_controls::stateful_interaction_buttons(button_data));

    Ok(reply)
}

// --- Simple Ephemeral Messages ---

/// Creates an ephemeral reply indicating the autoplay status.
pub fn autoplay_status(enabled: bool) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title(if enabled {
                    "🔄 Autoplay Enabled"
                } else {
                    "⏹️ Autoplay Disabled"
                })
                .description(if enabled {
                    "I will automatically play related songs when the queue is empty"
                } else {
                    "I will stop playing when the queue is empty"
                })
                .color(if enabled { 0x00ff00 } else { 0xff0000 }), // Green/Red
        )
        .ephemeral(true)
}

/// Creates a generic ephemeral success reply.
pub fn generic_success(title: &str, description: &str) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title(title)
                .description(description)
                .color(0x00ff00), // Green color
        )
        .ephemeral(true)
}

/// Creates a generic ephemeral error reply.
pub fn generic_error(description: &str) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(description)
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Creates an ephemeral error reply indicating the queue is empty.
pub fn queue_is_empty() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("The queue is empty")
            .color(0xff0000),
    )
}

/// Creates an ephemeral error reply indicating an invalid queue position was provided.
pub fn invalid_queue_position(queue_length: usize) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!(
                "Invalid position. The queue has {} tracks",
                queue_length - 1
            ))
            .color(0xff0000),
    )
}

/// Creates an ephemeral reply confirming a track was removed from the queue.
pub fn track_removed(metadata: &TrackMetadata, position: usize) -> CreateReply {
    // Extract metadata for the message.
    let (title, url, _) = parse_metadata(metadata);

    CreateReply::default().embed(
        CreateEmbed::new()
            .title("🗑️ Track Removed")
            .description(format!(
                "Removed [{}]({}) from position #{}",
                title, url, position
            ))
            .color(0x00ff00),
    )
}
