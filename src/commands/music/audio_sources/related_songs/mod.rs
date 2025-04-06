//! This module defines the trait and implementations for fetching songs related
//! to a given YouTube video, primarily used for the autoplay feature.

/// Implementation using the SerpAPI (Google Search Results API).
pub(crate) mod serp_api;
/// Implementation using `yt-dlp` to extract related videos.
pub(crate) mod ytdl;

use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;

/// A specialized `Result` type for related song fetching operations.
pub type RelatedSongsResult = Result<Vec<TrackMetadata>, MusicError>;

/// Defines the common interface for fetching related songs based on a video ID.
/// Requires `Send + Sync` for safe use across async tasks.
#[async_trait]
pub trait RelatedSongsFetcher {
    /// Asynchronously fetches a list of related songs (as `TrackMetadata`).
    ///
    /// # Arguments
    ///
    /// * `video_id` - The YouTube video ID to find related songs for.
    ///
    /// # Returns
    ///
    /// A `RelatedSongsResult` containing a `Vec<TrackMetadata>` on success, or a `MusicError` on failure.
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult;
}
