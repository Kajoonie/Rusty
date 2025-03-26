use super::*;
use crate::commands::music::utils::{
    embedded_messages, music_manager::MusicError, queue_manager::get_current_track,
};

/// Skip the currently playing song
#[poise::command(slash_command, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    // Stop the current track if there is one
    if let Some((track, metadata)) = current_track {
        track.stop()?;
        ctx.send(embedded_messages::skipped(&metadata)).await?;
    } else {
        ctx.send(embedded_messages::no_track_to_skip()).await?;
    }

    Ok(())
}
