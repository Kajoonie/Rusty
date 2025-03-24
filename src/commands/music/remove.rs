use super::*;
use crate::commands::music::utils::{
    music_manager::MusicError,
    queue_manager::{remove_track, get_queue},
};

/// Remove a track from the queue by its position
#[poise::command(slash_command, category = "Music")]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Position of the track to remove (1-based)"] position: usize,
) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Convert to 0-based index
    let index = position - 1;

    // Get the current queue length for validation
    let queue = get_queue(guild_id).await?;
    if queue.is_empty() {
        ctx.send(CreateReply::default()
            .embed(CreateEmbed::new()
                .title("❌ Error")
                .description("The queue is empty")
                .color(0xff0000)))
            .await?;
        return Ok(());
    }

    if index >= queue.len() {
        ctx.send(CreateReply::default()
            .embed(CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Invalid position. The queue has {} tracks", queue.len()))
                .color(0xff0000)))
            .await?;
        return Ok(());
    }

    // Remove the track
    if let Some(removed_track) = remove_track(guild_id, index).await? {
        ctx.send(CreateReply::default()
            .embed(CreateEmbed::new()
                .title("🗑️ Track Removed")
                .description(format!("Removed [{}]({}) from position #{}", 
                    removed_track.title,
                    removed_track.url.unwrap_or_else(|| "#".to_string()),
                    position))
                .color(0x00ff00)))
            .await?;
    } else {
        ctx.send(CreateReply::default()
            .embed(CreateEmbed::new()
                .title("❌ Error")
                .description("Failed to remove track")
                .color(0xff0000)))
            .await?;
    }

    Ok(())
} 