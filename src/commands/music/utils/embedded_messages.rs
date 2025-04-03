use poise::{CreateReply, serenity_prelude as serenity};
use serenity::all::CreateEmbed;
use songbird::tracks::{PlayMode, TrackQueue};
use std::{sync::Arc, time::Duration};
use tracing::{debug, error};

use crate::commands::music::{
    audio_sources::track_metadata::TrackMetadata,
    utils::{button_controls, format_duration, music_manager::MusicError},
};

use super::button_controls::RepeatState;

pub struct PlayerMessageData<'a> {
    pub queue: Option<&'a TrackQueue>,
    pub show_queue: bool,
    pub has_history: bool,
}

/// Create a progress bar for the current track
fn format_progress_bar(position: Duration, total: Duration) -> String {
    const BAR_LENGTH: usize = 15;
    let progress = if total.as_secs() == 0 {
        0.0
    } else {
        position.as_secs_f64() / total.as_secs_f64()
    };

    let filled = (progress * BAR_LENGTH as f64).round() as usize;
    let empty = BAR_LENGTH - filled;

    format!("{}🔘{}", "▬".repeat(filled), "▬".repeat(empty))
}

/// Parse the metadata for the now playing and added to queue embeds
fn parse_metadata(metadata: &TrackMetadata) -> (String, String, String) {
    let title = metadata.title.clone();
    let url = metadata.url.clone().unwrap_or_else(|| "#".to_string());
    let duration_str = metadata
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "Unknown duration".to_string());

    (title, url, duration_str)
}

/// Generates the main music player message embed and components.
pub async fn music_player_message(data: PlayerMessageData<'_>) -> Result<CreateReply, MusicError> {
    let mut embed = CreateEmbed::new().color(0x00ff00); // Green color

    let queue = data.queue.ok_or(MusicError::NoQueue)?;

    let current_track_opt = queue.current();

    let show_queue = data.show_queue;
    let has_history = data.has_history;
    let has_queue = !queue.is_empty();

    let mut is_playing = false;
    let mut no_track = false;

    if let Some(track_handle) = &current_track_opt {
        let metadata: Arc<TrackMetadata> = track_handle.data();

        match track_handle.get_info().await {
            Ok(track_info) => {
                is_playing = track_info.playing == PlayMode::Play;

                // Track is valid and playing/paused, build the detailed embed
                embed = embed.title("🎵 Music Player");

                // Add thumbnail if available
                if let Some(thumbnail) = &metadata.thumbnail {
                    embed = embed.thumbnail(thumbnail);
                }

                let duration = metadata.duration.unwrap_or(Duration::from_secs(0));
                let position = track_info.position;
                let (title, url, _) = parse_metadata(&metadata);

                let mut description = format!("**Now Playing:** [{}]({})\n", title, url);

                // Progress Bar and Timings
                let progress = format_progress_bar(position, duration);
                let pos_str = format_duration(position);
                let dur_str = format_duration(duration);
                description.push_str(&format!("{} `{}/{}`\n\n", progress, pos_str, dur_str));

                if !queue.is_empty() {
                    let remaining_in_current_track = duration.saturating_sub(position);

                    let queue_duration: Duration = queue
                        .current_queue()
                        .iter()
                        .filter_map(|track| {
                            let metadata: Arc<TrackMetadata> = track.data();
                            metadata.duration
                        })
                        .sum();

                    let total_duration_str =
                        format_duration(queue_duration + remaining_in_current_track);
                    description.push_str(&format!(
                        "**Queue:** {} tracks (`{}` remaining)\n",
                        queue.len(),
                        total_duration_str
                    ));
                } else {
                    description.push_str("**Queue:** Empty\n");
                }

                // Detailed Queue View (if toggled)
                if show_queue && !queue.is_empty() {
                    description.push_str("\n**Upcoming Tracks:**\n");
                    for (index, track) in queue.current_queue().iter().take(10).enumerate() {
                        let metadata: Arc<TrackMetadata> = track.data();
                        description.push_str(&format!(
                            "{}. [{}]({})",
                            index + 1,
                            metadata.title,
                            metadata.url.as_deref().unwrap_or("#")
                        ));
                        if let Some(dur) = metadata.duration {
                            description.push_str(&format!(" `{}`", format_duration(dur)));
                        }
                        description.push('\n');
                    }
                    if queue.len() > 10 {
                        description.push_str(&format!("... and {} more\n", queue.len() - 10));
                    }
                }

                embed = embed.description(description);
            }
            Err(songbird::error::ControlError::Finished) => {
                // Track just finished, treat as if nothing is playing for this update cycle
                no_track = true;
            }
            Err(e) => {
                // Propagate other errors
                return Err(MusicError::AudioSourceError(e.to_string()));
            }
        }
    } else {
        no_track = true;
    }

    if no_track {
        // Nothing playing or track just ended
        embed = embed.description("**🔇 Nothing playing or queued.**");
    }

    let reply = CreateReply::default().embed(embed).components(
        button_controls::stateful_interaction_buttons(
            is_playing,
            has_queue,
            has_history,
            no_track,
            RepeatState::Disabled,
            false,
        ),
    );

    Ok(reply)
}

// --- Simple Ephemeral Messages ---

/// Create an embed for when a user is not connected to a voice channel (ephemeral)
pub fn user_not_in_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("You need to be in a voice channel: {}", err))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when autoplay is enabled or disabled (ephemeral)
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

/// Create an embed for when the bot fails to join a voice channel (ephemeral)
pub fn failed_to_join_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Failed to join voice channel: {}", err))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when the bot fails to process an audio source (ephemeral)
pub fn failed_to_process_audio_source(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Failed to process audio source: {}", err))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when the bot fails to add a track to the queue (ephemeral)
pub fn failed_to_add_to_queue(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Failed to add track to queue: {}", err))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when the queue is empty (ephemeral)
pub fn queue_is_empty() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description("The queue is empty.")
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when a queue position is invalid (ephemeral)
pub fn invalid_queue_position(queue_length: usize) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!(
                    "Invalid position. The queue has {} tracks.",
                    queue_length
                ))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Create an embed for when a track is removed from the queue (ephemeral)
pub fn track_removed(metadata: &TrackMetadata, position: usize) -> CreateReply {
    let (title, url, _) = parse_metadata(metadata);

    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("🗑️ Track Removed")
                .description(format!(
                    "Removed [{}]({}) from position #{}.",
                    title, url, position
                ))
                .color(0x00ff00), // Green color
        )
        .ephemeral(true)
}

/// Generic error message (ephemeral)
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

/// Create an embed for when the bot fails to remove a track (ephemeral)
pub fn failed_to_remove_track() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description("Failed to remove track.")
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

/// Generic success message (ephemeral)
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
