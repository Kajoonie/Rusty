use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::MusicError,
    queue_manager::{get_queue, remove_track},
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
        ctx.send(embedded_messages::queue_is_empty()).await?;
        return Ok(());
    }

    if index >= queue.len() {
        ctx.send(embedded_messages::invalid_queue_position(queue.len()))
            .await?;
        return Ok(());
    }

    // Remove the track
    if let Some(removed_track) = remove_track(guild_id, index).await? {
        ctx.send(embedded_messages::track_removed(&removed_track, position))
            .await?;
    } else {
        ctx.send(embedded_messages::failed_to_remove_track())
            .await?;
    }

    Ok(())
}
