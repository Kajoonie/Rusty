use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{MusicError, MusicManager},
    queue_manager::get_current_track,
};
use songbird::tracks::PlayMode;

/// Pause or resume the current track
#[poise::command(slash_command, category = "Music")]
pub async fn pause(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Ensure we're in a call
    if let Err(err) = MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        ctx.send(embedded_messages::bot_not_in_voice_channel(err)).await?;
        return Ok(());
    }

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    match current_track {
        Some((track, metadata)) => {
            let track_info = track.get_info().await?;

            match track_info.playing {
                PlayMode::Play => {
                    track.pause()?;
                    ctx.send(embedded_messages::paused(&metadata)).await?;
                }
                PlayMode::Pause => {
                    track.play()?;
                    ctx.send(embedded_messages::resumed(&metadata)).await?;
                }
                _ => {
                    ctx.send(embedded_messages::not_pausable()).await?;
                }
            }
        }
        None => {
            ctx.send(embedded_messages::no_track_playing()).await?;
        }
    }

    Ok(())
}
