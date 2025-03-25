use super::*;
use crate::commands::music::utils::{
    autoplay_manager::is_autoplay_enabled,
    music_manager::{MusicError, MusicManager},
    queue_manager::{clear_queue, get_current_track, set_manual_stop_flag},
};

/// Stop the music and clear the queue
#[poise::command(slash_command, category = "Music")]
pub async fn stop(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current voice call
    match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
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

    // Check if autoplay is enabled to show in the message
    let autoplay_is_enabled = is_autoplay_enabled(guild_id).await;

    // Set the manual stop flag to prevent autoplay from triggering
    set_manual_stop_flag(guild_id, true).await;

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    // Stop the current track if there is one
    if let Some((track, _)) = current_track {
        track.stop()?;
    }

    // Clear the queue
    clear_queue(guild_id).await?;

    // Send success message
    let mut embed = CreateEmbed::new()
        .title("⏹️ Stopped")
        .description("Playback stopped and queue cleared")
        .color(0x00ff00);

    // Add information about autoplay if it's enabled
    if autoplay_is_enabled {
        embed = embed.field(
            "Autoplay",
            "Autoplay is paused and will resume on next play",
            false,
        );
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}
