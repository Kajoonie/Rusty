use ::serenity::all::GuildId;
use poise::serenity_prelude::{self as serenity, Context};
use serenity::{
    builder::{CreateActionRow, CreateInputText, CreateInteractionResponse, CreateModal},
    ComponentInteraction, InputTextStyle,
};
use songbird::tracks::{PlayMode, TrackHandle};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use super::{
    audio_sources::TrackMetadata,
    embedded_messages,
    music_manager::MusicManager,
    queue_manager::{
        self, clear_previous_action_flag, clear_queue, get_channel_id, get_current_track,
        get_message_id, set_current_track, set_manual_stop_flag, set_previous_action_flag,
    },
    track_cache, // Import track_cache
};
use tracing::warn;

type ButtonInteractionResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Handle a button interaction
pub async fn handle_button_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Defer the interaction response immediately
    interaction.defer(ctx).await?;

    // Ensure we're in a call
    if let Err(_) = MusicManager::get_call(ctx, guild_id).await {
        return error_followup(ctx, interaction, "I'm not in a voice channel.").await;
    }

    // Get the current track state
    let current_track_opt = get_current_track(guild_id).await?;

    match interaction.data.custom_id.as_str() {
        "music_play_pause" => handle_play_pause(ctx, interaction, current_track_opt).await?,
        "music_eject" => handle_music_eject(ctx, interaction, current_track_opt, guild_id).await?,
        "music_next" => handle_next(ctx, interaction, current_track_opt).await?,
        "music_queue_toggle" => handle_queue_toggle(ctx, interaction, guild_id).await?,
        "music_previous" => handle_previous(ctx, interaction, guild_id).await?,
        "music_search" => handle_search(ctx, interaction).await?,
        // Add cases for repeat and shuffle later
        _ => {
            error!("Unknown button ID: {}", interaction.data.custom_id);
            error_followup(ctx, interaction, "Unknown button action.").await?;
        }
    }

    Ok(())
}

/// Handler for alternating Play/Pause button
async fn handle_play_pause(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    current_track_opt: Option<(TrackHandle, TrackMetadata)>,
) -> ButtonInteractionResult {
    if let Some((track, _metadata)) = current_track_opt {
        let track_info = track.get_info().await?;
        let is_playing = track_info.playing == PlayMode::Play;

        if is_playing {
            track.pause()?;
        } else {
            track.play()?;
        }

        // Update the message
        update_player_message(ctx, interaction).await
    } else {
        error_followup(ctx, interaction, "No track is currently playing.").await
    }
}

