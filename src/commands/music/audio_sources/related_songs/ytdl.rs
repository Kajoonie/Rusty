use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::audio_sources::youtube::YoutubeApi;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use std::process::Command;
use std::time::Duration;

use super::{RelatedSongsFetcher, RelatedSongsResult};

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
                    if video_url == &orig_url || !YoutubeApi::is_youtube_url(video_url) {
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
                    requested_by: Some("Autoplay".into()),
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
