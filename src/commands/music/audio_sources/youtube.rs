use crate::commands::music::{
    audio_sources::related_songs::{
        RelatedSongsFetcher, serp_api::SerpApiFetcher, ytdl::YtDlpFetcher,
    },
    utils::music_manager::MusicError,
};
use regex::Regex;
use serenity::async_trait;
use tracing::info;

#[cfg(feature = "music")]
use std::process::Command;
use std::sync::LazyLock;
use url::Url;

use super::{AudioApi, AudioSourceResult, TrackMetadata};

static YOUTUBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:https?:)?//)?((?:www|m)\.)?((?:youtube\.com|youtu.be))(/(?:[\w\-]+\?v=|embed/|v/)?)([\w\-]+)(\S+)?$").unwrap()
});

pub struct YoutubeApi;

impl Default for YoutubeApi {
    fn default() -> Self {
        Self {}
    }
}

#[async_trait]
impl AudioApi for YoutubeApi {
    fn is_valid_url(&self, url: &str) -> bool {
        YoutubeApi::is_youtube_url(url)
    }

    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError> {
        info!("Creating YouTube audio source for URL: {}", url);

        // Get video metadata using yt-dlp
        let metadata_output = Command::new("yt-dlp")
            .args([
                "-j",            // Output as JSON
                "--no-playlist", // Don't process playlists
                url,
            ])
            .output()
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to get video metadata: {}", e))
            })?;

        let metadata = TrackMetadata::from_youtube(metadata_output, requestor_name)?;

        Ok(vec![metadata])
    }
}

impl YoutubeApi {
    /// Checks if a given string is a valid YouTube video URL.
    pub fn is_youtube_url(query: &str) -> bool {
        match Url::parse(query) {
            Ok(url) => {
                url.host_str().is_some_and(|host| {
                    host == "www.youtube.com" || host == "youtube.com" || host == "youtu.be"
                }) && url.path().starts_with("/watch")
                    || url.host_str() == Some("youtu.be")
                // Basic check, might need refinement for shorts, playlists etc. if needed
            }
            Err(_) => false,
        }
    }

    pub fn from_search(search_term: &str) -> Result<TrackMetadata, MusicError> {
        info!("Creating audio source from search term: {}", search_term);
        let search_param = format!("ytsearch:{}", search_term);

        // Get video metadata using yt-dlp
        let metadata_output = Command::new("yt-dlp")
            .args([
                "-j",            // Output as JSON
                "--no-playlist", // Don't process playlists
                &search_param,
            ])
            .output()
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to get video metadata: {}", e))
            })?;

        TrackMetadata::try_from(metadata_output)
    }

    /// Extract the video ID from a YouTube URL
    fn extract_video_id(url: &str) -> AudioSourceResult<String> {
        if let Some(captures) = YOUTUBE_REGEX.captures(url) {
            if let Some(id) = captures.get(5).map(|m| m.as_str().to_string()) {
                Ok(id)
            } else {
                Err(MusicError::AudioSourceError(
                    "Could not extract video ID".to_string(),
                ))
            }
        } else {
            Err(MusicError::AudioSourceError(
                "Not a valid YouTube URL".to_string(),
            ))
        }
    }

    /// Get related songs for a given YouTube video URL
    pub async fn get_related_songs(url: &str) -> AudioSourceResult<Vec<TrackMetadata>> {
        info!("Fetching related songs for URL: {}", url);

        // Extract video ID from URL
        let video_id = Self::extract_video_id(url)?;

        // Try using SerpAPI first
        if let Ok(serp_api_key) = std::env::var("SERP_API_KEY") {
            let serp_fetcher = SerpApiFetcher::new(serp_api_key);
            let related_songs = serp_fetcher.fetch_related_songs(&video_id).await?;

            if !related_songs.is_empty() {
                return Ok(related_songs);
            }
        }

        // Fallback to yt-dlp
        let ytdlp_fetcher = YtDlpFetcher::new();
        ytdlp_fetcher.fetch_related_songs(&video_id).await
    }
}
