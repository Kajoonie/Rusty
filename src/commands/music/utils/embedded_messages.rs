use poise::{CreateReply, serenity_prelude as serenity};
use serenity::all::CreateEmbed;
use serenity::model::id::GuildId;
use songbird::tracks::PlayMode;
use std::time::Duration;

use crate::{
    Error,
    commands::music::utils::{
        button_controls, format_duration,
        music_manager::MusicError,
        queue_manager::{self, get_current_track},
    },
};

use super::{audio_sources::TrackMetadata, queue_manager::is_queue_view_enabled};

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
pub async fn music_player_message(guild_id: GuildId) -> Result<CreateReply, Error> {
    let mut reply = CreateReply::default();
    let mut embed = CreateEmbed::new().color(0x00ff00); // Green color

    let current_track_opt = get_current_track(guild_id).await?;
    let queue = queue_manager::get_queue(guild_id).await?;
    let show_queue = is_queue_view_enabled(guild_id).await;

    let has_queue = !queue.is_empty();

    // Determine button states
    let is_playing = match &current_track_opt {
        Some((handle, _)) => handle.get_info().await?.playing == PlayMode::Play,
        None => false,
    };

    reply = reply.components(button_controls::create_updated_buttons(
        is_playing, has_queue,
    ));

    // Build the embed content
    if let Some((track_handle, metadata)) = current_track_opt {
        embed = embed.title("🎵 Music Player");

        // Add thumbnail if available
        if let Some(thumbnail) = &metadata.thumbnail {
            embed = embed.thumbnail(thumbnail);
        }

        let track_info = track_handle.get_info().await?;
        let duration = metadata.duration.unwrap_or(Duration::from_secs(0));
        let position = track_info.position;

        let (title, url, _) = parse_metadata(&metadata);

        let mut description = format!("**Now Playing:** [{}]({})\n", title, url);

        // Progress Bar and Timings
        let progress = format_progress_bar(position, duration);
        let pos_str = format_duration(position);
        let dur_str = format_duration(duration);
        description.push_str(&format!("{} `{}/{}`\n\n", progress, pos_str, dur_str));

        // Queue Information (Total)
        if !queue.is_empty() {
            let remaining_in_current_track = duration.saturating_sub(position);
            let queue_duration: Duration = queue.iter().filter_map(|track| track.duration).sum();
            let total_duration_str = format_duration(queue_duration + remaining_in_current_track);

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
            for (index, track) in queue.iter().take(10).enumerate() {
                // Limit display
                let number = format!("{}.", index + 1);
                description.push_str(&format!(
                    "{} [{}]({})",
                    number,
                    track.title,
                    track.url.as_deref().unwrap_or("#")
                ));
                if let Some(dur) = track.duration {
                    description.push_str(&format!(" `{}`", format_duration(dur)));
                }
                description.push('\n');
            }
            if queue.len() > 10 {
                description.push_str(&format!("... and {} more\n", queue.len() - 10));
            }
        }

        embed = embed.description(description);
    } else {
        // Nothing playing or queued
        embed = embed.description("**🔇 Nothing playing or queued.**");
    }

    Ok(reply.embed(embed))
}

// --- Simple Ephemeral Messages ---

/// Create an embed for when a track is paused (ephemeral)
pub fn paused(metadata: &TrackMetadata) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("⏸️ Paused")
                .description(format!(
                    "Paused [{}]({})",
                    metadata.title,
                    metadata.url.as_deref().unwrap_or("#")
                ))
                .color(0x00ff00),
        )
        .ephemeral(true)
}

/// Create an embed for when a track is resumed (ephemeral)
pub fn resumed(metadata: &TrackMetadata) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("▶️ Resumed")
                .description(format!(
                    "Resumed [{}]({})",
                    metadata.title,
                    metadata.url.as_deref().unwrap_or("#")
                ))
                .color(0x00ff00),
        )
        .ephemeral(true)
}

/// Create an embed for when a track is not in a pausable state (ephemeral)
pub fn not_pausable() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description("The track is not in a pausable state")
                .color(0xff0000),
        )
        .ephemeral(true)
}

/// Create an embed for when no track is playing (ephemeral)
pub fn no_track_playing() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description("No track is currently playing")
                .color(0xff0000),
        )
        .ephemeral(true)
}

/// Create an embed for when the bot is not connected to a voice channel (ephemeral)
pub fn bot_not_in_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Not connected to a voice channel: {}", err))
                .color(0xff0000), // Red color
        )
        .ephemeral(true)
}

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

/// Create an embed for when the bot leaves a voice channel (ephemeral)
pub fn left_voice_channel() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("👋 Left Voice Channel")
                .description("Successfully disconnected and cleared the queue.")
                .color(0x00ff00), // Green color
        )
        .ephemeral(true)
}

/// Create an embed for when the bot fails to leave a voice channel (ephemeral)
pub fn failed_to_leave_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Failed to leave voice channel: {}", err))
                .color(0xff0000), // Red color
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

/// Create an embed for when the bot stops playing music (ephemeral)
pub fn stopped() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("⏹️ Stopped")
                .description("Playback stopped and queue cleared.")
                .color(0x00ff00), // Green color
        )
        .ephemeral(true)
}

/// Create an embed for when a track is skipped (ephemeral)
pub fn skipped(metadata: &TrackMetadata) -> CreateReply {
    let (title, url, _) = parse_metadata(metadata);

    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("⏭️ Skipped")
                .description(format!("Skipped [{}]({})", title, url))
                .color(0x00ff00), // Green color
        )
        .ephemeral(true)
}

/// Create an embed for when there is no track to skip (ephemeral)
pub fn no_track_to_skip() -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description("No track is currently playing to skip.")
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
