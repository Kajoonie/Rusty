use super::spotify_api::{SpotifyApi, SpotifyTrack};
use crate::HTTP_CLIENT;
use crate::commands::music::utils::music_manager::MusicError;
use crate::commands::music::utils::song_fetchers::{
    RelatedSongsFetcher, SerpApiFetcher, YtDlpFetcher,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
#[cfg(feature = "music")]
use songbird::input::{HttpRequest, Input, YoutubeDl};
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{debug, info};
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

static YOUTUBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:https?:)?//)?((?:www|m)\.)?((?:youtube\.com|youtu.be))(/(?:[\w\-]+\?v=|embed/|v/)?)([\w\-]+)(\S+)?$").unwrap()
});

/// Audio source utilities for handling different types of audio inputs
pub struct AudioSource;

impl AudioSource {
    /// Create an audio source from a URL or search term
    pub async fn from_query(
        query: &str,
        queue_track_callback: Option<Box<dyn Fn(Input, TrackMetadata) + Send + Sync>>,
    ) -> AudioSourceResult<(Input, TrackMetadata)> {
        debug!("Creating audio source from query: {}", query);
        // Check if the query is a URL
        if Self::is_url(query) {
            Self::from_url(query, queue_track_callback).await
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

    /// Check if a URL is a YouTube video URL and not a channel, playlist, or other type
    pub fn is_youtube_video_url(url: &str) -> bool {
        if !Self::is_youtube_url(url) {
            return false;
        }

        // YouTube videos typically have /watch?v= format or youtu.be/ format
        url.contains("/watch?v=") || url.contains("youtu.be/")
    }

    /// Create an audio source from a URL
    pub async fn from_url(
        url: &str,
        queue_track_callback: Option<Box<dyn Fn(Input, TrackMetadata) + Send + Sync>>,
    ) -> AudioSourceResult<(Input, TrackMetadata)> {
        debug!("Creating audio source from URL: {}", url);

        // Handle Spotify URLs
        if SpotifyApi::is_spotify_url(url) {
            return Self::from_spotify_url(url, queue_track_callback).await;
        }

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
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to execute yt-dlp: {}", e))
            })?;

        if !output.status.success() {
            return Err(MusicError::AudioSourceError(
                "yt-dlp is not properly installed".to_string(),
            ));
        }

        debug!(
            "yt-dlp version: {}",
            String::from_utf8_lossy(&output.stdout)
        );

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

        let metadata_str = String::from_utf8_lossy(&metadata_output.stdout);
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Create the source with default options (Songbird will use best audio quality)
        let source = YoutubeDl::new(HTTP_CLIENT.clone(), url.to_string());

        // Extract metadata from JSON
        let title = metadata_json["title"]
            .as_str()
            .unwrap_or("Unknown Title")
            .to_string();

        let duration = metadata_json["duration"]
            .as_f64()
            .map(Duration::from_secs_f64);

        let thumbnail = metadata_json["thumbnail"].as_str().map(|s| s.to_string());

        // Create metadata with extracted information
        let metadata = TrackMetadata {
            title,
            url: Some(url.to_string()),
            duration,
            thumbnail,
            playlist: None, // Individual YouTube URL, not a playlist context
        };

        Ok((source.into(), metadata))
    }

