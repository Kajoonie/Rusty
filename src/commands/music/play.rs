use super::*;
use crate::commands::music::utils::{
    audio_sources::AudioSource,
    embedded_messages,
    event_handlers::play_next_track,
    music_manager::{MusicError, MusicManager},
    queue_manager::{
        self, QueueCallback, QueueItem, add_to_queue, get_current_track, store_channel_id,
    },
};
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

    // Defer the response ephemerally since audio processing might take time
    // and we want the final confirmation to be ephemeral.
    ctx.defer_ephemeral().await?;

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
    let queue_callback: QueueCallback = queue_manager::get_queue_callback(guild_id).await;

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
        play_next_track(ctx.serenity_context(), guild_id, call).await?;
    }

    // Send an ephemeral confirmation message
    let reply_content = if should_start_playing {
        format!("▶️ Playing: {}", metadata.title)
    } else {
        format!("✅ Added to queue: {}", metadata.title)
    };
    ctx.send(embedded_messages::generic_success("Music", &reply_content))
        .await?;

    Ok(())
}
