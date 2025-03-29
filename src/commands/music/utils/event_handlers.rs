use std::sync::Arc;

use crate::{
    commands::music::utils::{
        audio_sources::{AudioSource, TrackMetadata},
        autoplay_manager::is_autoplay_enabled,
        queue_manager::{
            self, add_to_queue, clear_manual_stop_flag, get_next_track, is_manual_stop_flag_set,
            set_current_track, // Removed QueueItem from here
        },
        track_cache, // Import track_cache
    },
    Error,
};
use poise::serenity_prelude as serenity;
use serenity::async_trait;
use songbird::input::Input; // Import Input
use tracing::{error, info, warn}; // Added warn

/// Event handler for when a song ends
pub struct SongEndNotifier {
    pub ctx: serenity::Context,
    pub guild_id: serenity::GuildId,
    pub call: Arc<serenity::prelude::Mutex<songbird::Call>>, // Use Arc directly
    pub track_metadata: TrackMetadata,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track(_) = ctx {
            self.handle_track_end().await;
        }
        None
    }
}

impl SongEndNotifier {
    async fn handle_track_end(&self) {
        info!("Track ended for guild {}", self.guild_id);

        let track_played = play_next_track(&self.ctx, self.guild_id, self.call.clone())
            .await
            .is_ok();

        if !track_played {
            self.handle_empty_queue().await;
        }
    }

    async fn handle_empty_queue(&self) {
        let manual_stop = is_manual_stop_flag_set(self.guild_id).await;

        if manual_stop {
            clear_manual_stop_flag(self.guild_id).await;
            return;
        }

        if is_autoplay_enabled(self.guild_id).await {
            if let Err(e) = self.attempt_autoplay().await {
                error!("Autoplay failed: {}", e);
            }
        }
    }

    async fn attempt_autoplay(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(url) = &self.track_metadata.url {
            let related_songs = AudioSource::get_related_songs(url).await?;

            for song in related_songs {
                if let Some(song_url) = &song.url {
                    // Ensure it's a valid YouTube video URL before proceeding
                    if !track_cache::is_youtube_url(song_url) {
                        warn!("Skipping non-YouTube URL from related songs: {}", song_url);
                        continue;
                    }

                    // Add metadata to queue, Input will be created when it plays
                    add_to_queue(self.guild_id, song.clone()).await?;
                    info!(
                        "Added related song '{}' to queue for guild {}",
                        song.title, self.guild_id
                    );

                    // Attempt to play the newly added track immediately
                    // Check if anything is currently playing first
                    let should_play_immediately = {
                        let queue_manager_lock = queue_manager::QUEUE_MANAGER.lock().await;
                        queue_manager_lock
                            .get_current_track(self.guild_id)
                            .is_none()
                    };

                    if should_play_immediately {
                        info!(
                            "Queue was empty, attempting to play related song immediately for guild {}",
                            self.guild_id
                        );
                        if let Err(e) =
                            play_next_track(&self.ctx, self.guild_id, self.call.clone()).await
                        {
                            error!(
                                "Failed to play related track immediately for guild {}: {}",
                                self.guild_id, e
                            );
                        }
                    }
                    // Stop after adding and attempting to play one related song
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Helper function to play the next track in the queue
/// Returns true if a track was played, false if the queue was empty
/// Helper function to play the next track in the queue
/// Returns true if a track was played, false if the queue was empty
pub async fn play_next_track(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    call: Arc<serenity::prelude::Mutex<songbird::Call>>,
) -> Result<bool, Error> {
    info!("Attempting to play next track for guild {}", guild_id);

    // Get the next track's metadata from the queue
    let metadata = match get_next_track(guild_id).await? {
        Some(meta) => meta,
        None => {
            info!("No more tracks in queue for guild {}", guild_id);
            // Stop the update task if the queue is empty and nothing is playing
            let mut queue_manager_lock = queue_manager::QUEUE_MANAGER.lock().await;
            if queue_manager_lock.get_current_track(guild_id).is_none() {
                queue_manager_lock.stop_update_task(guild_id).await;
                info!("Stopped update task for guild {} due to empty queue.", guild_id);
            }
            return Ok(false); // Indicate no track was played
        }
    };

    info!("Got next track metadata from queue: {:?}", metadata.title);

    // --- Create Input source on demand ---
    let url = match metadata.url {
        Some(ref u) => u,
        None => {
            error!(
                "Track metadata for '{}' is missing URL, cannot play.",
                metadata.title
            );
            // Attempt to play the *next* track instead of failing silently
            warn!("Skipping track without URL, trying next in queue...");
            return play_next_track(ctx, guild_id, call).await; // Recursive call for next track
        }
    };

    let input = match track_cache::create_input_from_url(url).await {
        Ok(inp) => inp,
        Err(e) => {
            error!("Failed to create audio input for URL {}: {}", url, e);
            // Attempt to play the *next* track
            warn!("Skipping track due to input creation error, trying next...");
            return play_next_track(ctx, guild_id, call).await; // Recursive call for next track
        }
    };
    // --- End Input creation ---

    // Get a lock on the call
    let mut handler = call.lock().await;
    info!("Obtained lock on voice handler, preparing to play audio");

    // Play the track using the created Input
    let track_handle = handler.play_input(input); // Use the created input
    info!("Track handle created for: {}", metadata.title);

    // Store the current track's metadata
    // Cloning metadata is cheap and necessary as it's moved to the notifier later
    set_current_track(guild_id, track_handle.clone(), metadata.clone()).await?;

    // Start the update task now that a track is playing
    let ctx_arc = Arc::new(ctx.clone());
    if let Err(e) = queue_manager::start_update_task(ctx_arc, guild_id).await {
        error!("Failed to start update task for guild {}: {}", guild_id, e);
    }

    // Set up a handler for when the track ends
    let ctx = ctx.clone();
    let call = call.clone();

    let _ = track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        SongEndNotifier {
            ctx,
            guild_id,
            call,
            track_metadata: metadata, // Pass the metadata
        },
    );

    Ok(true) // Indicate a track was played
}