    /// Create an audio source from a search term using YouTube search
    pub async fn from_search(search_term: &str) -> AudioSourceResult<(Input, TrackMetadata)> {
        info!("Creating audio source from search term: {}", search_term);
        let search_url = format!("ytsearch:{}", search_term);

        // Get video metadata using yt-dlp
        let metadata_output = Command::new("yt-dlp")
            .args([
                "-j",            // Output as JSON
                "--no-playlist", // Don't process playlists
                &search_url,
            ])
            .output()
            .map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to get video metadata: {}", e))
            })?;

        let metadata_str = String::from_utf8_lossy(&metadata_output.stdout);
        let metadata_json: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                MusicError::AudioSourceError(format!("Failed to parse video metadata: {}", e))
            })?;

        // Create the source with default options
        let source = YoutubeDl::new(HTTP_CLIENT.clone(), search_url);

        // Extract metadata from JSON
        let title = metadata_json["title"]
            .as_str()
            .unwrap_or("Unknown Title")
            .to_string();

        let duration = metadata_json["duration"]
            .as_f64()
            .map(Duration::from_secs_f64);

        let thumbnail = metadata_json["thumbnail"].as_str().map(|s| s.to_string());

        let video_url = metadata_json["webpage_url"].as_str().map(|s| s.to_string());

        // Create metadata with extracted information
        let metadata = TrackMetadata {
            title,
            url: video_url,
            duration,
            thumbnail,
            playlist: None, // Search result, not a playlist context
        };

        Ok((source.into(), metadata))
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

    /// Create an audio source from a Spotify URL
    pub async fn from_spotify_url(
        url: &str,
        queue_track_callback: Option<Box<dyn Fn(Input, TrackMetadata) + Send + Sync>>,
    ) -> AudioSourceResult<(Input, TrackMetadata)> {
        info!("Creating audio source from Spotify URL: {}", url);

        // Determine the type of Spotify URL (track, playlist, album)
        if let Some(track_id) = SpotifyApi::extract_track_id(url) {
            // It's a track URL
            let track = SpotifyApi::get_track(&track_id).await?;
            return Self::from_spotify_track(track).await;
        } else if let Some(playlist_id) = SpotifyApi::extract_playlist_id(url) {
            // It's a playlist URL
            let (playlist_name, tracks) = SpotifyApi::get_playlist_tracks(&playlist_id).await?;
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify playlist is empty".to_string(),
                ));
            }

            // It's a playlist URL
            let (playlist_name, tracks) = SpotifyApi::get_playlist_tracks(&playlist_id).await?;
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify playlist is empty".to_string(),
                ));
            }

            let total_tracks = tracks.len();
            let first_track = tracks[0].clone();

            // Queue the remaining tracks if we have a callback
            if total_tracks > 1 {
                if let Some(callback) = queue_track_callback {
                    let remaining_tracks = tracks[1..].to_vec();
                    tokio::spawn(async move {
                        for track in remaining_tracks {
                            if let Ok((input, metadata)) = Self::from_spotify_track(track).await {
                                (callback)(input, metadata);
                            }
                        }
                    });
                }
            }

            // Get source and metadata for the *first* track
            let (source, mut metadata) = Self::from_spotify_track(first_track).await?;

            // Add playlist info to the metadata of the first track
            metadata.playlist = Some(PlaylistMetadata {
                title: playlist_name,
                track_count: total_tracks,
            });

            return Ok((source, metadata));
        } else if let Some(album_id) = SpotifyApi::extract_album_id(url) {
            // It's an album URL
            let (album_name, tracks) = SpotifyApi::get_album_tracks(&album_id).await?;
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify album is empty".to_string(),
                ));
            }

            let total_tracks = tracks.len();
            let first_track = tracks[0].clone();

            // Queue the remaining tracks if we have a callback
            if total_tracks > 1 {
                if let Some(callback) = queue_track_callback {
                    let remaining_tracks = tracks[1..].to_vec();
                    tokio::spawn(async move {
                        for track in remaining_tracks {
                            if let Ok((input, metadata)) = Self::from_spotify_track(track).await {
                                (callback)(input, metadata);
                            }
                        }
                    });
                }
            }

            // Get source and metadata for the *first* track
            let (source, mut metadata) = Self::from_spotify_track(first_track).await?;

            // Add album info to the metadata of the first track
            metadata.playlist = Some(PlaylistMetadata {
                // Reusing PlaylistMetadata for albums
                title: album_name,
                track_count: total_tracks,
            });

            return Ok((source, metadata));
        }

        Err(MusicError::AudioSourceError(
            "Invalid Spotify URL".to_string(),
        ))
    }

    /// Create an audio source from a Spotify track
    pub async fn from_spotify_track(
        track: SpotifyTrack,
    ) -> AudioSourceResult<(Input, TrackMetadata)> {
        // Create a search query for YouTube based on the Spotify track
        let search_query = SpotifyApi::get_youtube_search_query(&track);
        info!("Searching YouTube for Spotify track: {}", search_query);

        // Search for the track on YouTube
        let (source, _) = Self::from_search(&search_query).await?;

        // Create metadata from the Spotify track
        let duration = if track.duration_ms > 0 {
            Some(Duration::from_millis(track.duration_ms))
        } else {
            None
        };

        let artists_str = track.artists.join(", ");
        let title = if artists_str.is_empty() {
            track.name
        } else {
            format!("{} - {}", track.name, artists_str)
        };

        let metadata = TrackMetadata {
            title,
            url: Some(track.url), // Keep Spotify URL here for reference
            duration,
            thumbnail: track.album_image,
            playlist: None, // Individual track context, no playlist info here
        };

        // The source is from YouTube search, but metadata is from Spotify
        Ok((source, metadata))
    }
}
