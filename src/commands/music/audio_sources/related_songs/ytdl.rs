//! Implements the `RelatedSongsFetcher` trait using `yt-dlp` command-line tool.
//! This serves as a fallback if other methods (like SerpAPI) are unavailable or fail.

use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::audio_sources::youtube::YoutubeApi;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use std::process::Command;
use std::time::Duration;

use super::{RelatedSongsFetcher, RelatedSongsResult};

/// Implements `RelatedSongsFetcher` by using `yt-dlp`'s search functionality.
/// It derives a search term from the original video's title.
pub struct YtDlpFetcher;

impl YtDlpFetcher {
    /// Creates a new `YtDlpFetcher`.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RelatedSongsFetcher for YtDlpFetcher {
    /// Implementation of `fetch_related_songs` using `yt-dlp`.
    ///
    /// 1. Fetches metadata of the original video to get its title.
    /// 2. Derives a search term based on the title.
    /// 3. Uses `yt-dlp` search (`ytsearch5:...`) to find related videos.
    /// 4. Parses the output (JSON lines) and converts results to `TrackMetadata`,
    ///    skipping the original video and limiting to 5 results.
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult {
        // Construct the URL for the original video.
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        // Fetch metadata of the original video using yt-dlp.
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

        // Parse the metadata JSON.
        let metadata_str = String::from_utf8_lossy(&output.stdout);
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Extract the title to use for deriving a search term.
        let title = metadata_json["title"].as_str().unwrap_or("").to_string();

        // Attempt to create a reasonable search term from the title.
        let search_term = if title.contains(" - ") {
            // If title seems like 'Artist - Song', search for 'Artist music'.
            title.split(" - ").next().unwrap_or(&title).to_string() + " music"
        } else {
            // Otherwise, use the first few words or just 'music'.
            let words: Vec<&str> = title.split_whitespace().collect();
            if words.len() > 2 {
                words[0..2].join(" ")
            } else {
                "music".to_string()
            }
        };

        // Perform the YouTube search using yt-dlp.
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

        // Store the original URL to avoid adding it to related songs.
        let search_str = String::from_utf8_lossy(&search_output.stdout);
        let orig_url = url.clone();

        // Process the search results (each line is a JSON object).
        let mut related_songs = Vec::new();
        for line in search_str.lines() {
            // Try parsing the line as JSON.
            if let Ok(video_json) = serde_json::from_str::<serde_json::Value>(line) {
                // Extract the URL from the search result.
                let video_url = video_json["webpage_url"].as_str().map(|s| s.to_string());

                    // Check if the URL matches the original video or isn't a valid YouTube URL.
                if let Some(ref video_url) = video_url {
                    // Skip original video or non-video URLs (like channels)
                    if video_url == &orig_url || !YoutubeApi::is_youtube_url(video_url) {
                        continue;
                    }
                }

                // Extract metadata fields from the JSON.
                let title = video_json["title"]
                    .as_str()
                    .unwrap_or("Unknown Title")
                    .to_string();

                let duration = video_json["duration"].as_f64().map(Duration::from_secs_f64);

                let thumbnail = video_json["thumbnail"].as_str().map(|s| s.to_string());

                // Create TrackMetadata and add to the list.
                related_songs.push(TrackMetadata {
                    title,
                    url: video_url,
                    duration,
                    thumbnail,
                    requested_by: Some("Autoplay".into()),
                });

                // Stop after collecting 5 related songs.
                if related_songs.len() >= 5 {
                    break;
                }
            }
        }

        Ok(related_songs)
    }
}
