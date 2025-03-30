use std::sync::Arc;

use super::*;
use crate::commands::music::utils::{
    audio_sources::{AudioSource, TrackMetadata},
    embedded_messages,
    event_handlers::play_next_track,
    music_manager::{self, MusicError, MusicManager},
    queue_manager::{
        self, QueueCallback, add_to_queue, get_current_track, start_update_task, store_channel_id,
    },
    track_cache::{cache_metadata, create_input_from_url, get_cached_metadata, is_youtube_url},
};
use poise::serenity_prelude as serenity;
use serenity::{ChannelId, GuildId};
use tracing::{debug, error, info, warn};

/// Play a song from YouTube or a direct URL
#[poise::command(slash_command, prefix_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"]
    #[rest]
    query: String,
) -> CommandResult {
    info!("Received /play command with query: {}", query);
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    // Store the channel ID where the command was invoked (for potential message updates)
    store_channel_id(guild_id, ctx.channel_id()).await;

    // Get the user's voice channel ID
    let user_id = ctx.author().id;
    let voice_channel_id =
        match MusicManager::get_user_voice_channel(ctx.serenity_context(), guild_id, user_id) {
            Ok(id) => id,
            Err(err) => {
                ctx.send(embedded_messages::user_not_in_voice_channel(err))
                    .await?;
                return Ok(());
            }
        };

    // Defer the response ephemerally
    ctx.defer_ephemeral().await?;

    // Get songbird manager
    let manager = songbird::serenity::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in scope at initialization.")
        .clone();

    // Call the reusable processing function
    match process_play_request(
        manager,                             // Pass the songbird manager Arc
        ctx.serenity_context().http.clone(), // Pass http Arc
        // ctx.data(), // Pass data Arc if needed later
        guild_id,
        voice_channel_id,
        &query,
    )
    .await
    {
        Ok(reply_content) => {
            // Send the success message from the processing function
            ctx.send(embedded_messages::generic_success("Music", &reply_content))
                .await?;
        }
        Err(err) => {
            // Send an appropriate error message
            let reply = match err {
                MusicError::JoinError(_) => embedded_messages::failed_to_join_voice_channel(err),
                MusicError::CacheError(_) => embedded_messages::failed_to_process_audio_source(err),
                MusicError::AudioSourceError(msg) => embedded_messages::generic_error(&msg),
                _ => embedded_messages::generic_error(&format!(
                    "An unexpected error occurred: {}",
                    err
                )), // Generic fallback for others
            };
            ctx.send(reply).await?;
        }
    }

    Ok(())
}

/// Processes the request to play or queue a track/playlist.
/// Handles joining voice, fetching metadata, caching, queueing, and starting playback if needed.
/// Returns a user-friendly status message string on success.
pub async fn process_play_request(
    manager: Arc<songbird::Songbird>,
    http: Arc<serenity::Http>,
    guild_id: GuildId,
    channel_id: ChannelId,
    query: &str,
) -> Result<String, MusicError> {
    info!(
        "Processing play request for query '{}' in guild {}",
        query, guild_id
    );

    // Join the voice channel if not already connected, or get the existing call
    let call = match manager.get(guild_id) {
        Some(call) => call, // Already connected
        None => {
            // Not connected, attempt to join
            match manager.join(guild_id, channel_id).await {
                Ok(call) => call,
                Err(err) => {
                    error!(
                        "Failed to join voice channel {} for guild {}: {}",
                        channel_id, guild_id, err
                    );
                    return Err(MusicError::JoinError(err.to_string()));
                }
            }
        }
    };

    // --- Track Processing Logic ---
    let metadata: TrackMetadata;
    let mut is_playlist = false; // Flag to track if we handled a playlist

    // Check if the query is a YouTube URL and if it's in the cache
    if is_youtube_url(&query) {
        if let Some(cached_metadata) = get_cached_metadata(&query) {
            info!("Cache hit for URL: {}", query);
            // Verify we can create input before using cached data
            if let Err(err) = create_input_from_url(&query).await {
                warn!(
                    "Cache hit for {}, but failed to create input: {}. Re-fetching.",
                    query, err
                );
                // Fall through to re-fetch if input creation fails
                let queue_callback: QueueCallback =
                    queue_manager::get_queue_callback(guild_id).await;
                match AudioSource::from_query(&query, Some(queue_callback)).await {
                    Ok(fetched_metadata) => {
                        metadata = fetched_metadata;
                        is_playlist = metadata.playlist.is_some();
                        // Re-cache potentially updated metadata
                        if is_youtube_url(metadata.url.as_deref().unwrap_or("")) {
                            cache_metadata(metadata.url.as_ref().unwrap(), metadata.clone());
                        }
                    }
                    Err(err) => {
                        error!(
                            "Failed to re-fetch audio source from URL {}: {}",
                            query, err
                        );
                        return Err(err);
                    }
                }
            } else {
                metadata = cached_metadata; // Use cached metadata
            }
        } else {
            // Not in cache, process as usual and cache later
            info!("Cache miss for URL: {}. Processing query...", query);
            let queue_callback: QueueCallback = queue_manager::get_queue_callback(guild_id).await;
            match AudioSource::from_query(&query, Some(queue_callback)).await {
                Ok(fetched_metadata) => {
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
                    metadata = fetched_metadata;
                    is_playlist = metadata.playlist.is_some();
                }
                Err(err) => {
                    error!("Failed to create audio source from URL {}: {}", query, err);
                    return Err(err);
                }
            }
        }
    } else {
        // Query is not a YouTube URL (likely a search term or other URL)
        info!("Query is not a YouTube URL. Processing query: {}", query);
        let queue_callback: QueueCallback = queue_manager::get_queue_callback(guild_id).await;
        match AudioSource::from_query(&query, Some(queue_callback)).await {
            Ok(fetched_metadata) => {
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
                metadata = fetched_metadata;
                is_playlist = metadata.playlist.is_some();
            }
            Err(err) => {
                error!(
                    "Failed to create audio source from query '{}': {}",
                    query, err
                );
                return Err(err);
            }
        }
    }

    // --- Queueing Logic ---
    debug!(
        "Creating queue item for initial track with metadata: {:?}",
        metadata
    );

    // Check if we're already playing something
    let current_track = get_current_track(guild_id).await?;
    let should_start_playing = current_track.is_none();

    // Add the first track's metadata to the queue
    add_to_queue(guild_id, metadata.clone()).await?; // Return error if adding fails

    // If nothing is currently playing, start playback
    if should_start_playing {
        // Pass http context directly
        let _played = play_next_track(&http, guild_id, call.clone()) // Use http directly
            .await
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to start playback: {}", e))
            })?; // Map error to AudioSourceError

        start_update_task(http, guild_id).await?;
    }

    // --- Generate Success Message ---
    let reply_content = if is_playlist {
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
                .unwrap_or(0)
        )
    } else if should_start_playing {
        format!("▶️ Playing: {}", metadata.title)
    } else {
        format!("✅ Added to queue: {}", metadata.title)
    };

    Ok(reply_content)
}
