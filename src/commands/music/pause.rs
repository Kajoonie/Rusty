use super::*;
use crate::commands::music::utils::{
    music_manager::{MusicManager, MusicError},
    queue_manager::get_current_track,
};
use songbird::tracks::PlayMode;

/// Pause or resume the current track
#[poise::command(slash_command, category = "Music")]
pub async fn pause(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current voice call
    let _call = match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        Ok(call) => call,
        Err(err) => {
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("❌ Error")
                    .description(format!("Not connected to a voice channel: {}", err))
                    .color(0xff0000)))
                .await?;
            return Ok(());
        }
    };

    // Get the current track
    let current_track = get_current_track(guild_id).await?;
    
    match current_track {
        Some((track, metadata)) => {
            let track_info = track.get_info().await?;
            
            match track_info.playing {
                PlayMode::Play => {
                    track.pause()?;
                    ctx.send(CreateReply::default()
                        .embed(CreateEmbed::new()
                            .title("⏸️ Paused")
                            .description(format!("Paused [{}]({})",
                                metadata.title,
                                metadata.url.as_deref().unwrap_or("#")))
                            .color(0x00ff00)))
                        .await?;
                },
                PlayMode::Pause => {
                    track.play()?;
                    ctx.send(CreateReply::default()
                        .embed(CreateEmbed::new()
                            .title("▶️ Resumed")
                            .description(format!("Resumed [{}]({})",
                                metadata.title,
                                metadata.url.as_deref().unwrap_or("#")))
                            .color(0x00ff00)))
                        .await?;
                },
                _ => {
                    ctx.send(CreateReply::default()
                        .embed(CreateEmbed::new()
                            .title("❌ Error")
                            .description("The track is not in a pausable state")
                            .color(0xff0000)))
                        .await?;
                }
            }
        },
        None => {
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("❌ Error")
                    .description("No track is currently playing")
                    .color(0xff0000)))
                .await?;
        }
    }

    Ok(())
} 