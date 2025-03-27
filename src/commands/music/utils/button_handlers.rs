use poise::serenity_prelude::{self as serenity, Context, Message};
use serenity::{ComponentInteraction, CreateInteractionResponseMessage};
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::error;

use super::{
    button_controls::create_updated_buttons,
    embedded_messages,
    music_manager::MusicManager,
    queue_manager::{self, get_current_track},
};

/// Handle a button interaction
pub async fn handle_button_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Ensure we're in a call
    if let Err(err) = MusicManager::get_call(ctx, guild_id).await {
        return create_response(
            ctx,
            interaction,
            embedded_messages::bot_not_in_voice_channel(err.into()),
        )
        .await;
    }

    // Get the current track
    let current_track = get_current_track(guild_id).await?;
    let has_queue = queue_manager::get_queue(guild_id).await?.len() > 0;

    match interaction.data.custom_id.as_str() {
        "music_play_pause" => {
            if let Some((track, metadata)) = current_track {
                let track_info = track.get_info().await?;
                let is_playing = track_info.playing == PlayMode::Play;

                if is_playing {
                    track.pause()?;
                    create_response(ctx, interaction, embedded_messages::paused(&metadata)).await?;
                } else {
                    track.play()?;
                    create_response(ctx, interaction, embedded_messages::resumed(&metadata)).await?;
                }

                // Update button states
                update_message_components(ctx, &mut *interaction.message, !is_playing, has_queue)
                    .await?;
            } else {
                create_response(ctx, interaction, embedded_messages::no_track_playing()).await?;
            }
        }
        "music_stop" => {
            if let Some((track, _)) = current_track {
                track.stop()?;
                create_response(ctx, interaction, embedded_messages::stopped(false)).await?;

                // Update button states
                update_message_components(ctx, &mut *interaction.message, false, false).await?;
            } else {
                create_response(ctx, interaction, embedded_messages::no_track_playing()).await?;
            }
        }
        "music_skip" => {
            if let Some((track, metadata)) = current_track {
                let track_info = track.get_info().await?;
                let is_playing = track_info.playing == PlayMode::Play;
                track.stop()?;
                create_response(ctx, interaction, embedded_messages::skipped(&metadata)).await?;

                // Update button states after a short delay to allow the next track to start
                sleep(Duration::from_millis(100)).await;
                update_message_components(ctx, &mut *interaction.message, is_playing, has_queue)
                    .await?;
            } else {
                create_response(ctx, interaction, embedded_messages::no_track_to_skip()).await?;
            }
        }
        _ => {
            error!("Unknown button ID: {}", interaction.data.custom_id);
            create_response(ctx, interaction, embedded_messages::not_pausable()).await?;
        }
    }

    Ok(())
}

/// Create a response for the interaction
async fn create_response(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
    embed: CreateEmbed,
) -> Result<(), serenity::Error> {
    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::default()
                    .content("")
                    .embed(embed),
            ),
        )
        .await?;
    Ok(())
}

/// Update the components of a message with new button states
async fn update_message_components(
    ctx: &Context,
    message: &mut Message,
    is_playing: bool,
    has_queue: bool,
) -> Result<(), serenity::Error> {
    message
        .edit(
            ctx,
            serenity::EditMessage::new().components(create_updated_buttons(is_playing, has_queue)),
        )
        .await?;
    Ok(())
}
