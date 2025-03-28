use super::*;
use crate::commands::music::utils::{
    embedded_messages,
    music_manager::{MusicError, MusicManager},
    queue_manager::{
        clear_queue, get_channel_id, get_current_track, get_message_id, set_manual_stop_flag,
    },
};

/// Stop the music, clear the queue, and leave the voice channel
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

    // Set the manual stop flag to prevent autoplay from triggering
    set_manual_stop_flag(guild_id, true).await;

    // Get the current track
    let current_track = get_current_track(guild_id).await?;

    // Stop the current track if there is one
    if let Some((track, _)) = current_track {
        track.stop()?;
    }

    // Clear the queue (this also stops the update task)
    clear_queue(guild_id).await?;

    // Attempt to leave the voice channel
    if let Err(e) = MusicManager::leave_channel(ctx.serenity_context(), guild_id).await {
        // Log the error but continue, as the main goal (stopping) is achieved
        tracing::warn!("Failed to leave voice channel during stop: {}", e);
    }

    // Delete the existing player message if possible
    if let (Some(channel_id), Some(message_id)) = (
        get_channel_id(guild_id).await,
        get_message_id(guild_id).await,
    ) {
        if let Err(e) = ctx
            .http()
            .delete_message(channel_id, message_id, None)
            .await
        {
            tracing::warn!(
                "Failed to delete player message {} in channel {}: {}",
                message_id,
                channel_id,
                e
            );
        }
    }

    // Send ephemeral success message
    ctx.send(embedded_messages::stopped()).await?;

    // No need to update the message as we are leaving/deleting it

    Ok(())
}
