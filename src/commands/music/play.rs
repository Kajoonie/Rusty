use super::*;
use crate::commands::music::utils::{
    audio_sources::{AudioSource, TrackMetadata}, // Correct import path for TrackMetadata
    embedded_messages,
    event_handlers::play_next_track,
    music_manager::{MusicError, MusicManager},
    queue_manager::{
        self, MetadataCallback, QueueItem, add_to_queue, get_current_track, store_channel_id, // Changed QueueCallback to MetadataCallback
    },
    track_cache::{cache_metadata, create_input_from_url, get_cached_metadata, is_youtube_url},
};
use songbird::input::Input;
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

    // --- Track Processing Logic ---
    let (source, metadata): (Input, TrackMetadata);
    let mut is_playlist = false; // Flag to track if we handled a playlist

    // Check if the query is a YouTube URL and if it's in the cache
    if is_youtube_url(&query) {
        if let Some(cached_metadata) = get_cached_metadata(&query) {
            info!("Cache hit for URL: {}", query);
            match create_input_from_url(&query).await {
                Ok(cached_source) => {
                    source = cached_source;
                    metadata = cached_metadata;
                }
                Err(err) => {
                    error!("Failed to create input from cached URL {}: {}", query, err);
                    ctx.send(embedded_messages::failed_to_process_audio_source(
                        MusicError::CacheError(err),
                    ))
                    .await?;
                    return Ok(());
                }
            }
        } else {
            // Not in cache, process as usual and cache later
            info!("Cache miss for URL: {}. Processing query...", query);
            // Create a callback to add tracks to the queue (for potential playlists)
            let queue_callback: MetadataCallback = queue_manager::get_queue_callback(guild_id).await; // Use MetadataCallback
            match AudioSource::from_query(&query, Some(queue_callback)).await {
                Ok((fetched_source, fetched_metadata)) => {
                    info!(
                        "Successfully created audio source. Metadata: {:?}",
                        fetched_metadata
                    );
                    // Cache the metadata for the primary track
                    if is_youtube_url(fetched_metadata.url.as_deref().unwrap_or("")) {
                        cache_metadata(
                            fetched_metadata.url.as_ref().unwrap(),
                            fetched_metadata.clone(),
                        );
                    } else {
                        debug!(
                            "Fetched metadata URL is not a cacheable YouTube URL: {:?}",
                            fetched_metadata.url
                        );
                    }
                    source = fetched_source;
                    metadata = fetched_metadata;
                    // Check if the metadata indicates a playlist was processed
                    is_playlist = metadata.playlist.is_some();
                }
                Err(err) => {
                    error!("Failed to create audio source from URL {}: {}", query, err);
                    ctx.send(embedded_messages::failed_to_process_audio_source(err))
                        .await?;
                    return Ok(());
                }
            }
        }
    } else {
        // Query is not a YouTube URL (likely a search term or other URL)
        info!("Query is not a YouTube URL. Processing query: {}", query);
        // Create a callback to add tracks to the queue (for potential playlists)
        let queue_callback: MetadataCallback = queue_manager::get_queue_callback(guild_id).await; // Use MetadataCallback
        match AudioSource::from_query(&query, Some(queue_callback)).await {
            Ok((fetched_source, fetched_metadata)) => {
                info!(
                    "Successfully created audio source. Metadata: {:?}",
                    fetched_metadata
                );
                // Cache the metadata if it resolved to a YouTube URL
                if is_youtube_url(fetched_metadata.url.as_deref().unwrap_or("")) {
                    cache_metadata(
                        fetched_metadata.url.as_ref().unwrap(),
                        fetched_metadata.clone(),
                    );
                } else {
                    debug!(
                        "Fetched metadata URL is not a cacheable YouTube URL: {:?}",
                        fetched_metadata.url
                    );
                }
                source = fetched_source;
                metadata = fetched_metadata;
                // Check if the metadata indicates a playlist was processed
                is_playlist = metadata.playlist.is_some();
            }
            Err(err) => {
                error!(
                    "Failed to create audio source from query '{}': {}",
                    query, err
                );
                ctx.send(embedded_messages::failed_to_process_audio_source(err))
                    .await?;
                return Ok(());
            }
        }
    }

    // --- Queueing Logic ---

    // Create a queue item for the *first* track (even if it's a playlist)
    debug!(
        "Creating queue item for initial track with metadata: {:?}",
        metadata
    );
    // QueueItem now only holds metadata. Input is handled by play_next_track.
    let queue_item = QueueItem {
        metadata: metadata.clone(),
    };

    // Check if we're already playing something
    let current_track = get_current_track(guild_id).await?;
    let should_start_playing = current_track.is_none();

    // Add the first track's metadata to the queue
    // Note: We pass metadata directly now, not the QueueItem struct
    if let Err(err) = add_to_queue(guild_id, metadata.clone()).await { // Pass metadata directly
        ctx.send(embedded_messages::failed_to_add_to_queue(err))
            .await?;
        return Ok(());
    }

    // If nothing is currently playing, start playback
    if should_start_playing {
        play_next_track(ctx.serenity_context(), guild_id, call).await?;
    }

    // Send an ephemeral confirmation message
    let reply_content = if is_playlist {
        // If a playlist was added, the callback handles adding subsequent tracks.
        // The 'metadata' here refers to the *first* track of the playlist.
        format!(
            "✅ Added playlist: {} (and {} other tracks)",
            metadata
                .playlist
                .as_ref()
                .map(|p| p.title.as_str())
                .unwrap_or("Unknown Playlist"),
            metadata
                .playlist
                .as_ref()
                .map(|p| p.track_count.saturating_sub(1))
                .unwrap_or(0) // Show count of *other* tracks
        )
    } else if should_start_playing {
        format!("▶️ Playing: {}", metadata.title)
    } else {
        format!("✅ Added to queue: {}", metadata.title)
    };
    ctx.send(embedded_messages::generic_success("Music", &reply_content))
        .await?;

    Ok(())
}
