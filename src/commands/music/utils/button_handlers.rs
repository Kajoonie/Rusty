use poise::serenity_prelude::{self as serenity, Context};
use serenity::{ComponentInteraction, CreateInteractionResponse, CreateInteractionResponseMessage};
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::error;

use super::{
    button_controls::create_updated_buttons, embedded_messages, music_manager::MusicManager, queue_manager::{self, get_current_track}
};

/// Handle a button interaction
pub async fn handle_button_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Ensure we're in a call
    if let Err(_err) = MusicManager::get_call(ctx, guild_id).await {
        return error_response(ctx, interaction).await; // bot not in voice channel
    }

    // Get the current track
    let current_track = get_current_track(guild_id).await?;
    let has_queue = !queue_manager::get_queue(guild_id).await?.is_empty();

    match interaction.data.custom_id.as_str() {
        "music_play_pause" => {
            if let Some((track, _metadata)) = current_track {
                let track_info = track.get_info().await?;
                let is_playing = track_info.playing == PlayMode::Play;

                if is_playing {
                    track.pause()?;
                } else {
                    track.play()?;
                }

                // Update button states
                update_with_response(ctx, interaction, !is_playing, has_queue).await?;
            } else {
                error_response(ctx, interaction).await?; // no track playing
            }
        }
        "music_stop" => {
            if let Some((track, _)) = current_track {
                track.stop()?;

                // Update button states
                update_with_response(ctx, interaction, false, false).await?;
            } else {
                error_response(ctx, interaction).await?; // no track playing
            }
        }
        "music_skip" => {
            if let Some((track, _metadata)) = current_track {
                let track_info = track.get_info().await?;
                let is_playing = track_info.playing == PlayMode::Play;
                track.stop()?;

                // Update button states after a short delay to allow the next track to start
                sleep(Duration::from_millis(100)).await;
                update_with_response(ctx, interaction, is_playing, has_queue).await?;
            } else {
                error_response(ctx, interaction).await?; // no track to skip
            }
        }
        _ => {
            error!("Unknown button ID: {}", interaction.data.custom_id);
            error_response(ctx, interaction).await?; // not pausable
        }
    }

    Ok(())
}

/// Update the message with new button states using direct interaction response
async fn update_with_response(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    is_playing: bool,
    has_queue: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embeds(embedded_messages::music_player_message(interaction.guild_id.unwrap()).await?.embeds)
                    .components(create_updated_buttons(is_playing, has_queue)),
            ),
        )
        .await?;

    Ok(())
}

/// Send an error response for failed interactions
async fn error_response(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    interaction
        .create_response(
            ctx, 
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Unable to process this action. The bot may not be playing music or encountered an error.")
                    .ephemeral(true)
            )
        )
        .await?;
    Ok(())
}
