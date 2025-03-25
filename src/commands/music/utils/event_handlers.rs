use crate::commands::music::utils::{
    audio_sources::{AudioSource, TrackMetadata},
    autoplay_manager::is_autoplay_enabled,
    queue_manager::{
        add_to_queue, clear_manual_stop_flag, get_next_track, is_manual_stop_flag_set,
        set_current_track,
    },
};
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use tracing::{error, info};

/// Event handler for when a song ends
pub struct SongEndNotifier {
    pub ctx: serenity::Context,
    pub guild_id: serenity::GuildId,
    pub call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
    pub track_metadata: TrackMetadata,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        info!("Track end event triggered for guild {}", self.guild_id);

        // Check if this is a track end event
        if let songbird::EventContext::Track(_track_list) = ctx {
            info!("Track ended naturally, proceeding to next track");

            // Attempt to play the next track
            match play_next_track(&self.ctx, self.guild_id, self.call.clone()).await {
                Ok(track_played) => {
                    if track_played {
                        info!("Successfully started playing next track");
                    } else {
                        info!("Queue is empty, checking if autoplay is enabled");

                        // Check if the manual stop flag is set
                        let manual_stop = is_manual_stop_flag_set(self.guild_id).await;

                        if manual_stop {
                            info!("Manual stop flag is set, skipping autoplay");
                            // Clear the flag for future playback
                            clear_manual_stop_flag(self.guild_id).await;
                        } else if is_autoplay_enabled(self.guild_id).await {
                            info!("Autoplay is enabled, attempting to find related songs");

                            // Use the metadata we stored in the struct
                            if let Some(url) = &self.track_metadata.url {
                                match AudioSource::get_related_songs(url).await {
                                    Ok(related_songs) => {
                                        for song in related_songs {
                                            if let Some(song_url) = &song.url {
                                                info!(
                                                    "Adding related song to queue: {}",
                                                    song.title
                                                );

                                                // Make sure the URL is a valid YouTube video URL
                                                if !AudioSource::is_youtube_video_url(song_url) {
                                                    info!("Skipping non-video URL: {}", song_url);
                                                    continue;
                                                }

                                                // Create audio source from the related song
                                                if let Ok((source, _)) =
                                                    AudioSource::from_youtube_url(song_url).await
                                                {
                                                    let queue_item = QueueItem {
                                                        input: source,
                                                        metadata: song,
                                                    };

                                                    // Add to queue and start playing
                                                    if (add_to_queue(self.guild_id, queue_item)
                                                        .await)
                                                        .is_ok()
                                                    {
                                                        let _ = play_next_track(
                                                            &self.ctx,
                                                            self.guild_id,
                                                            self.call.clone(),
                                                        )
                                                        .await;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to get related songs: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to play next track: {}", e);
                }
            }
        }

        None
    }
}

/// Helper function to play the next track in the queue
/// Returns true if a track was played, false if the queue was empty
pub async fn play_next_track(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    info!("Attempting to play next track for guild {}", guild_id);

    // Get the next track from the queue
    let queue_item = match get_next_track(guild_id).await? {
        Some(item) => item,
        None => {
            info!("No more tracks in queue for guild {}", guild_id);
            return Ok(false); // Indicate no track was played
        }
    };

    info!("Got next track from queue: {:?}", queue_item.metadata.title);

    // Get a lock on the call
    let mut handler = call.lock().await;
    info!("Obtained lock on voice handler, preparing to play audio");

    // Play the track and verify it started successfully
    let track_handle = handler.play_input(queue_item.input);
    info!("Track handle created");

    // Store the current track
    set_current_track(guild_id, track_handle.clone(), queue_item.metadata.clone()).await?;

    // Set up a handler for when the track ends
    let ctx = ctx.clone();
    let call = call.clone();

    let _ = track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        SongEndNotifier {
            ctx,
            guild_id,
            call,
            track_metadata: queue_item.metadata.clone(),
        },
    );

    Ok(true) // Indicate a track was played
}

/// Struct needed for QueueItem
pub use crate::commands::music::utils::queue_manager::QueueItem;
