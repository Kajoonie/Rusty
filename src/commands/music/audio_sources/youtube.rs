//! Implements the `AudioApi` trait for fetching metadata from YouTube.
//! Uses `yt-dlp` command-line tool for extracting information.
//! Also includes logic for finding related songs.

use super::related_songs::serp_api::{RealSerpApiSearcher, SerpApiFetcher};
use std::sync::Arc;

use crate::commands::music::{
    audio_sources::related_songs::{
        RelatedSongsFetcher, ytdl::YtDlpFetcher,
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

/// Regex to match and capture YouTube video URLs (various formats).
static YOUTUBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:https?:)?//)?((?:www|m)\.)?((?:youtube\.com|youtu.be))(/(?:[\w\-]+\?v=|embed/|v/)?)([\w\-]+)(\S+)?$").unwrap()
});

/// The main struct implementing YouTube API logic (via `yt-dlp`).
#[derive(Default)]
pub struct YoutubeApi;


#[async_trait]
impl AudioApi for YoutubeApi {
    /// Implementation of `is_valid_url` for the `AudioApi` trait.
    /// Delegates to the `is_youtube_url` helper function.
    fn is_valid_url(&self, url: &str) -> bool {
        YoutubeApi::is_youtube_url(url)
    }

    /// Implementation of `get_metadata` for the `AudioApi` trait.
    ///
    /// Fetches metadata for a single YouTube video URL using `yt-dlp`.
    /// Note: This implementation currently ignores playlists specified in the URL
    /// due to the `--no-playlist` flag passed to `yt-dlp`.
    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError> {
        info!("Creating YouTube audio source for URL: {}", url);

        // Execute yt-dlp to get metadata as JSON for the given URL.
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

        // Convert the yt-dlp output (JSON) into TrackMetadata.
        let metadata = TrackMetadata::from_youtube(metadata_output, requestor_name)?;

        // Return the metadata wrapped in a Vec (as required by the trait).
        Ok(vec![metadata])
    }
}

impl YoutubeApi {
    /// Checks if the input string is a valid YouTube URL (watch page or youtu.be).
    pub fn is_youtube_url(query: &str) -> bool {
        // Try parsing the input as a URL.
        match Url::parse(query) {
            Ok(url) => {
                // Check if the host matches known YouTube domains.
                url.host_str().is_some_and(|host| {
                    host == "www.youtube.com" || host == "youtube.com" || host == "youtu.be"
                    // Check if it's a standard watch page or a short youtu.be link.
                }) && url.path().starts_with("/watch")
                    || url.host_str() == Some("youtu.be")
                // Basic check, might need refinement for shorts, playlists etc. if needed
            }
            // If parsing fails, it's not a valid URL.
            Err(_) => false,
        }
    }

    /// Fetches metadata for the first YouTube search result for a given search term.
    /// Uses `yt-dlp` with the `ytsearch:` prefix.
    pub fn from_search(search_term: &str) -> Result<TrackMetadata, MusicError> {
        info!("Creating audio source from search term: {}", search_term);
        // Format the search term for yt-dlp.
        let search_param = format!("ytsearch:{}", search_term);

        // Execute yt-dlp to get metadata for the first search result.
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

        // Convert the yt-dlp output to TrackMetadata.
        TrackMetadata::try_from(metadata_output)
    }

    /// Extracts the video ID from various YouTube URL formats using regex.
    fn extract_video_id(url: &str) -> AudioSourceResult<String> {
        // Try to match the URL against the regex.
        if let Some(captures) = YOUTUBE_REGEX.captures(url) {
            // Extract the 5th capture group (the video ID).
            if let Some(id) = captures.get(5).map(|m| m.as_str().to_string()) {
                Ok(id)
            } else {
                // Error if ID couldn't be extracted.
                Err(MusicError::AudioSourceError(
                    "Could not extract video ID".to_string(),
                ))
            }
        } else {
            // Error if the URL didn't match the regex.
            Err(MusicError::AudioSourceError(
                "Not a valid YouTube URL".to_string(),
            ))
        }
    }

    /// Fetches a list of related songs for a given YouTube video URL.
    ///
    /// Tries to use the SerpAPI first if the `SERP_API_KEY` environment variable is set.
    /// Falls back to using `yt-dlp` if SerpAPI is unavailable or returns no results.
    pub async fn get_related_songs(url: &str) -> AudioSourceResult<Vec<TrackMetadata>> {
        info!("Fetching related songs for URL: {}", url);

        // Extract the video ID required by the fetchers.
        let video_id = Self::extract_video_id(url)?;

        // Check if SerpAPI key is available.
        if let Ok(serp_api_key) = std::env::var("SERP_API_KEY") {
            // Create the SerpAPI fetcher.
            let real_searcher = Arc::new(RealSerpApiSearcher::new(serp_api_key));
          
            // Specify the generic type for SerpApiFetcher
            let serp_fetcher: SerpApiFetcher<RealSerpApiSearcher> = SerpApiFetcher::new(real_searcher);
            // Attempt to fetch related songs via SerpAPI.
            let related_songs = serp_fetcher.fetch_related_songs(&video_id).await?;

            // If SerpAPI returned results, use them.
            if !related_songs.is_empty() {
                return Ok(related_songs);
            }
        }

        // If SerpAPI failed or is unavailable, use the yt-dlp fetcher.
        let ytdlp_fetcher = YtDlpFetcher::new();
        ytdlp_fetcher.fetch_related_songs(&video_id).await
    }
}
