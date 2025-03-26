use super::*;
use crate::commands::music::utils::{
    audio_sources::AudioSource,
    event_handlers::play_next_track,
    music_manager::{MusicError, MusicManager},
    queue_manager::{add_to_queue, get_current_track, get_queue, queue_length, QueueItem},
};
use poise::serenity_prelude::CreateEmbed;
use std::time::Duration;
use tracing::{debug, error, info};

/// Play a song from YouTube or a direct URL
#[poise::command(slash_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"] query: String,
) -> CommandResult {
    info!("Received play command with query: {}", query);
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the user's voice channel
    let user_id = ctx.author().id;
    let channel_id =
        match MusicManager::get_user_voice_channel(ctx.serenity_context(), guild_id, user_id) {
            Ok(channel_id) => channel_id,
            Err(err) => {
                ctx.send(
                    CreateReply::default()
                        .embed(
                            CreateEmbed::new()
                                .title("❌ Error")
                                .description(format!("You need to be in a voice channel: {}", err))
                                .color(0xff0000),
                        )
                        .ephemeral(true),
                )
                .await?;
                return Ok(());
            }
        };

    // Defer the response since audio processing might take time
    ctx.defer().await?;

    // Join the voice channel if not already connected
    let call = match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        Ok(call) => call,
        Err(_) => {
            // Not connected, so join the channel
            match MusicManager::join_channel(ctx.serenity_context(), guild_id, channel_id).await {
                Ok(call) => call,
                Err(err) => {
                    ctx.send(
                        CreateReply::default().embed(
                            CreateEmbed::new()
                                .title("❌ Error")
                                .description(format!("Failed to join voice channel: {}", err))
                                .color(0xff0000),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }
    };

    // Process the query to get an audio source
    info!("Processing audio source for query: {}", query);

    // Create a callback to add tracks to the queue
    let guild_id_clone = guild_id;
    let queue_callback: Box<
        dyn Fn(songbird::input::Input, crate::commands::music::utils::audio_sources::TrackMetadata)
            + Send
            + Sync,
    > = Box::new(move |input, metadata| {
        // Clone values for the async block
        let guild_id = guild_id_clone;

        tokio::spawn(async move {
            // Create a queue item for this track
            let queue_item = QueueItem {
                input,
                metadata: metadata.clone(),
            };

            // Add track to queue
            if let Err(err) = add_to_queue(guild_id, queue_item).await {
                error!("Failed to add track to queue: {}", err);
                return;
            }

            info!("Added track to queue: {}", metadata.title);
        });
    });

    let (source, metadata) = match AudioSource::from_query(&query, Some(queue_callback)).await {
        Ok(result) => {
            let (src, meta) = result;
            info!("Successfully created audio source. Metadata: {:?}", meta);
            (src, meta)
        }
        Err(err) => {
            error!("Failed to create audio source: {}", err);
            ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("❌ Error")
                        .description(format!("Failed to process audio source: {}", err))
                        .color(0xff0000),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    // Create a queue item for the first track
    debug!("Creating queue item with metadata: {:?}", metadata);
    let queue_item = QueueItem {
        input: source,
        metadata: metadata.clone(),
    };

    // Check if we're already playing something
    let current_track = get_current_track(guild_id).await?;
    let should_start_playing = current_track.is_none();

    // Add the first track to the queue
    if let Err(err) = add_to_queue(guild_id, queue_item).await {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title("❌ Error")
                    .description(format!("Failed to add track to queue: {}", err))
                    .color(0xff0000),
            ),
        )
        .await?;
        return Ok(());
    }

    // If nothing is currently playing, start playback
    if should_start_playing {
        play_next_track(ctx.serenity_context(), guild_id, call).await?;
    }

    // Get the queue length
    let position = queue_length(guild_id).await.unwrap_or(0);

    // Send a success message
    let title = metadata.title.clone();
    let url = metadata.url.clone().unwrap_or_else(|| "#".to_string());
    let duration_str = metadata
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "Unknown duration".to_string());

    let mut embed = if position == 0 {
        // Playing now
        CreateEmbed::new()
            .title("🎵 Now Playing")
            .description(format!("[{}]({})", title, url))
            .field("Duration", format!("`{}`", duration_str), true)
            .color(0x00ff00)
    } else {
        // Added to queue
        CreateEmbed::new()
            .title("🎵 Added to Queue")
            .description(format!("[{}]({})", title, url))
            .field("Duration", format!("`{}`", duration_str), true)
            .field("Position", format!("`#{}`", position), true)
            .color(0x00ff00)
    };

    // Add thumbnail if available
    if let Some(thumbnail) = metadata.thumbnail {
        embed = embed.thumbnail(thumbnail);
    }

    // Add queue information
    let queue_length = queue_length(guild_id).await?;
    if queue_length > 1 {
        let total_duration: Duration = get_queue(guild_id)
            .await?
            .iter()
            .filter_map(|track| track.duration)
            .sum();

        if total_duration.as_secs() > 0 {
            embed = embed.field(
                "Queue Info",
                format!(
                    "`{} tracks` • Total Length: `{}`",
                    queue_length,
                    format_duration(total_duration)
                ),
                false,
            );
        } else {
            embed = embed.field("Queue Info", format!("`{} tracks`", queue_length), false);
        }
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// Format a duration into a human-readable string
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    if minutes >= 60 {
        let hours = minutes / 60;
        let minutes = minutes % 60;
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}
