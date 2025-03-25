use super::*;
use crate::commands::music::utils::{
    event_handlers::play_next_track,
    music_manager::{MusicError, MusicManager},
    queue_manager::{get_current_track, get_next_track},
};
use std::time::Duration;

/// Skip the currently playing song
#[poise::command(slash_command, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current voice call
    let call = match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        Ok(call) => call,
        Err(err) => {
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("❌ Error")
                        .description(format!("Not connected to a voice channel: {}", err))
                        .color(0xff0000),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    // Stop the current track if there is one
    if let Some((track, _)) = current_track {
        track.stop()?;
    }

    // Get the next track from the queue and play it
    match play_next_track(ctx.serenity_context(), guild_id, call.clone()).await {
        Ok(true) => {
            // Successfully started playing next track
            let next_track = get_next_track(guild_id).await?;
            match next_track {
                Some(queue_item) => {
                    // Send a success message
                    let title = queue_item.metadata.title.clone();
                    let url = queue_item
                        .metadata
                        .url
                        .clone()
                        .unwrap_or_else(|| "#".to_string());
                    let duration_str = queue_item
                        .metadata
                        .duration
                        .map(format_duration)
                        .unwrap_or_else(|| "Unknown duration".to_string());

                    let mut embed = CreateEmbed::new()
                        .title("⏭️ Now Playing")
                        .description(format!("[{}]({})", title, url))
                        .field("Duration", format!("`{}`", duration_str), true)
                        .color(0x00ff00);

                    // Add thumbnail if available
                    if let Some(thumbnail) = queue_item.metadata.thumbnail {
                        embed = embed.thumbnail(thumbnail);
                    }

                    ctx.send(CreateReply::default().embed(embed)).await?;
                }
                None => {
                    // This shouldn't happen since play_next_track returned true
                    ctx.send(
                        CreateReply::default().embed(
                            CreateEmbed::new()
                                .title("❓ Unexpected Error")
                                .description("Track was played but queue information is missing")
                                .color(0xff0000),
                        ),
                    )
                    .await?;
                }
            }
        }
        Ok(false) => {
            // Queue is empty
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("⏭️ Queue Empty")
                        .description("No more tracks in the queue")
                        .color(0xffaa00),
                ),
            )
            .await?;
        }
        Err(err) => {
            // Error playing next track
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("❌ Error")
                        .description(format!("Failed to play next track: {}", err))
                        .color(0xff0000),
                ),
            )
            .await?;
        }
    }

    Ok(())
}

/// Format a duration into a human-readable string
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    if minutes >= 60 {
        let hours = minutes / 60;
        let minutes = minutes % 60;
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}
