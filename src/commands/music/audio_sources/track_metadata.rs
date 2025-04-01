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

pub static AUDIO_CACHE: LazyLock<Arc<DashMap<Url, TrackMetadata>>> =
    LazyLock::new(|| Arc::new(DashMap::new()));

/// Represents metadata for a track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub url: Option<String>,
    #[serde(with = "humantime_serde")] // Use humantime_serde for Duration
    pub duration: Option<Duration>,
    pub thumbnail: Option<String>,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            title: "Unknown Track".to_string(),
            url: None,
            duration: None,
            thumbnail: None,
        }
    }
}

impl TryFrom<Output> for TrackMetadata {
    type Error = MusicError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        let metadata_str = String::from_utf8_lossy(&value.stdout);
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Extract metadata from JSON
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
        };

        if let Some(url) = url_str {
            if let Ok(url) = Url::parse(&url) {
                AUDIO_CACHE.insert(url, metadata.clone());
            }
        }

        Ok(metadata)
    }
}

impl TryFrom<SpotifyTrack> for TrackMetadata {
    type Error = MusicError;

    fn try_from(value: SpotifyTrack) -> Result<Self, Self::Error> {
        let search_query = SpotifyApi::get_youtube_search_query(&value);
        info!("Searching YouTube for Spotify track: {}", search_query);
        YoutubeApi::from_search(&search_query)
    }
}