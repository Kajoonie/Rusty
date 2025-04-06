//! Contains Songbird event handlers specific to the music functionality,
//! primarily for handling track endings and triggering autoplay.

use std::sync::Arc;

use crate::commands::music::audio_sources::{track_metadata::TrackMetadata, youtube::YoutubeApi};
use poise::serenity_prelude as serenity;
use serenity::async_trait;
use tracing::{error, info, warn};

use super::{
    autoplay_manager::{self},
    music_manager::MusicManager,
};

/// A Songbird event handler that triggers when a track finishes playing.
/// It checks if the queue is empty and if autoplay is enabled, then attempts
/// to find and queue a related song.
pub struct SongEndNotifier {
    /// The ID of the guild where the event occurred.
    pub guild_id: serenity::GuildId,
    /// A handle to the Songbird voice call.
    pub call: Arc<serenity::prelude::Mutex<songbird::Call>>,
    /// Metadata of the track that just finished.
    pub track_metadata: TrackMetadata,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    /// The main action performed when a Songbird event occurs.
    /// This implementation specifically listens for `EventContext::Track` events.
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        // Check if the event is a track ending event.
        if let songbird::EventContext::Track([(_track_state, _track_handle)]) = ctx {
            // Get the queue for the guild.
            if let Some(queue) = MusicManager::get_queue(&self.guild_id).await {
                // If the queue is now empty, handle the empty queue logic (check autoplay).
                if queue.is_empty() {
                    self.handle_empty_queue().await;
                }
            }
        }
        // Indicate that this handler doesn't need to handle further events for this track.
        None
    }
}

impl SongEndNotifier {
    /// Handles the logic when the queue becomes empty after a track ends.
    /// Checks if autoplay is enabled for the guild and attempts it if so.
    async fn handle_empty_queue(&self) {
        // Check if autoplay is enabled via the AutoplayManager.
        if autoplay_manager::is_autoplay_enabled(self.guild_id).await {
            // Try to find and queue a related song.
            if let Err(e) = self.attempt_autoplay().await {
                // Log any errors during autoplay attempt.
                error!("Autoplay failed: {}", e);
            }
        }
    }

    /// Attempts to find and queue a related song based on the finished track's metadata.
    /// Uses `YoutubeApi::get_related_songs` and adds the first valid result to the queue.
    async fn attempt_autoplay(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if the finished track has a URL (needed to find related songs).
        if let Some(url) = &self.track_metadata.url {
            // Fetch related songs using the YouTube API implementation.
            let related_songs = YoutubeApi::get_related_songs(url).await?;

            // Iterate through the fetched related songs.
            for metadata in related_songs {
                // Check if the related song has a URL.
                if let Some(song_url) = &metadata.url {
                    // Basic validation: ensure it looks like a YouTube URL.
                    if !YoutubeApi::is_youtube_url(song_url) {
                        warn!("Skipping non-YouTube URL from related songs: {}", song_url);
                        continue;
                    }

                    // Add the valid related song's metadata to the queue.
                    MusicManager::add_to_queue(self.call.clone(), metadata.clone()).await;
                    info!(
                        "Added related song '{}' to queue for guild {}",
                        metadata.title, self.guild_id
                    );

                    // Only add the first valid related song found.
                    break;
                }
            }
        }

        Ok(())
    }
}
