//! Handles component interactions (button clicks) for the music player controls.
//! Routes interactions based on custom IDs to specific handler functions.

use ::serenity::all::{
    ComponentInteraction, CreateInteractionResponseFollowup, CreateQuickModal, GuildId,
};
use poise::serenity_prelude::{self as serenity, Context};
use serenity::{InputTextStyle, builder::CreateInputText};
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::commands::music::audio_sources::track_metadata::TrackMetadata;

use super::{button_controls::RepeatState, embedded_messages, music_manager::MusicManager};
use tracing::warn;

/// A specialized `Result` type for button interaction handlers.
type ButtonInteractionResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The main entry point for handling music-related component interactions.
///
/// It defers the interaction, checks if the bot is in a voice channel (except for search),
/// and routes the interaction to the appropriate handler based on the `custom_id`.
pub async fn handle_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    // Ensure the interaction happened within a guild.
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Don't defer immediately if opening a modal (search).
    if interaction.data.custom_id != "music_search" {
        // Acknowledge the interaction quickly.
        interaction.defer(ctx).await?;

        // Check if the bot is connected to a voice channel in this guild.
        if let Err(_) = MusicManager::get_call(ctx, guild_id).await {
            // Send an error message if not connected.
            return error_followup(ctx, interaction, "I'm not in a voice channel.").await;
        }
    }

    // Match the custom ID to the corresponding handler function.
    match interaction.data.custom_id.as_str() {
        "music_play_pause" => handle_play_pause(ctx, interaction, guild_id).await?,
        "music_eject" => handle_music_eject(ctx, interaction, guild_id).await?,
        "music_next" => handle_next(ctx, interaction, guild_id).await?,
        "music_queue_toggle" => handle_queue_toggle(ctx, interaction, guild_id).await?,
        "music_search" => handle_search(ctx, interaction).await?,
        "music_repeat" => handle_repeat(ctx, interaction, guild_id).await?,
        "music_shuffle" => handle_shuffle(ctx, interaction, guild_id).await?,
        _ => {
            error!("Unknown button ID: {}", interaction.data.custom_id);
            // Handle unknown button IDs.
            error_followup(ctx, interaction, "Unknown button action.").await?;
        }
    }

    Ok(())
}

/// Handles the play/pause button interaction.
/// Toggles the playback state of the current track and updates the player message.
async fn handle_play_pause(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the handle for the currently playing track.
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
        // Get the current playback state (playing or paused).
        let track_info = track.get_info().await?;
        let is_playing = track_info.playing == PlayMode::Play;

        // Toggle the state.
        if is_playing {
            track.pause()?;
        } else {
            track.play()?;
        }

        // Update the original message with new buttons/embed.
        update_player_message(ctx, interaction).await
    } else {
        // Send error if no track is playing.
        error_followup(ctx, interaction, "No track is currently playing.").await
    }
}

