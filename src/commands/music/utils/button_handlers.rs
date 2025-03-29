use poise::serenity_prelude::{self as serenity, Context};
use serenity::ComponentInteraction;
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use super::{
    embedded_messages,
    music_manager::MusicManager,
    queue_manager::{
        self, clear_queue, get_channel_id, get_current_track, get_message_id,
        set_current_track, // Add set_current_track
        set_manual_stop_flag,
    },
    track_cache, // Import track_cache
};
use tracing::warn;

/// Handle a button interaction
pub async fn handle_button_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Defer the interaction response immediately
    interaction.defer(ctx).await?;

    let http = ctx.http.clone(); // Get http client reference

    // Ensure we're in a call
    match MusicManager::get_call(ctx, guild_id).await {
        Ok(call) => call,
        Err(_) => {
            return error_followup(ctx, interaction, "I'm not in a voice channel.").await;
        }
    };

    // Get the current track state
    let current_track_opt = get_current_track(guild_id).await?;

    match interaction.data.custom_id.as_str() {
        "music_play_pause" => {
            if let Some((track, _metadata)) = current_track_opt {
                let track_info = track.get_info().await?;
                let is_playing = track_info.playing == PlayMode::Play;

                if is_playing {
                    track.pause()?;
                } else {
                    track.play()?;
                }

                // Update the message
                update_player_message(ctx, interaction).await?;
            } else {
                error_followup(ctx, interaction, "No track is currently playing.").await?;
            }
        }
        "music_stop" => {
            // Set the manual stop flag to prevent autoplay if track ends naturally
            set_manual_stop_flag(guild_id, true).await;

            // Stop the current track if playing
            if let Some((track, _)) = current_track_opt {
                // If we successfully stopped the track or received a "track ended" error, we can continue as normal
                match track.stop() {
                    Ok(_) => (),                                        // do nothing
                    Err(songbird::error::ControlError::Finished) => (), // also do nothing
                    Err(e) => warn!("Error stopping track via button: {}", e),
                }
            }

            // Clear the queue (this also stops the update task)
            clear_queue(guild_id).await?;

            // Attempt to leave the voice channel
            if let Err(e) = MusicManager::leave_channel(ctx, guild_id).await {
                warn!("Failed to leave voice channel via button stop: {}", e);
                // Don't return error, proceed to delete message if possible
            }

            // Delete the original player message
            if let (Some(channel_id), Some(message_id)) = (
                get_channel_id(guild_id).await,
                get_message_id(guild_id).await,
            ) {
                // Check if the interaction message is the one we want to delete
                if interaction.message.id == message_id {
                    if let Err(e) = interaction.delete_response(&ctx.http).await {
                        warn!(
                            "Failed to delete player message via interaction response: {}",
                            e
                        );
                        // Fallback: try deleting the message directly
                        if let Err(e_del) = http.delete_message(channel_id, message_id, None).await
                        {
                            warn!(
                                "Failed to delete player message directly after interaction failure: {}",
                                e_del
                            );
                        }
                    }
                } else {
                    // If the interaction is somehow not on the player message, delete the stored one

                    if let Err(e) = http.delete_message(channel_id, message_id, None).await {
                        warn!(
                            "Failed to delete player message {} in channel {}: {}",
                            message_id, channel_id, e
                        );
                    }
                }
            } else {
                warn!(
                    "Could not find channel/message ID to delete for guild {}",
                    guild_id
                );
            }

            // No need to update the message as we are deleting it.
            // The interaction was already deferred, so we don't need to send a followup unless there's an error *before* this point.
        }
        "music_next" => {
            if let Some((track, _metadata)) = current_track_opt {
                // Stop the current track (SongEndNotifier will handle playing the next)
                track.stop()?;

                // Give a moment for the next track event to potentially fire
                sleep(Duration::from_millis(100)).await;

                // Update the message
                update_player_message(ctx, interaction).await?;
            } else {
                error_followup(ctx, interaction, "No track is currently playing to skip.").await?;
            }
        }
        "music_queue_toggle" => {
            // Toggle the queue view state
            queue_manager::toggle_queue_view(guild_id).await?;

            // Update the message to show/hide the queue
            update_player_message(ctx, interaction).await?;
        }
        "music_previous" => {
            // Check if there's history
            if !queue_manager::has_history(guild_id).await {
                return error_followup(ctx, interaction, "No previous track in history.").await;
            }

            // Stop the current track if one is playing
            if let Some((track, _)) = current_track_opt {
                match track.stop() {
                    Ok(_) => (),                                        // do nothing
                    Err(songbird::error::ControlError::Finished) => (), // also do nothing
                    Err(e) => warn!("Error stopping current track for 'previous': {}", e),
                }
                // Give a moment for the stop to register if needed, though get_previous_track handles queue logic
                sleep(Duration::from_millis(50)).await;
            }

            // Get the previous track metadata (this also moves current track back to queue front)
            match queue_manager::get_previous_track(guild_id).await? {
                Some(previous_metadata) => {
                    info!(
                        "Playing previous track: {} for guild {}",
                        previous_metadata.title, guild_id
                    );

                    // Get the call instance again (might be needed if connection dropped/restarted)
                    let call = match MusicManager::get_call(ctx, guild_id).await {
                        Ok(call) => call,
                        Err(_) => {
                            return error_followup(
                                ctx,
                                interaction,
                                "Lost connection to voice channel.",
                            )
                            .await;
                        }
                    };

                    // Get the URL from metadata
                    let url = match previous_metadata.url.as_ref() {
                        Some(url) => url,
                        None => {
                            error!("Previous track metadata missing URL for guild {}", guild_id);
                            return error_followup(
                                ctx,
                                interaction,
                                "Previous track is missing URL.",
                            )
                            .await;
                        }
                    };

                    // Create the audio source input
                    let input = match track_cache::create_input_from_url(url).await {
                        Ok(input) => input,
                        Err(e) => {
                            error!("Failed to create input from URL '{}': {}", url, e);
                            return error_followup(
                                ctx,
                                interaction,
                                "Failed to create audio source for previous track.",
                            )
                            .await;
                        }
                    };

                    // Play the source and get the handle
                    let track_handle = {
                        let mut call_lock = call.lock().await;
                        call_lock.play_source(input)
                    };

                    // Update the queue manager with the new current track
                    if let Err(e) =
                        set_current_track(guild_id, track_handle, previous_metadata).await
                    {
                        error!("Failed to set current track after playing previous: {}", e);
                        // Don't necessarily stop here, but log the error
                    }

                    // Update the message after successfully starting the previous track
                    update_player_message(ctx, interaction).await?;
                }
                None => {
                    // This case should ideally be caught by has_history check, but handle defensively
                    error_followup(ctx, interaction, "Could not retrieve previous track.").await?;
                    }
                }
                None => {
                    // This case should ideally be caught by has_history check, but handle defensively
                    error_followup(ctx, interaction, "Could not retrieve previous track.").await?;
                }
            }
        }
        _ => {
            error!("Unknown button ID: {}", interaction.data.custom_id);
            error_followup(ctx, interaction, "Unknown button action.").await?;
        }
    }

    Ok(())
}

/// Update the original player message after a button interaction
async fn update_player_message(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Fetch the latest player message content
    let reply = embedded_messages::music_player_message(guild_id).await?;

    // Edit the original interaction message
    interaction
        .edit_response(
            &ctx.http,
            serenity::EditInteractionResponse::new()
                .embeds(reply.embeds)
                .components(reply.components.unwrap_or_default()),
        )
        .await?;

    Ok(())
}

/// Send an ephemeral error followup message for failed interactions
async fn error_followup(
    ctx: &Context,
    interaction: &ComponentInteraction,
    content: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    interaction
        .create_followup(
            &ctx.http,
            serenity::CreateInteractionResponseFollowup::new()
                .content(content)
                .ephemeral(true),
        )
        .await?;
    Ok(())
}
