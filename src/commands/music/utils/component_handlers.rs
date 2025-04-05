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
        "music_repeat" => handle_repeat(ctx, interaction, guild_id).await?,
        "music_shuffle" => handle_shuffle(ctx, interaction, guild_id).await?,
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

/// Handler for Repeat button
async fn handle_repeat(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    // Get the current track
    let current_track_opt = MusicManager::get_current_track(&guild_id).await;

    if let Some(track) = current_track_opt {
        // Toggle the repeat state
        let current_state = MusicManager::get_repeat_state(guild_id).await;
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

        // Update the state in MusicManager
        MusicManager::set_repeat_state(guild_id, new_state).await;

        // Update the message
        update_player_message(ctx, interaction).await
    } else {
        error_followup(ctx, interaction, "No track is currently playing.").await
    }
}

/// Handler for Shuffle button
async fn handle_shuffle(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    guild_id: GuildId,
) -> ButtonInteractionResult {
    MusicManager::shuffle_queue(&guild_id).await;

    // Update the player message
    update_player_message(ctx, interaction).await
}

/// Update the original player message after a button interaction
async fn update_player_message(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> ButtonInteractionResult {
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
