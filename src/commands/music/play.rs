use super::*;
use crate::commands::music::utils::{
    embedded_messages, music_manager::{MusicError, MusicManager}, queue_manager::store_channel_id,
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

    // Store the channel ID where the command was invoked (for potential message updates)
    store_channel_id(guild_id, ctx.channel_id()).await;

    let (metadata, number_of_tracks) = MusicManager::process_play_request(&ctx.serenity_context(), query).await?;

    // --- Generate Success Message ---
    let reply_content = if number_of_tracks > 1 {
        format!("✅ Added playlist: with {} tracks", number_of_tracks)
    } else {
        format!("✅ Added to queue: {}", metadata.title)
    };

    // Send the success message from the processing function
    ctx.send(embedded_messages::generic_success("Music", &reply_content))
        .await?;

    Ok(())
}
