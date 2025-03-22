use poise::{serenity_prelude as serenity, CreateReply};
use serenity::all::CreateEmbed;

use crate::{CommandResult, Context};

/// View the current music queue
#[poise::command(slash_command, category = "Music")]
pub async fn queue(ctx: Context<'_>) -> CommandResult {
    // For now, just acknowledge that we received the command
    ctx.send(CreateReply::default()
        .embed(CreateEmbed::new()
            .title("🎵 Music Queue")
            .description("The queue is currently empty")
            .color(0x00ff00))
        .ephemeral(false))
        .await?;

    Ok(())
}