/// Handler for Eject button
async fn handle_music_eject(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    current_track_opt: Option<(TrackHandle, TrackMetadata)>,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    let http = ctx.http.clone(); // Get http client reference

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
                if let Err(e_del) = http.delete_message(channel_id, message_id, None).await {
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
    Ok(())
}

/// Handler for Next Track button
async fn handle_next(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    current_track_opt: Option<(TrackHandle, TrackMetadata)>,
) -> ButtonInteractionResult {
    if let Some((track, _metadata)) = current_track_opt {
        // Stop the current track (SongEndNotifier will handle playing the next)
        track.stop()?;

        // Give a moment for the next track event to potentially fire
        sleep(Duration::from_millis(100)).await;

        // Update the message
        update_player_message(ctx, interaction).await
    } else {
        error_followup(ctx, interaction, "No track is currently playing to skip.").await
    }
}

/// Handler for Queue button
async fn handle_queue_toggle(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Toggle the queue view state
    queue_manager::toggle_queue_view(guild_id).await?;

    // Update the message to show/hide the queue
    update_player_message(ctx, interaction).await
}

/// Handler for Previous Track button
async fn handle_previous(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Check if there's history
    if !queue_manager::has_history(guild_id).await {
        return error_followup(ctx, interaction, "No previous track in history.").await;
    }

    // Get the previous track metadata *without* modifying the queue yet
    let previous_metadata = match queue_manager::get_previous_track(guild_id).await? {
        Some(metadata) => metadata,
        None => {
            // Should be caught by has_history, but handle defensively
            return error_followup(ctx, interaction, "Could not retrieve previous track.")
                .await;
        }
    };

    info!(
        "Attempting to play previous track: {} for guild {}",
        previous_metadata.title, guild_id
    );

    // Set flag to prevent SongEndNotifier from auto-playing next
    set_previous_action_flag(guild_id, true).await;

    // Stop the current track *after* setting the flag
    // We need the handle of the track *currently* playing before we stop it.
    let current_track_handle = match get_current_track(guild_id).await? {
        Some((handle, _)) => Some(handle),
        None => None,
    };

    if let Some(track) = current_track_handle {
        match track.stop() {
            Ok(_) => info!(
                "Stopped current track to play previous for guild {}",
                guild_id
            ),
            Err(songbird::error::ControlError::Finished) => info!(
                "Current track already finished when trying to play previous for guild {}",
                guild_id
            ),
            Err(e) => warn!(
                "Error stopping current track for 'previous' action in guild {}: {}",
                guild_id, e
            ),
        }
        // Small delay might help ensure stop event propagates if needed, but flag should be primary mechanism
        // sleep(Duration::from_millis(50)).await;
    } else {
        info!(
            "No current track playing when 'previous' was clicked for guild {}",
            guild_id
        );
    }

    // Get the call instance
    let call = match MusicManager::get_call(ctx, guild_id).await {
        Ok(call) => call,
        Err(_) => {
            // Clear flag on error path
            clear_previous_action_flag(guild_id).await;
            return error_followup(ctx, interaction, "Lost connection to voice channel.")
                .await;
        }
    };

    // Get the URL from metadata
    let url = match previous_metadata.url.as_ref() {
        Some(url) => url,
        None => {
            error!("Previous track metadata missing URL for guild {}", guild_id);
            // Clear flag on error path
            clear_previous_action_flag(guild_id).await;
            return error_followup(ctx, interaction, "Previous track is missing URL.")
                .await;
        }
    };

    // Create the audio source input
    let input = match track_cache::create_input_from_url(url).await {
        Ok(input) => input,
        Err(e) => {
            error!("Failed to create input from URL '{}': {}", url, e);
            // Clear flag on error path
            clear_previous_action_flag(guild_id).await;
            return error_followup(
                ctx,
                interaction,
                "Failed to create audio source for previous track.",
            )
            .await;
        }
    };

    // Play the source and get the handle
    let new_track_handle = {
        let mut call_lock = call.lock().await;
        call_lock.play_input(input)
    };
    info!(
        "Started playing previous track: {} for guild {}",
        previous_metadata.title, guild_id
    );

    // Update the queue manager with the new current track (this moves the old one to history)
    // Clone metadata as it's consumed by set_current_track
    if let Err(e) = set_current_track(
        guild_id,
        new_track_handle.clone(),
        previous_metadata.clone(),
    )
    .await
    // Clone metadata
    {
        error!(
            "Failed to set current track after playing previous in guild {}: {}",
            guild_id, e
        );
        // Don't necessarily stop here, but log the error
    }

    // Add SongEndNotifier to the *new* track handle
    let ctx_clone = ctx.clone();
    let call_clone = call.clone();
    let _ = new_track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        crate::commands::music::utils::event_handlers::SongEndNotifier {
            // Use full path
            ctx_http: ctx.http.clone(), // Use ctx_http and clone the Arc<Http>
            guild_id,
            call: call_clone,
            track_metadata: previous_metadata, // Use the cloned metadata again
        },
    );

    // Clear the flag now that the new track is playing and state is updated
    clear_previous_action_flag(guild_id).await;

    // Update the message after successfully starting the previous track
    update_player_message(ctx, interaction).await
}

/// Handler for Search button - presents a modal
async fn handle_search(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    info!(
        "Handling search button interaction for user {}",
        interaction.user.id
    );

    let input_text = CreateInputText::new(InputTextStyle::Short, "URL or Search Query")
        .custom_id("search_query_input") // Unique ID for the text input
        .placeholder("Enter a song URL or search term...")
        .required(true);

    let modal = CreateModal::new("music_search_modal", "Add Track to Queue") // Unique ID for the modal
        .components(vec![CreateActionRow::InputText(input_text)]);

    // Respond to the interaction by showing the modal
    interaction
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;

    Ok(()) // Modal presentation itself is the success case here
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
