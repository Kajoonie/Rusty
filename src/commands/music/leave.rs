use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{MusicError, MusicManager},
    queue_manager::clear_queue,
};

/// Leave the voice channel
#[poise::command(slash_command, category = "Music")]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Try to leave the voice channel
    match MusicManager::leave_channel(ctx.serenity_context(), guild_id).await {
        Ok(_) => {
            // Clear the queue
            clear_queue(guild_id).await?;

            // Send success message
            ctx.send(embedded_messages::left_voice_channel()).await?;
        }
        Err(err) => {
            // Send error message
            ctx.send(embedded_messages::failed_to_leave_voice_channel(err))
                .await?;
        }
    }

    Ok(())
}
