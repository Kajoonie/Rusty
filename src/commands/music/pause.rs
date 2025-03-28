use std::time::Duration;
use tokio::time::sleep;

use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{self, MusicError, MusicManager},
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
        ctx.send(embedded_messages::bot_not_in_voice_channel(err))
            .await?;
        return Ok(());
    }

    // Get the current track
    let current_track_opt = get_current_track(guild_id).await?;

    match current_track_opt {
        Some((track, metadata)) => {
            let track_info = track.get_info().await?;
            let current_state = track_info.playing;

            let action_result = match current_state {
                PlayMode::Play => track.pause(),
                PlayMode::Pause => track.play(),
                _ => Err(songbird::tracks::ControlError::Finished), // Treat other states as not pausable/resumable
            };

            match action_result {
                Ok(_) => {
                    let message = if current_state == PlayMode::Play {
                        format!("⏸️ Paused: {}", metadata.title)
                    } else {
                        format!("▶️ Resumed: {}", metadata.title)
                    };
                    // Send ephemeral confirmation
                    ctx.send(embedded_messages::generic_success("Music", &message))
                        .await?;

                    // Update the main player message after a short delay
                    sleep(Duration::from_millis(100)).await;
                    music_manager::send_or_update_message(ctx.serenity_context(), guild_id).await?;
                }
                Err(_) => {
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
