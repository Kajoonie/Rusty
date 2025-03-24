use songbird::input::{
    cached::Compressed, 
    Input,
    YoutubeDl,
    HttpRequest,
};
use songbird::driver::Bitrate;
use url::Url;
use crate::commands::music::utils::music_manager::MusicError;
use std::time::Duration;
use regex::Regex;
use lazy_static::lazy_static;
use reqwest::Client;
use tracing::{info, debug};
use std::process::Command;

/// Result type for audio source operations
pub type AudioSourceResult<T> = Result<T, MusicError>;

/// Represents metadata for a track
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub url: Option<String>,
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

lazy_static! {
    static ref YOUTUBE_REGEX: Regex = Regex::new(
        r"^((?:https?:)?//)?((?:www|m)\.)?((?:youtube\.com|youtu.be))(/(?:[\w\-]+\?v=|embed/|v/)?)([\w\-]+)(\S+)?$"
    ).unwrap();
    
    // Create a shared HTTP client for reuse
    static ref HTTP_CLIENT: Client = Client::new();
}

/// Audio source utilities for handling different types of audio inputs
pub struct AudioSource;

impl AudioSource {
    /// Create an audio source from a URL or search term
    pub async fn from_query(query: &str) -> AudioSourceResult<(Input, TrackMetadata)> {
        debug!("Creating audio source from query: {}", query);
        // Check if the query is a URL
        if Self::is_url(query) {
            Self::from_url(query).await
        } else {
            // Treat as a search term
            Self::from_search(query).await
        }
    }

    /// Check if a string is a valid URL
    pub fn is_url(input: &str) -> bool {
        Url::parse(input).is_ok()
    }

    /// Check if a URL is a YouTube URL
    pub fn is_youtube_url(url: &str) -> bool {
        YOUTUBE_REGEX.is_match(url)
    }

    /// Create an audio source from a URL
    pub async fn from_url(url: &str) -> AudioSourceResult<(Input, TrackMetadata)> {
        debug!("Creating audio source from URL: {}", url);
        // Handle YouTube URLs with ytdl
        if Self::is_youtube_url(url) {
            return Self::from_youtube_url(url).await;
        }

        // Handle direct audio URLs
        let source = HttpRequest::new(HTTP_CLIENT.clone(), url.to_string());

        // Create basic metadata
        let metadata = TrackMetadata {
            title: url.to_string(),
            url: Some(url.to_string()),
            ..Default::default()
        };

        Ok((source.into(), metadata))
    }

    /// Create an audio source from a YouTube URL
    pub async fn from_youtube_url(url: &str) -> AudioSourceResult<(Input, TrackMetadata)> {
        info!("Creating YouTube audio source for URL: {}", url);

        // First, verify yt-dlp is working
        let output = Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map_err(|e| MusicError::AudioSourceError(format!("Failed to execute yt-dlp: {}", e)))?;

        if !output.status.success() {
            return Err(MusicError::AudioSourceError("yt-dlp is not properly installed".to_string()));
        }

        debug!("yt-dlp version: {}", String::from_utf8_lossy(&output.stdout));
        
        // Create the source with default options (Songbird will use best audio quality)
        let source = YoutubeDl::new(HTTP_CLIENT.clone(), url.to_string());

        // Create basic metadata since we can't easily get it from YouTube
        let metadata = TrackMetadata {
            title: url.to_string(),
            url: Some(url.to_string()),
            duration: None,
            thumbnail: None,
        };

        Ok((source.into(), metadata))
    }

    /// Create an audio source from a search term using YouTube search
    pub async fn from_search(search_term: &str) -> AudioSourceResult<(Input, TrackMetadata)> {
        info!("Creating audio source from search term: {}", search_term);
        // Use YoutubeDl with search prefix for YouTube search
        let search_url = format!("ytsearch:{}", search_term);
        
        // Create the source with default options
        let source = YoutubeDl::new(HTTP_CLIENT.clone(), search_url.clone());

        // Create basic metadata
        let metadata = TrackMetadata {
            title: format!("Search result for: {}", search_term),
            url: None,
            duration: None,
            thumbnail: None,
        };

        Ok((source.into(), metadata))
    }

    /// Create a cached audio source to improve performance for frequently played tracks
    pub async fn cached_source(input: Input) -> AudioSourceResult<Input> {
        match Compressed::new(input, Bitrate::Auto).await {
            Ok(cached) => Ok(cached.into()),
            Err(e) => Err(MusicError::AudioSourceError(format!("Failed to create cached source: {}", e))),
        }
    }
}
