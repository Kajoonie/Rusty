pub(crate) mod serp_api;
pub(crate) mod ytdl;

use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;

/// Result type for fetching related songs
pub type RelatedSongsResult = Result<Vec<TrackMetadata>, MusicError>;

/// Trait defining the interface for fetching related songs
#[async_trait]
pub trait RelatedSongsFetcher {
    /// Fetch related songs for a given video ID
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult;
}
