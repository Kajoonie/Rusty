use crate::commands::music::utils::audio_sources::{AudioSource, TrackMetadata};
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use serpapi_search_rust::serp_api_search::SerpApiSearch;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

/// Result type for fetching related songs
pub type RelatedSongsResult = Result<Vec<TrackMetadata>, MusicError>;

/// Trait defining the interface for fetching related songs
#[async_trait]
pub trait RelatedSongsFetcher {
    /// Fetch related songs for a given video ID
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult;
}

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
                        playlist: None, // Add missing playlist field
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

/// yt-dlp implementation for fetching related songs
pub struct YtDlpFetcher;

impl YtDlpFetcher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RelatedSongsFetcher for YtDlpFetcher {
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult {
        // First, get information about the current video
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        let output = Command::new("yt-dlp")
            .args([
                "-j", // Output metadata as JSON
                "--no-playlist",
                &url,
            ])
            .output()
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to get video metadata: {}", e))
            })?;

        let metadata_str = String::from_utf8_lossy(&output.stdout);
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Extract title for search
        let title = metadata_json["title"].as_str().unwrap_or("").to_string();

        // Use title as search term
        let search_term = if title.contains(" - ") {
            // Likely artist - title format, use artist
            title.split(" - ").next().unwrap_or(&title).to_string() + " music"
        } else {
            // Use whole title or part of it
            let words: Vec<&str> = title.split_whitespace().collect();
            if words.len() > 2 {
                words[0..2].join(" ")
            } else {
                "music".to_string()
            }
        };

        // Use yt-dlp to search for videos
        let search_output = Command::new("yt-dlp")
            .args([
                "-j",              // Output metadata as JSON
                "--flat-playlist", // Don't get full metadata for each video
                "--no-download",
                "--default-search",
                "ytsearch5", // Search for 5 videos
                &search_term,
            ])
            .output()
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to search for related videos: {}", e))
            })?;

        let search_str = String::from_utf8_lossy(&search_output.stdout);
        let orig_url = url.clone();

        // Parse each line as a separate JSON object (one per video)
        let mut related_songs = Vec::new();
        for line in search_str.lines() {
            if let Ok(video_json) = serde_json::from_str::<serde_json::Value>(line) {
                // Skip the original video if it appears in results
                let video_url = video_json["webpage_url"].as_str().map(|s| s.to_string());

                if let Some(ref video_url) = video_url {
                    // Skip original video or non-video URLs (like channels)
                    if video_url == &orig_url || !AudioSource::is_youtube_video_url(video_url) {
                        continue;
                    }
                }

                // Extract metadata from JSON
                let title = video_json["title"]
                    .as_str()
                    .unwrap_or("Unknown Title")
                    .to_string();

                let duration = video_json["duration"].as_f64().map(Duration::from_secs_f64);

                let thumbnail = video_json["thumbnail"].as_str().map(|s| s.to_string());

                // Add to related songs
                related_songs.push(TrackMetadata {
                    title,
                    url: video_url,
                    duration,
                    thumbnail,
                });

                // Stop if we have enough related songs
                if related_songs.len() >= 5 {
                    break;
                }
            }
        }

        Ok(related_songs)
    }
}
