use poise::serenity_prelude::{self as serenity, Context};
use serenity::ComponentInteraction;
use songbird::tracks::PlayMode;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use super::{
    embedded_messages,
    music_manager::MusicManager,
    queue_manager::{self, clear_queue, get_current_track},
};

/// Handle a button interaction
pub async fn handle_button_interaction(
    ctx: &Context,
    interaction: &mut ComponentInteraction,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = interaction.guild_id.ok_or("Not in a guild")?;

    // Defer the interaction response immediately
    interaction.defer(ctx).await?;

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
            // Stop the current track if playing
            if let Some((track, _)) = current_track_opt {
                track.stop()?;
            }
            // Clear the queue (this also stops the update task)
            clear_queue(guild_id).await?;

            // Update the message to reflect stopped state
            update_player_message(ctx, interaction).await?;
        }
        "music_skip" => {
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
            info!("Toggled queue view via button for guild {}", guild_id);

            // Update the message to show/hide the queue
            update_player_message(ctx, interaction).await?;
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
