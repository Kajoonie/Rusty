use std::sync::Arc;

use crate::{
    Error,
    commands::music::utils::{
        audio_sources::{AudioSource, TrackMetadata},
        autoplay_manager::is_autoplay_enabled,
        queue_manager::{
            self, add_to_queue, clear_manual_stop_flag, clear_previous_action_flag, get_next_track,
            is_manual_stop_flag_set, is_previous_action_flag_set, set_current_track,
        },
        track_cache,
    },
};
use poise::serenity_prelude as serenity;
use serenity::async_trait;
use tracing::{error, info, warn};

/// Event handler for when a song ends
pub struct SongEndNotifier {
    pub ctx: serenity::Context,
    pub guild_id: serenity::GuildId,
    pub call: Arc<serenity::prelude::Mutex<songbird::Call>>,
    pub track_metadata: TrackMetadata,
}

#[async_trait]                                                                                                              
impl songbird::EventHandler for SongEndNotifier {                                                                           
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {                                      
        if let songbird::EventContext::Track(_) = ctx {                                                                     
            // Check if a "previous" action triggered this end event                                                        
            if is_previous_action_flag_set(self.guild_id).await {                                                           
                info!(                                                                                                      
                    "Track ended due to 'previous' action, skipping automatic next track play for guild {}",                
                    self.guild_id                                                                                           
                );                                                                                                          
                // Clear the flag here, as the button handler might clear it slightly later. Redundant clear is fine.       
                clear_previous_action_flag(self.guild_id).await;                                                            
            } else {                                                                                                        
                // Proceed with normal track end handling                                                                   
                self.handle_track_end().await;                                                                              
            }                                                                                                               
        }                                                                                                                   
        None                                                                                                                
    }                                                                                                                       
}

impl SongEndNotifier {
    async fn handle_track_end(&self) {
        info!("Track ended for guild {}", self.guild_id);

        // Pass the http context clone
        let track_played = play_next_track(&self.ctx_http, self.guild_id, self.call.clone())
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

/// Helper function to play the next track in the queue.
/// Loops through the queue until a playable track is found or the queue is empty.
/// Returns Ok(true) if a track was successfully started, Ok(false) if the queue became empty.
/// Returns Err if a non-recoverable error occurs during queue access or handler interaction.
pub async fn play_next_track(
    ctx_http: &Arc<serenity::Http>, // Changed to Arc<Http>
    guild_id: serenity::GuildId,
    call: Arc<serenity::prelude::Mutex<songbird::Call>>,
) -> Result<bool, Error> {
    info!("Attempting to play next track for guild {}", guild_id);

    loop {
        // Get the next track's metadata from the queue
        let metadata = match get_next_track(guild_id).await? {
            Some(meta) => meta,
            None => {
                info!("No more tracks in queue for guild {}", guild_id);
                // Stop the update task if the queue is empty and nothing is playing
                let mut queue_manager_lock = queue_manager::QUEUE_MANAGER.lock().await;
                if queue_manager_lock.get_current_track(guild_id).is_none() {
                    queue_manager_lock.stop_update_task(guild_id).await;
                    info!(
                        "Stopped update task for guild {} due to empty queue.",
                        guild_id
                    );
                }
                return Ok(false); // Indicate queue is empty, break loop
            }
        };

        info!("Got next track metadata from queue: {:?}", metadata.title);

        // --- Check URL and Create Input source on demand ---
        let url = match metadata.url {
            Some(ref u) => u,
            None => {
                error!(
                    "Track metadata for '{}' is missing URL, cannot play.",
                    metadata.title
                );
                warn!("Skipping track without URL, trying next in queue...");
                continue; // Try the next track in the loop
            }
        };

        let input = match track_cache::create_input_from_url(url).await {
            Ok(inp) => inp,
            Err(e) => {
                error!("Failed to create audio input for URL {}: {}", url, e);
                warn!("Skipping track due to input creation error, trying next...");
                continue; // Try the next track in the loop
            }
        };
        // --- End Input creation ---

        // If we reach here, we have valid metadata and input

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
        // We need the full Context here, which we don't have.
        // TODO: Revisit how the update task is started/managed.
        // For now, we assume it might be started elsewhere or handle its absence.
        // let ctx_arc = Arc::new(ctx.clone()); // Cannot clone ctx from just http
        // if let Err(e) = queue_manager::start_update_task(ctx_arc, guild_id).await {
        //     error!("Failed to start update task for guild {}: {}", guild_id, e);
        // }

        // Set up a handler for when the track ends
        let http_clone = ctx_http.clone(); // Clone Arc<Http> for the closure
        let call_clone = call.clone(); // Clone Arc for the closure

        let _ = track_handle.add_event(
            songbird::Event::Track(songbird::TrackEvent::End),
            SongEndNotifier {
                ctx_http: http_clone, // Pass http clone
                guild_id,
                call: call_clone,
                track_metadata: metadata, // Pass the metadata
            },
        );

        return Ok(true); // Indicate a track was played successfully, break loop
    }
}
