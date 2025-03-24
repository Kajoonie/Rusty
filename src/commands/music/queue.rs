use super::*;
use crate::commands::music::utils::queue_manager::{get_current_track, get_queue};
use crate::commands::music::utils::music_manager::MusicError;
use std::time::Duration;

/// View the current music queue
#[poise::command(slash_command, category = "Music")]
pub async fn queue(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id()
        .ok_or_else(|| Box::new(MusicError::NotInGuild))?;

    // Get the current track and queue
    let current_track = get_current_track(guild_id).await?;
    let queue = get_queue(guild_id).await?;

    // Build the queue display
    let mut description = String::new();

    // Add current track information if there is one playing
    let has_current_track = if let Some((_, metadata)) = &current_track {
        description.push_str("**Now Playing:**\n");
        description.push_str(&format!("🎵 {}\n", metadata.title));
        if let Some(duration) = metadata.duration {
            description.push_str(&format!("⏱️ Duration: {}\n", format_duration(duration)));
        }
        description.push_str("\n");
        true
    } else {
        false
    };

    // Add upcoming tracks
    if queue.is_empty() {
        if !has_current_track {
            description.push_str("The queue is currently empty");
        } else {
            description.push_str("**No songs in queue**");
        }
    } else {
        description.push_str("**Up Next:**\n");
        for (index, track) in queue.iter().enumerate() {
            description.push_str(&format!("{}. {}", index + 1, track.title));
            if let Some(duration) = track.duration {
                description.push_str(&format!(" (⏱️ {})", format_duration(duration)));
            }
            description.push('\n');
        }
    }

    // Create and send the embed
    ctx.send(CreateReply::default()
        .embed(CreateEmbed::new()
            .title("🎵 Music Queue")
            .description(description)
            .color(0x00ff00))
        .ephemeral(false))
        .await?;

    Ok(())
}

/// Format a duration into a human-readable string (e.g., "3:45" or "1:23:45")
fn format_duration(duration: Duration) -> String {
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