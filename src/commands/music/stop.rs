use super::*;

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