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
    if let Some((track_handle, metadata)) = &current_track {
        let track_info = track_handle.get_info().await.ok();
        let position = track_info.as_ref().map(|info| info.position);
        
        description.push_str("**🎵 Now Playing**\n");
        description.push_str(&format!("**[{}]({})**\n", 
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

        description.push_str("\n");
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
            
            description.push_str(&format!("{} [{}]({})", 
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
        let total_duration: Duration = queue.iter()
            .filter_map(|track| track.duration)
            .sum();
        if total_duration.as_secs() > 0 {
            description.push_str(&format!("\n**⏱️ Total Duration:** `{}`", format_duration(total_duration)));
        }
    }

    // Create and send the embed
    let mut embed = CreateEmbed::new()
        .title("🎵 Music Queue")
        .description(description)
        .color(0x00ff00);

    // Add thumbnail if available from current track
    if let Some((_, metadata)) = &current_track {
        if let Some(thumbnail) = &metadata.thumbnail {
            embed = embed.thumbnail(thumbnail);
        }
    }

    ctx.send(CreateReply::default()
        .embed(embed)
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

    format!("▬{}🔘{}▬",
        "▬".repeat(filled),
        "▬".repeat(empty)
    )
}