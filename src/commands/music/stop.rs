use super::*;
use crate::commands::music::utils::{
    autoplay_manager::is_autoplay_enabled,
    embedded_messages,
    music_manager::{self, MusicError, MusicManager},
    queue_manager::{clear_queue, get_current_track, set_manual_stop_flag},
};
use poise::CreateReply;

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
            ctx.send(embedded_messages::bot_not_in_voice_channel(err))
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

    // Send ephemeral success message
    ctx.send(embedded_messages::stopped()).await?;

    // Update the main player message
    music_manager::send_or_update_message(ctx.serenity_context(), guild_id).await?;

    Ok(())
}
