use std::sync::Arc;

use crate::commands::music::audio_sources::{track_metadata::TrackMetadata, youtube::YoutubeApi};
use poise::serenity_prelude as serenity;
use serenity::async_trait;
use tracing::{error, info, warn};

use super::{
    autoplay_manager::{self},
    music_manager::MusicManager,
};

/// Event handler for when a song ends
pub struct SongEndNotifier {
    pub guild_id: serenity::GuildId,
    pub call: Arc<serenity::prelude::Mutex<songbird::Call>>,
    pub track_metadata: TrackMetadata,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track([(_track_state, _track_handle)]) = ctx {
            if let Some(queue) = MusicManager::get_queue(&self.guild_id).await {
                if queue.is_empty() {
                    self.handle_empty_queue().await;
                }
            }
        }
        None
    }
}

impl SongEndNotifier {
    async fn handle_empty_queue(&self) {
        if autoplay_manager::is_autoplay_enabled(self.guild_id).await {
            if let Err(e) = self.attempt_autoplay().await {
                error!("Autoplay failed: {}", e);
            }
        }
    }

    async fn attempt_autoplay(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(url) = &self.track_metadata.url {
            let related_songs = YoutubeApi::get_related_songs(url).await?;

            for metadata in related_songs {
                if let Some(song_url) = &metadata.url {
                    // Ensure it's a valid YouTube video URL before proceeding
                    if !YoutubeApi::is_youtube_url(song_url) {
                        warn!("Skipping non-YouTube URL from related songs: {}", song_url);
                        continue;
                    }

                    // Add metadata to queue, Input will be created when it plays
                    MusicManager::add_to_queue(self.call.clone(), metadata.clone()).await;
                    info!(
                        "Added related song '{}' to queue for guild {}",
                        metadata.title, self.guild_id
                    );

                    // Stop after adding and attempting to play one related song
                    break;
                }
            }
        }

        Ok(())
    }
}
