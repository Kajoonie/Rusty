use super::*;
use crate::commands::music::utils::embedded_messages;
use crate::commands::music::utils::music_manager::MusicError;
use crate::commands::music::utils::queue_manager::{get_current_track, get_queue};

/// View the current music queue
#[poise::command(slash_command, category = "Music")]
pub async fn queue(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Box::new(MusicError::NotInGuild))?;

    // Get the current track and queue
    let current_track = get_current_track(guild_id).await?;
    let queue = get_queue(guild_id).await?;

    let mut embed = embedded_messages::music_queue(&current_track, &queue).await;

    // Add thumbnail if available from current track
    if let Some((_, metadata)) = &current_track {
        if let Some(thumbnail) = &metadata.thumbnail {
            embed = embed.thumbnail(thumbnail);
        }
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}
