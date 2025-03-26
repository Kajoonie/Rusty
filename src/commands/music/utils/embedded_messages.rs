use poise::{serenity_prelude as serenity, CreateReply};
use serenity::all::CreateEmbed;
use songbird::tracks::TrackHandle;
use std::time::Duration;

use super::{audio_sources::TrackMetadata, format_duration, music_manager::MusicError};

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

    format!("▬{}🔘{}▬", "▬".repeat(filled), "▬".repeat(empty))
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

/// Create an embed for when a song is now playing
pub fn now_playing(metadata: &TrackMetadata) -> CreateEmbed {
    let (title, url, duration_str) = parse_metadata(metadata);

    CreateEmbed::new()
        .title("🎵 Now Playing")
        .description(format!("[{}]({})", title, url))
        .field("Duration", format!("`{}`", duration_str), true)
        .color(0x00ff00)
}

/// Create an embed for when a song is added to the queue
pub fn added_to_queue(metadata: &TrackMetadata, position: &usize) -> CreateEmbed {
    let (title, url, duration_str) = parse_metadata(metadata);

    CreateEmbed::new()
        .title("🎵 Added to Queue")
        .description(format!("[{}]({})", title, url))
        .field("Duration", format!("`{}`", duration_str), true)
        .field("Position", format!("`#{}`", position), true)
        .color(0x00ff00)
}

/// Create an embed for the music queue
pub async fn music_queue(
    current_track: &Option<(TrackHandle, TrackMetadata)>,
    queue: &Vec<TrackMetadata>,
) -> CreateEmbed {
    // Build the queue display
    let mut description = String::new();

    // Add current track information if there is one playing
    if let Some((track_handle, metadata)) = &current_track {
        let track_info = track_handle.get_info().await.ok();
        let position = track_info.as_ref().map(|info| info.position);

        description.push_str("**🎵 Now Playing**\n");
        description.push_str(&format!(
            "**[{}]({})**\n",
            metadata.title,
            metadata.url.as_deref().unwrap_or("#")
        ));

        // Add progress bar if we have duration and position
        if let (Some(duration), Some(pos)) = (metadata.duration, position) {
            let progress = format_progress_bar(pos, duration);
            let pos_str = format_duration(pos);
            let dur_str = format_duration(duration);
            description.push_str(&format!("{} `{}/{}`\n", progress, pos_str, dur_str));
        }

        description.push('\n');
    } else {
        description.push_str("**🔇 Nothing playing**\n\n");
    }

    // Add upcoming tracks
    if queue.is_empty() {
        description.push_str("**📭 Queue is empty**");
    } else {
        description.push_str(&format!("**📋 Queue - {} tracks**\n", queue.len()));
        for (index, track) in queue.iter().enumerate() {
            // Add track number emoji (1-10) or default bullet point
            let number = if index < 10 {
                format!("{}\u{FE0F}\u{20E3}", index + 1) // Unicode keycap emoji
            } else {
                "•".to_string()
            };

            description.push_str(&format!(
                "{} [{}]({})",
                number,
                track.title,
                track.url.as_deref().unwrap_or("#")
            ));

            if let Some(duration) = track.duration {
                description.push_str(&format!(" `{}`", format_duration(duration)));
            }
            description.push('\n');
        }

        // Add total duration if available
        let total_duration: Duration = queue.iter().filter_map(|track| track.duration).sum();
        if total_duration.as_secs() > 0 {
            description.push_str(&format!(
                "\n**⏱️ Total Duration:** `{}`",
                format_duration(total_duration)
            ));
        }
    }

    // Create and send the embed
    CreateEmbed::new()
        .title("🎵 Music Queue")
        .description(description)
        .color(0x00ff00)
}

/// Create an embed for when the bot is not connected to a voice channel
pub fn bot_not_in_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!("Not connected to a voice channel: {}", err))
            .color(0xff0000),
    )
}

/// Create an embed for when a user is not connected to a voice channel
pub fn user_not_in_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default()
        .embed(
            CreateEmbed::new()
                .title("❌ Error")
                .description(format!("You need to be in a voice channel: {}", err))
                .color(0xff0000),
        )
        .ephemeral(true)
}

/// Create an embed for when a track is paused
pub fn paused(metadata: &TrackMetadata) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("⏸️ Paused")
            .description(format!(
                "Paused [{}]({})",
                metadata.title,
                metadata.url.as_deref().unwrap_or("#")
            ))
            .color(0x00ff00),
    )
}

/// Create an embed for when a track is resumed
pub fn resumed(metadata: &TrackMetadata) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("▶️ Resumed")
            .description(format!(
                "Resumed [{}]({})",
                metadata.title,
                metadata.url.as_deref().unwrap_or("#")
            ))
            .color(0x00ff00),
    )
}

/// Create an embed for when a track is not in a pausable state
pub fn not_pausable() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("The track is not in a pausable state")
            .color(0xff0000),
    )
}

/// Create an embed for when no track is playing
pub fn no_track_playing() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("No track is currently playing")
            .color(0xff0000),
    )
}

/// Create an embed for when autoplay is enabled or disabled
pub fn autoplay_status(enabled: bool) -> CreateReply {
    CreateReply::default().embed(
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
            .color(if enabled { 0x00ff00 } else { 0xff0000 }),
    )
}

/// Create an embed for when the bot leaves a voice channel
pub fn left_voice_channel() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("👋 Left Voice Channel")
            .description("Successfully disconnected and cleared the queue")
            .color(0x00ff00),
    )
}

/// Create an embed for when the bot fails to leave a voice channel
pub fn failed_to_leave_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!("Failed to leave voice channel: {}", err))
            .color(0xff0000),
    )
}

/// Create an embed for when the bot fails to join a voice channel
pub fn failed_to_join_voice_channel(err: MusicError) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!("Failed to join voice channel: {}", err))
            .color(0xff0000),
    )
}

/// Create an embed for when the bot fails to process an audio source
pub fn failed_to_process_audio_source(err: MusicError) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!("Failed to process audio source: {}", err))
            .color(0xff0000),
    )
}

/// Create an embed for when the bot fails to add a track to the queue
pub fn failed_to_add_to_queue(err: MusicError) -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description(format!("Failed to add track to queue: {}", err))
            .color(0xff0000),
    )
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
                queue_length
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

/// Create an embed for when the bot fails to remove a track
pub fn failed_to_remove_track() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("Failed to remove track")
            .color(0xff0000),
    )
}

/// Create an embed for when the bot stops playing music
pub fn stopped(autoplay_enabled: bool) -> CreateReply {
    let mut embed = CreateEmbed::new()
        .title("⏹️ Stopped")
        .description("Playback stopped and queue cleared")
        .color(0x00ff00);

    // Add information about autoplay if it's enabled
    if autoplay_enabled {
        embed = embed.field(
            "Autoplay",
            "Autoplay is paused and will resume on next play",
            false,
        );
    }

    CreateReply::default().embed(embed)
}

/// Create an embed for when a track is skipped
pub fn skipped(metadata: &TrackMetadata) -> CreateReply {
    let (title, url, _) = parse_metadata(metadata);

    CreateReply::default().embed(
        CreateEmbed::new()
            .title("⏭️ Skipped")
            .description(format!("Skipped [{}]({})", title, url)),
    )
}

/// Create an embed for when there is no track to skip
pub fn no_track_to_skip() -> CreateReply {
    CreateReply::default().embed(
        CreateEmbed::new()
            .title("❌ Error")
            .description("No track is currently playing"),
    )
}
