use poise::{serenity_prelude as serenity, CreateReply};
use serenity::all::CreateEmbed;

use crate::{CommandResult, Context};

/// Skip the currently playing song
#[poise::command(slash_command, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> CommandResult {
    // For now, just acknowledge that we received the command
    ctx.send(CreateReply::default()
        .embed(CreateEmbed::new()
            .title("⏭️ Skipped Track")
            .description("Skipped to the next track")
            .color(0x00ff00))
        .ephemeral(false))
        .await?;

    Ok(())
}

/// Stop the music and clear the queue
#[poise::command(slash_command, category = "Music")]
pub async fn stop(ctx: Context<'_>) -> CommandResult {
    // For now, just acknowledge that we received the command
    ctx.send(CreateReply::default()
        .content("🛑 Stopped playback and cleared the queue")
        .ephemeral(false))
        .await?;

    Ok(())
}

/// Leave the voice channel
#[poise::command(slash_command, category = "Music")]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    // For now, just acknowledge that we received the command
    ctx.send(CreateReply::default()
        .content("👋 Left the voice channel")
        .ephemeral(false))
        .await?;

    Ok(())
}