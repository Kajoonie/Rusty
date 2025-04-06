//! Defines the `/play` command for adding songs or playlists to the music queue.

use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{MusicError, MusicManager},
};
use tracing::info;

/// Adds a song or playlist to the music queue using a URL or search query.
///
/// Supports YouTube URLs, Spotify URLs (if feature enabled), and general search terms
/// which will be searched on YouTube. Handles joining the voice channel if necessary.
/// Delegates processing to `MusicManager::process_play_request`.
#[poise::command(slash_command, prefix_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"]
    #[rest]
    query: String,
) -> CommandResult {
    // Defer the response ephemerally initially, as processing might take time.
    ctx.defer_ephemeral().await?;

    info!("Received /play command with query: {}", query);
    // Ensure the command is used within a guild.
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    // Delegate the core logic (joining channel, searching/fetching track, adding to queue) to the MusicManager.
    match MusicManager::process_play_request(
        ctx.serenity_context(),
        guild_id,
        ctx.channel_id(),
        ctx.author(),
        query,
    )
    .await
    {
        Ok((metadata, number_of_tracks)) => {
            // On success, send a confirmation message (e.g., 'Added X tracks to queue').
            ctx.send(MusicManager::play_success_response(
                metadata,
                number_of_tracks,
            ))
            .await?;
        }
        Err(e) => {
            // On error, send a generic error message.
            ctx.send(embedded_messages::generic_error(&e.to_string()))
                .await?;
        }
    };

    Ok(())
}
