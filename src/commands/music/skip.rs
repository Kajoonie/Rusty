use super::*;

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