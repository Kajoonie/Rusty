//! Defines the `TrackMetadata` struct, a unified representation of track information
//! from various audio sources, and provides conversion logic.

use crate::commands::music::audio_sources::spotify::SpotifyApi;
use crate::commands::music::audio_sources::youtube::YoutubeApi;
use crate::commands::music::utils::music_manager::MusicError;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::process::Output;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tracing::info;
use url::Url;

use super::spotify::SpotifyTrack;

/// Lazily initialized, thread-safe cache for storing fetched `TrackMetadata`.
/// Uses the track's URL as the key. `DashMap` allows concurrent reads/writes.
pub static AUDIO_CACHE: LazyLock<Arc<DashMap<Url, TrackMetadata>>> =
    LazyLock::new(|| Arc::new(DashMap::new()));

/// Unified representation of metadata for a playable track.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackMetadata {
    /// The title of the track.
    pub title: String,
    /// The direct URL to the track, if available (e.g., YouTube video URL).
    pub url: Option<String>,
    /// The duration of the track, if available.
    #[serde(with = "humantime_serde")]
    pub duration: Option<Duration>,
    /// URL to a thumbnail image for the track, if available.
    pub thumbnail: Option<String>,
    /// The name of the user who requested the track.
    pub requested_by: Option<String>,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            title: "Unknown Track".to_string(),
            url: None,
            duration: None,
            thumbnail: None,
            requested_by: None,
        }
    }
}

impl TrackMetadata {
    /// Creates `TrackMetadata` from YouTube (`yt-dlp`) output, adding the requestor's name.
    pub fn from_youtube(output: Output, requested_by: String) -> Result<TrackMetadata, MusicError> {
        let mut metadata = Self::try_from(output)?;
        metadata.misc_data(requested_by);
        Ok(metadata)
    }

    /// Creates `TrackMetadata` from Spotify track info, adding the requestor's name.
    /// This involves searching YouTube for the corresponding track.
    pub fn from_spotify(
        spotify_track: SpotifyTrack,
        requested_by: String,
    ) -> Result<TrackMetadata, MusicError> {
        let mut metadata = Self::try_from(spotify_track)?;
        metadata.misc_data(requested_by);
        Ok(metadata)
    }

    /// Helper method to set miscellaneous data, currently just the requestor's name.
    fn misc_data(&mut self, requested_by: String) {
        self.requested_by = Some(requested_by);
    }
}

/// Converts the output of `yt-dlp --dump-json` into `TrackMetadata`.
impl TryFrom<Output> for TrackMetadata {
    type Error = MusicError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        // Convert stdout bytes to string.
        let metadata_str = String::from_utf8_lossy(&value.stdout);
        // Parse the string as JSON.
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Extract fields, providing defaults if missing.
        let title = metadata_json["title"]
            .as_str()
            .unwrap_or("Unknown Title")
            .to_string();

        let duration = metadata_json["duration"]
            .as_f64()
            .map(Duration::from_secs_f64);

        let thumbnail = metadata_json["thumbnail"].as_str().map(|s| s.to_string());

        let url_str = metadata_json["webpage_url"].as_str().map(|s| s.to_string());

        // Create metadata with extracted information
        let metadata = TrackMetadata {
            title,
            url: url_str.clone(),
            duration,
            thumbnail,
            requested_by: None,
        };

        // If a valid URL was extracted, attempt to cache the metadata.
        if let Some(url) = url_str {
            // Parse the URL string.
            if let Ok(url) = Url::parse(&url) {
                // Insert into the cache.
                AUDIO_CACHE.insert(url, metadata.clone());
            }
        }

        Ok(metadata)
    }
}

/// Converts Spotify track information into `TrackMetadata` by searching YouTube.
///
/// This conversion is lossy as it relies on finding a matching track on YouTube.
/// It constructs a search query from the Spotify track name and artists.
impl TryFrom<SpotifyTrack> for TrackMetadata {
    type Error = MusicError;

    fn try_from(value: SpotifyTrack) -> Result<Self, Self::Error> {
        // Generate a YouTube search query from the Spotify track details.
        let search_query = SpotifyApi::get_youtube_search_query(&value);
        info!("Searching YouTube for Spotify track: {}", search_query);
        // Perform the YouTube search and return the resulting metadata (or error).
        YoutubeApi::from_search(&search_query)
    }
}
