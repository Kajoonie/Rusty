use super::*;
use crate::commands::music::utils::{
    audio_sources::AudioSource,
    embedded_messages,
    event_handlers::play_next_track,
    music_manager::{MusicError, MusicManager},
    queue_manager::{
        QueueItem, add_to_queue, get_current_track, get_queue, queue_length, store_channel_id,
    },
};
use std::time::Duration;
use tracing::{debug, error, info};

/// Play a song from YouTube or a direct URL
#[poise::command(slash_command, prefix_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"]
    #[rest]
    query: String,
) -> CommandResult {
    info!("Received play command with query: {}", query);
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Store the channel ID where the command was invoked
    store_channel_id(guild_id, ctx.channel_id()).await;

    // Get the user's voice channel
    let user_id = ctx.author().id;
    let channel_id =
        match MusicManager::get_user_voice_channel(ctx.serenity_context(), guild_id, user_id) {
            Ok(channel_id) => channel_id,
            Err(err) => {
                ctx.send(embedded_messages::user_not_in_voice_channel(err))
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
                    ctx.send(embedded_messages::failed_to_join_voice_channel(err))
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
            ctx.send(embedded_messages::failed_to_process_audio_source(err))
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
        ctx.send(embedded_messages::failed_to_add_to_queue(err))
            .await?;
        return Ok(());
    }

    // If nothing is currently playing, start playback
    if should_start_playing {
        play_next_track(ctx.serenity_context(), guild_id, call, false).await?;
    }

    // Get the queue length
    let position = queue_length(guild_id).await.unwrap_or(0);

    let mut reply = if position == 0 {
        embedded_messages::now_playing(&metadata)
    } else {
        embedded_messages::added_to_queue(&metadata, &position)
    };

    // Add queue information
    let queue_length = queue_length(guild_id).await?;
    if queue_length > 1 {
        let total_duration: Duration = get_queue(guild_id)
            .await?
            .iter()
            .filter_map(|track| track.duration)
            .sum();

        // Create a new embed with the queue info field
        let queue_info = if total_duration.as_secs() > 0 {
            format!(
                "`{} tracks` • Total Length: `{}`",
                queue_length,
                utils::format_duration(total_duration)
            )
        } else {
            format!("`{} tracks`", queue_length)
        };

        // Update the reply with the new embed
        if let Some(first_embed) = reply.embeds.first() {
            let mut new_embed = first_embed.clone();
            new_embed = new_embed.field("Queue Info", queue_info, false);
            reply.embeds = vec![new_embed];
        }
    }

    ctx.send(reply).await?;

    Ok(())
}
