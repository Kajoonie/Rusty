use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use serpapi_search_rust::serp_api_search::SerpApiSearch;
use std::collections::HashMap;
use std::time::Duration;

use super::{RelatedSongsFetcher, RelatedSongsResult};

/// SerpAPI implementation for fetching related songs
pub struct SerpApiFetcher {
    api_key: String,
}

impl SerpApiFetcher {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }

    /// Helper function to parse YouTube duration strings (e.g. "5:32" or "1:23:45")
    fn parse_duration_string(duration_str: &str) -> Option<Duration> {
        let parts: Vec<&str> = duration_str.split(':').collect();

        match parts.len() {
            // MM:SS format
            2 => {
                let minutes = parts[0].parse::<u64>().ok()?;
                let seconds = parts[1].parse::<u64>().ok()?;
                Some(Duration::from_secs(minutes * 60 + seconds))
            }
            // HH:MM:SS format
            3 => {
                let hours = parts[0].parse::<u64>().ok()?;
                let minutes = parts[1].parse::<u64>().ok()?;
                let seconds = parts[2].parse::<u64>().ok()?;
                Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
            }
            _ => None,
        }
    }
}

#[async_trait]
impl RelatedSongsFetcher for SerpApiFetcher {
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult {
        // Set up the SerpAPI parameters
        let mut params: HashMap<String, String> = HashMap::new();
        params.insert("v".to_string(), video_id.to_string());

        // Create the SerpAPI search object with correct parameters
        let search = SerpApiSearch::new("youtube_video".to_string(), params, self.api_key.clone());

        let results = search
            .json()
            .await
            .map_err(|e| MusicError::AudioSourceError(format!("SerpAPI request failed: {}", e)))?;

        let mut related_songs = Vec::new();

        // Check if the response contains related videos
        if let Some(related_videos) = results.get("related_videos").and_then(|v| v.as_array()) {
            for video in related_videos {
                if let (Some(title), Some(link)) = (
                    video.get("title").and_then(|t| t.as_str()),
                    video.get("link").and_then(|l| l.as_str()),
                ) {
                    // Extract duration if available
                    let duration = video
                        .get("length")
                        .and_then(|d| d.as_str())
                        .and_then(Self::parse_duration_string);

                    // Extract thumbnail if available
                    let thumbnail = video
                        .get("thumbnail")
                        .and_then(|t| t.get("static"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());

                    related_songs.push(TrackMetadata {
                        title: title.to_string(),
                        url: Some(link.to_string()),
                        duration,
                        thumbnail,
                        requested_by: Some("Autoplay".into()),
                    });

                    // Limit to 5 related videos
                    if related_songs.len() >= 5 {
                        break;
                    }
                }
            }
        }

        Ok(related_songs)
    }
}