/// Handles the eject button interaction.
/// Stops playback, clears the queue, leaves the voice channel, and deletes the player message.
async fn handle_music_eject(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Clone HTTP client for potential direct message deletion.
    let http = ctx.http.clone(); // Get http client reference

    // Stop playback and clear the queue.
    if let Some(queue) = MusicManager::get_queue(&guild_id).await {
        queue.stop();
    }

    // Attempt to leave the voice channel.
    if let Err(e) = MusicManager::leave_channel(ctx, guild_id).await {
        warn!("Failed to leave voice channel via button stop: {}", e);
        // Don't return error, proceed to delete message if possible
    }

    // Try to delete the original player message.
    if let (Some(channel_id), Some(message_id)) = (
        MusicManager::get_channel_id(guild_id).await,
        MusicManager::get_message_id(guild_id).await,
    ) {
        // Check if the interaction message is the one we want to delete
        if interaction.message.id == message_id {
            // Try deleting via the interaction response first.
            if let Err(e) = interaction.delete_response(&ctx.http).await {
                warn!(
                    "Failed to delete player message via interaction response: {}",
                    e
                );
                // If interaction deletion fails, try direct deletion.
                if let Err(e_del) = http.delete_message(channel_id, message_id, None).await {
                    warn!(
                        "Failed to delete player message directly after interaction failure: {}",
                        e_del
                    );
                }
            }
        } else {
            // If interaction isn't on the player message, delete the stored message ID directly.
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

    // Clean up any remaining guild-specific data in the MusicManager.
    MusicManager::drop_all(&guild_id).await;

    Ok(())
}

/// Handles the next track (skip) button interaction.
/// Stops the current track, allowing the `SongEndNotifier` to play the next one.
/// Updates the player message.
async fn handle_next(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the handle for the currently playing track.
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
        // Stop the track. The event handler will trigger the next song.
        track.stop()?;

        // Short delay to allow Songbird event handlers to potentially update state.
        sleep(Duration::from_millis(100)).await;

        // Update the original message.
        update_player_message(ctx, interaction).await
    } else {
        // Send error if no track is playing.
        error_followup(ctx, interaction, "No track is currently playing to skip.").await
    }
}

/// Handles the queue toggle button interaction.
/// Toggles the visibility state of the queue in the player message and updates it.
async fn handle_queue_toggle(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Flip the boolean state for showing the queue.
    MusicManager::toggle_queue_view(guild_id).await;

    // Update the original message.
    update_player_message(ctx, interaction).await
}

/// Handles the search button interaction.
/// Presents a modal window to the user to input a search query or URL.
async fn handle_search(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> ButtonInteractionResult {
    info!(
        "Handling search button interaction for user {}",
        interaction.user.id
    );

    // Define the text input field for the modal.
    let input_text = CreateInputText::new(
        InputTextStyle::Short,
        "URL or Search Query",
        "search_query_input",
    )
    .placeholder("Enter a song URL or search term...")
    .required(true);

    // Create the modal structure.
    let modal = CreateQuickModal::new("Add Track to Queue")
        .timeout(Duration::from_secs(300))
        .field(input_text);

    // Display the modal and wait for submission.
    let response = interaction.quick_modal(ctx, modal).await?;

    // Check if the modal was submitted (not cancelled/timed out).
    if let Some(response) = response {
        let interaction = response.interaction;
        // Defer the modal submission response.
        interaction.defer(&ctx.http).await?;

        // Get the user's input from the modal.
        let input = response.inputs[0].clone();

        // Process the input as a play request.
        match MusicManager::process_play_request(
            &ctx,
            interaction.guild_id.unwrap(),
            interaction.channel_id,
            &interaction.user,
            input,
        )
        .await
        {
            // On success, send an ephemeral followup with the result.
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
            // On error, send an ephemeral followup with the error message.
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

/// Handles the repeat button interaction.
/// Toggles the loop state of the current track (Disabled <-> Track) and updates the player message.
async fn handle_repeat(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the handle for the currently playing track.
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
        // Get the current repeat state.
        let current_state = MusicManager::get_repeat_state(guild_id).await;
        // Determine the new state and enable/disable looping on the track handle.
        let new_state = match current_state {
            RepeatState::Disabled => {
                debug!("Looping track '{}'", track.data::<TrackMetadata>().title);
                track.enable_loop()?;
                RepeatState::Track
            }
            RepeatState::Track => {
                debug!(
                    "Disabling loop for track '{}'",
                    track.data::<TrackMetadata>().title
                );
                track.disable_loop()?;
                RepeatState::Disabled
            }
        };

        // Store the new repeat state in the manager.
        MusicManager::set_repeat_state(guild_id, new_state).await;

        // Update the original message.
        update_player_message(ctx, interaction).await
    } else {
        // Send error if no track is playing.
        error_followup(ctx, interaction, "No track is currently playing.").await
    }
}

/// Handles the shuffle button interaction.
/// Shuffles the current queue and updates the player message.
async fn handle_shuffle(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Shuffle the queue via the MusicManager.
    MusicManager::shuffle_queue(&guild_id).await;

    // Update the original message.
    update_player_message(ctx, interaction).await
}

/// Helper function to update the original music player message after an interaction.
/// Fetches the latest state and edits the interaction's message with the new embed and components.
async fn update_player_message(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> ButtonInteractionResult {
    // Ensure guild context.
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Generate the updated message content (embeds + components).
    let reply = embedded_messages::music_player_message(guild_id).await?;

    // Edit the original message the button was attached to.
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

/// Helper function to send an ephemeral error message as a followup to a failed interaction.
async fn error_followup(
    ctx: &Context,
    interaction: &ComponentInteraction,
    content: &str,
) -> ButtonInteractionResult {
    // Create an ephemeral followup message.
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
