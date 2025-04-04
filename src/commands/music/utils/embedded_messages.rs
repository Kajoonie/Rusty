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

pub struct PlayerMessageData {
    pub queue: Option<TrackQueue>,
    pub show_queue: bool,
    pub repeat_state: RepeatState,
}

/// Create a progress bar for the current track
fn format_progress_bar(position: Duration, total: Duration) -> String {
    const BAR_LENGTH: usize = 15;

    // Handle the case where total duration is 0 to avoid division by zero
    if total.as_secs() == 0 {
        return format!("{}🔘{}", "", "▬".repeat(BAR_LENGTH)); // Return empty progress bar
    }

    // Use modulo to wrap the position within the track duration
    let wrapped_position = Duration::from_secs(position.as_secs() % total.as_secs());
    let progress = wrapped_position.as_secs_f64() / total.as_secs_f64();

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
pub async fn music_player_message(guild_id: GuildId) -> Result<CreateReply, MusicError> {
    let mut embed = CreateEmbed::new().color(0x00ff00); // Green color

    let data = MusicManager::get_player_message_data(&guild_id).await;

    let queue = data.queue.ok_or(MusicError::NoQueue)?;

    let current_track_opt = queue.current();

    let show_queue = data.show_queue;
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

                let (title, url, _) = parse_metadata(&metadata);
                let mut description = format!("**Now Playing:** [{}]({})\n", title, url);

                // Progress Bar
                let duration = metadata.duration.unwrap_or(Duration::from_secs(0));
                let position = track_info.position;
                let progress = format_progress_bar(position, duration);

                // Position / Duration
                let wrapped_position = Duration::from_secs(position.as_secs() % duration.as_secs());
                let pos_str = format_duration(wrapped_position);
                let dur_str = format_duration(duration);
                description.push_str(&format!("{} `{}/{}`\n\n", progress, pos_str, dur_str));

                if !queue.is_empty() {
                    let remaining_in_current_track = duration.saturating_sub(position);

                    let queue_duration: Duration = queue
                        .current_queue()
                        .iter()
                        .skip(1) // Ignore head of queue, currently-playing song
                        .filter_map(|track| {
                            let metadata: Arc<TrackMetadata> = track.data();
                            metadata.duration
                        })
                        .sum();

                    let total_duration_str =
                        format_duration(queue_duration + remaining_in_current_track);
                    description.push_str(&format!(
                        "**Queue:** {} tracks (`{}` remaining)\n",
                        queue.len() - 1, // Ignore head of queue, currently-playing song
                        total_duration_str
                    ));
                } else {
                    description.push_str("**Queue:** Empty\n");
                }

                // Detailed Queue View (if toggled)
                if show_queue && queue.len() > 1 {
                    // Again, > 1 rather than !is_empty() to ignore head of queue
                    description.push_str("\n**Upcoming Tracks:**\n");
                    for (index, track) in queue
                        .current_queue()
                        .iter()
                        .skip(1) // Ignore head of queue
                        .take(10) // Display only the first 10 tracks
                        .enumerate()
                    {
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
                    if queue.len() > 11 {
                        description.push_str(&format!("... and {} more\n", queue.len() - 11));
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

    let repeat_state = data.repeat_state;

    let button_data = ButtonData {
        is_playing,
        has_queue,
        show_queue,
        no_track,
        repeat_state,
    };

    let reply = CreateReply::default()
        .embed(embed)
        .components(button_controls::stateful_interaction_buttons(button_data));

    Ok(reply)
}

// --- Simple Ephemeral Messages ---

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

/// Create an embed for when the queue is empty
pub fn queue_is_empty() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("The queue is empty")
            .color(0xff0000),
    )
}

/// Create an embed for when a queue position is invalid
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

/// Create an embed for when a track is removed from the queue
pub fn track_removed(metadata: &TrackMetadata, position: usize) -> CreateReply {
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
