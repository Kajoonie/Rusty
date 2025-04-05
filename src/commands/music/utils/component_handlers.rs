use ::serenity::all::{
    ComponentInteraction, CreateInteractionResponseFollowup, CreateQuickModal, GuildId,
};
use poise::serenity_prelude::{self as serenity, Context};
use serenity::{InputTextStyle, builder::CreateInputText};
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use super::{embedded_messages, music_manager::MusicManager};
use tracing::warn;

type ButtonInteractionResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Handle a button interaction
pub async fn handle_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // We _do not_ want to defer the response (or potentially delay with another async call) when creating
    // a modal, like how "music_search" does
    if interaction.data.custom_id != "music_search" {
        // Defer the interaction response immediately
        interaction.defer(ctx).await?;

        // Ensure we're in a call
        if let Err(_) = MusicManager::get_call(ctx, guild_id).await {
            return error_followup(ctx, interaction, "I'm not in a voice channel.").await;
        }
    }

    match interaction.data.custom_id.as_str() {
        "music_play_pause" => handle_play_pause(ctx, interaction, guild_id).await?,
        "music_eject" => handle_music_eject(ctx, interaction, guild_id).await?,
        "music_next" => handle_next(ctx, interaction, guild_id).await?,
        "music_queue_toggle" => handle_queue_toggle(ctx, interaction, guild_id).await?,
        // "music_previous" => handle_previous(ctx, interaction, guild_id).await?,
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
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the current track state
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
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
    guild_id: GuildId,
) -> ButtonInteractionResult {
    let http = ctx.http.clone(); // Get http client reference

    // Stop and clear the queue
    if let Some(queue) = MusicManager::get_queue(&guild_id).await {
        queue.stop();
    }

    // Attempt to leave the voice channel
    if let Err(e) = MusicManager::leave_channel(ctx, guild_id).await {
        warn!("Failed to leave voice channel via button stop: {}", e);
        // Don't return error, proceed to delete message if possible
    }

    // Delete the original player message
    if let (Some(channel_id), Some(message_id)) = (
        MusicManager::get_channel_id(guild_id).await,
        MusicManager::get_message_id(guild_id).await,
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

    MusicManager::drop_all(&guild_id).await;

    Ok(())
}

/// Handler for Next Track button
async fn handle_next(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the current track state
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
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
    MusicManager::toggle_queue_view(guild_id).await;

    // Update the message to show/hide the queue
    update_player_message(ctx, interaction).await
}

// /// Handler for Previous Track button
// async fn handle_previous(
//     ctx: &Context,
//     interaction: &mut ComponentInteraction,
//     guild_id: GuildId,
// ) -> ButtonInteractionResult {
//     // Check if there's history
//     if !queue_manager::has_history(guild_id).await {
//         return error_followup(ctx, interaction, "No previous track in history.").await;
//     }

//     // Get the previous track metadata *without* modifying the queue yet
//     let previous_metadata = match queue_manager::get_previous_track(guild_id).await? {
//         Some(metadata) => metadata,
//         None => {
//             // Should be caught by has_history, but handle defensively
//             return error_followup(ctx, interaction, "Could not retrieve previous track.").await;
//         }
//     };

//     info!(
//         "Attempting to play previous track: {} for guild {}",
//         previous_metadata.title, guild_id
//     );

//     // Set flag to prevent SongEndNotifier from auto-playing next
//     set_previous_action_flag(guild_id, true).await;

//     // Stop the current track *after* setting the flag
//     // We need the handle of the track *currently* playing before we stop it.
//     let current_track_handle = match get_current_track(guild_id).await? {
//         Some((handle, _)) => Some(handle),
//         None => None,
//     };

//     if let Some(track) = current_track_handle {
//         match track.stop() {
//             Ok(_) => info!(
//                 "Stopped current track to play previous for guild {}",
//                 guild_id
//             ),
//             Err(songbird::error::ControlError::Finished) => info!(
//                 "Current track already finished when trying to play previous for guild {}",
//                 guild_id
//             ),
//             Err(e) => warn!(
//                 "Error stopping current track for 'previous' action in guild {}: {}",
//                 guild_id, e
//             ),
//         }
//         // Small delay might help ensure stop event propagates if needed, but flag should be primary mechanism
//         // sleep(Duration::from_millis(50)).await;
//     } else {
//         info!(
//             "No current track playing when 'previous' was clicked for guild {}",
//             guild_id
//         );
//     }

//     // Get the call instance
//     let call = match MusicManager::get_call(ctx, guild_id).await {
//         Ok(call) => call,
//         Err(_) => {
//             // Clear flag on error path
//             clear_previous_action_flag(guild_id).await;
//             return error_followup(ctx, interaction, "Lost connection to voice channel.").await;
//         }
//     };

//     // Get the URL from metadata
//     let url = match previous_metadata.url.as_ref() {
//         Some(url) => url,
//         None => {
//             error!("Previous track metadata missing URL for guild {}", guild_id);
//             // Clear flag on error path
//             clear_previous_action_flag(guild_id).await;
//             return error_followup(ctx, interaction, "Previous track is missing URL.").await;
//         }
//     };

//     // Create the audio source input
//     let input = match track_cache::create_input_from_url(url).await {
//         Ok(input) => input,
//         Err(e) => {
//             error!("Failed to create input from URL '{}': {}", url, e);
//             // Clear flag on error path
//             clear_previous_action_flag(guild_id).await;
//             return error_followup(
//                 ctx,
//                 interaction,
//                 "Failed to create audio source for previous track.",
//             )
//             .await;
//         }
//     };

//     // Play the source and get the handle
//     let new_track_handle = {
//         let mut call_lock = call.lock().await;
//         call_lock.play_input(input)
//     };
//     info!(
//         "Started playing previous track: {} for guild {}",
//         previous_metadata.title, guild_id
//     );

//     // Update the queue manager with the new current track (this moves the old one to history)
//     // Clone metadata as it's consumed by set_current_track
//     if let Err(e) = set_current_track(
//         guild_id,
//         new_track_handle.clone(),
//         previous_metadata.clone(),
//     )
//     .await
//     // Clone metadata
//     {
//         error!(
//             "Failed to set current track after playing previous in guild {}: {}",
//             guild_id, e
//         );
//         // Don't necessarily stop here, but log the error
//     }

//     // Add SongEndNotifier to the *new* track handle
//     // let ctx_clone = ctx.clone(); // Remove unused ctx_clone
//     let call_clone = call.clone();
//     let _ = new_track_handle.add_event(
//         songbird::Event::Track(songbird::TrackEvent::End),
//         crate::commands::music::utils::event_handlers::SongEndNotifier {
//             // Use full path
//             ctx_http: ctx.http.clone(), // Use ctx_http and clone the Arc<Http>
//             guild_id,
//             call: call_clone,
//             track_metadata: previous_metadata, // Use the cloned metadata again
//         },
//     );

//     // Clear the flag now that the new track is playing and state is updated
//     clear_previous_action_flag(guild_id).await;

//     // Update the message after successfully starting the previous track
//     update_player_message(ctx, interaction).await
// }

/// Handler for Search button - presents a modal
async fn handle_search(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    info!(
        "Handling search button interaction for user {}",
        interaction.user.id
    );

    let input_text = CreateInputText::new(
        InputTextStyle::Short,
        "URL or Search Query",
        "search_query_input",
    )
    .placeholder("Enter a song URL or search term...")
    .required(true);

    let modal = CreateQuickModal::new("Add Track to Queue")
        .timeout(Duration::from_secs(300))
        .field(input_text);

    let response = interaction.quick_modal(ctx, modal).await?;

    if let Some(response) = response {
        let interaction = response.interaction;
        // Defer the response immediately
        interaction.defer(&ctx.http).await?;

        // This modal only has a single input field
        let input = response.inputs[0].clone();

        match MusicManager::process_play_request(
            &ctx,
            interaction.guild_id.unwrap(),
            interaction.channel_id,
            &interaction.user,
            input,
        )
        .await
        {
            Ok((metadata, number_of_tracks)) => {
                let response = MusicManager::play_success_response(metadata, number_of_tracks);

                interaction
                    .create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .embeds(response.embeds)
                            .ephemeral(true),
                    )
                    .await?;
            }
            Err(e) => {
                let response = embedded_messages::generic_error(&e.to_string());

                interaction
                    .create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .embeds(response.embeds)
                            .ephemeral(true),
                    )
                    .await?;
            }
        }
    }

    Ok(())
}

/// Update the original player message after a button interaction
async fn update_player_message(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> ButtonInteractionResult {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    let player_message_data = MusicManager::get_player_message_data(guild_id).await;

    // Fetch the latest player message content
    let reply = embedded_messages::music_player_message(player_message_data).await?;

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
) -> ButtonInteractionResult {
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
