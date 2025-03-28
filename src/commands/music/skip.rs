use std::time::Duration;
use tokio::time::sleep;

use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{self, MusicError},
    queue_manager::get_current_track,
};

/// Skip the currently playing song
#[poise::command(slash_command, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    // Stop the current track if there is one (SongEndNotifier handles the rest)
    if let Some((track, metadata)) = current_track {
        track.stop()?;
        // Send ephemeral confirmation
        ctx.send(embedded_messages::skipped(&metadata)).await?;

        // Give a moment for the next track event to potentially fire before updating
        sleep(Duration::from_millis(100)).await;
        // Update the main player message
        music_manager::send_or_update_message(ctx.serenity_context(), guild_id).await?;

    } else {
        // Send ephemeral error
        ctx.send(embedded_messages::no_track_to_skip()).await?;
    }

    Ok(())
}
