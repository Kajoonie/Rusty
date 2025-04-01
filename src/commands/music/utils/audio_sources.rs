use super::audio_cache::AUDIO_CACHE;
use super::spotify::{SpotifyApi, SpotifyTrack};
use crate::HTTP_CLIENT;
use crate::commands::music::utils::music_manager::MusicError;
use crate::commands::music::utils::youtube::YoutubeApi;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use songbird::input::{Input, YoutubeDl};
#[cfg(feature = "music")]
use std::process::Output;
use std::time::Duration;
use tracing::info;
use url::Url;

/// Result type for audio source operations
pub type AudioSourceResult<T> = Result<T, MusicError>;

/// Represents metadata for a playlist or album
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistMetadata {
    pub title: String,
    pub track_count: usize,
}

/// Represents metadata for a track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub url: Option<String>,
    #[serde(with = "humantime_serde")] // Use humantime_serde for Duration
    pub duration: Option<Duration>,
    pub thumbnail: Option<String>,
    // Optional field to indicate if this track is part of a playlist/album
    pub playlist: Option<PlaylistMetadata>,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            title: "Unknown Track".to_string(),
            url: None,
            duration: None,
            thumbnail: None,
            playlist: None, // Default to no playlist info
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
            playlist: None,
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

// How do I fix this? I want to have an array/vec of my structs implementing the 'AudioApi' trait so I can iterate through them and call then trait's functions. AI?
pub static AUDIO_APIS: Vec<Box<dyn AudioApi>> = [YoutubeApi, SpotifyApi];

#[async_trait]
pub trait AudioApi {
    // fn is_valid_url(url: &str) -> bool;
    async fn get_metadata(url: &str) -> Result<Vec<TrackMetadata>, MusicError>;
    // async fn to_audio_source(url: &str) -> Result<Vec<Input>, MusicError> {
    //     if Self::is_valid_url(url) {
    //         let metadata_vec = Self::get_metadata(url).await?;
    //         metadata_vec
    //             .into_iter()
    //             .map(|metadata| {
    //                 Ok(YoutubeDl::new(HTTP_CLIENT.clone(), metadata.url.unwrap()).into())
    //             })
    //             .collect()
    //     } else {
    //         Err(MusicError::AudioSourceError(format!(
    //             "Failed to create Input from url: {}",
    //             url
    //         )))
    //     }
    // }
}

/// Audio source utilities for handling different types of audio inputs
pub struct AudioSource;

impl AudioSource {
    /// Check if a string is a valid URL
    pub fn is_url(input: &str) -> bool {
        Url::parse(input).is_ok()
    }
}
