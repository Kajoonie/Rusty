use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{MusicError, MusicManager},
};
use tracing::info;

/// Play a song from YouTube or a direct URL
#[poise::command(slash_command, prefix_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"]
    #[rest]
    query: String,
) -> CommandResult {
    // Defer the response ephemerally
    ctx.defer_ephemeral().await?;

    info!("Received /play command with query: {}", query);
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    match MusicManager::process_play_request(
        &ctx.serenity_context(),
        guild_id,
        ctx.channel_id(),
        ctx.author(),
        query,
    )
    .await
    {
        Ok((metadata, number_of_tracks)) => {
            ctx.send(MusicManager::play_success_response(
                metadata,
                number_of_tracks,
            ))
            .await?;
        }
        Err(e) => {
            ctx.send(embedded_messages::generic_error(&e.to_string()))
                .await?;
        }
    };

    Ok(())
}
