use super::*;

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