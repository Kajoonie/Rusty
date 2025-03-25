use super::*;
use crate::commands::music::utils::{
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
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("👋 Left Voice Channel")
                        .description("Successfully disconnected and cleared the queue")
                        .color(0x00ff00),
                ),
            )
            .await?;
        }
        Err(err) => {
            // Send error message
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("❌ Error")
                        .description(format!("Failed to leave voice channel: {}", err))
                        .color(0xff0000),
                ),
            )
            .await?;
        }
    }

    Ok(())
}
