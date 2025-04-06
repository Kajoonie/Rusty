//! Implements the `AudioApi` trait for fetching metadata from Spotify.
//! Handles authentication (client credentials flow), URL parsing, and API requests.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use regex::Regex;
use reqwest::header;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use std::env;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::info;

use crate::HTTP_CLIENT;
use crate::commands::music::utils::music_manager::MusicError;

use super::{AudioApi, TrackMetadata};

/// Result type specific to Spotify API operations.
pub type SpotifyResult<T> = Result<T, MusicError>;

/// Represents basic track information retrieved from Spotify.
#[derive(Clone, Debug)]
pub struct SpotifyTrack {
    /// The name of the track.
    pub name: String,
    /// A list of artist names associated with the track.
    pub artists: Vec<String>,
}

/// Represents the response from Spotify's token endpoint.
#[derive(Debug, Serialize, Deserialize)]
struct SpotifyToken {
    /// The OAuth2 access token.
    access_token: String,
    /// The type of token (usually "Bearer").
    token_type: String,
    /// The duration in seconds for which the token is valid.
    expires_in: u64,
    /// The time when the token was created, used to check expiry.
    #[serde(skip, default = "Instant::now")]
    created_at: Instant,
}

impl SpotifyToken {
    /// Checks if the token has expired or is close to expiring.
    /// Considers the token expired 30 seconds before its actual expiry time
    /// to provide a buffer.
    fn is_expired(&self) -> bool {
        let expiry = Duration::from_secs(self.expires_in);
        let elapsed = self.created_at.elapsed();
        // Consider it expired 30 seconds before actual expiry
        elapsed > expiry.saturating_sub(Duration::from_secs(30))
    }
}

/// Lazily initialized, thread-safe storage for the Spotify API access token.
/// Uses a Mutex to allow safe concurrent access and updates.
static SPOTIFY_TOKEN: LazyLock<Arc<Mutex<Option<SpotifyToken>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

/// Regex to match and capture Spotify track URLs.
static SPOTIFY_TRACK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/track/([a-zA-Z0-9]+)(\?.*)?$").unwrap()
});

/// Regex to match and capture Spotify playlist URLs.
static SPOTIFY_PLAYLIST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/playlist/([a-zA-Z0-9]+)(\?.*)?$")
        .unwrap()
});

/// Regex to match and capture Spotify album URLs.
static SPOTIFY_ALBUM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/album/([a-zA-Z0-9]+)(\?.*)?$").unwrap()
});

/// The main struct implementing Spotify API logic.
#[derive(Default)]
pub struct SpotifyApi;


impl SpotifyApi {
    /// Checks if the provided URL matches any of the known Spotify URL patterns
    /// (track, playlist, album).
    pub fn is_spotify_url(url: &str) -> bool {
        SPOTIFY_TRACK_REGEX.is_match(url)
            || SPOTIFY_PLAYLIST_REGEX.is_match(url)
            || SPOTIFY_ALBUM_REGEX.is_match(url)
    }

    /// Attempts to extract the Spotify track ID from a URL using the track regex.
    pub fn extract_track_id(url: &str) -> Option<String> {
        SPOTIFY_TRACK_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Attempts to extract the Spotify playlist ID from a URL using the playlist regex.
    pub fn extract_playlist_id(url: &str) -> Option<String> {
        SPOTIFY_PLAYLIST_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Attempts to extract the Spotify album ID from a URL using the album regex.
    pub fn extract_album_id(url: &str) -> Option<String> {
        SPOTIFY_ALBUM_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Retrieves a valid Spotify API access token.
    ///
    /// Checks the cached token first. If it's missing or expired, requests a new one
    /// using the client credentials flow with `SPOTIFY_CLIENT_ID` and `SPOTIFY_CLIENT_SECRET`
    /// environment variables. Caches the new token upon success.
    async fn get_access_token() -> SpotifyResult<String> {
        // Acquire lock on the token cache.
        let mut token_lock = SPOTIFY_TOKEN.lock().await;

        // If a valid token exists in the cache, clone and return it.
        if let Some(token) = &*token_lock {
            if !token.is_expired() {
                return Ok(token.access_token.clone());
            }
        }

        // No valid token in cache, need to request a new one.
        let client_id = env::var("SPOTIFY_CLIENT_ID")
            .map_err(|_| MusicError::ConfigError("SPOTIFY_CLIENT_ID not set".to_string()))?;
        let client_secret = env::var("SPOTIFY_CLIENT_SECRET")
            .map_err(|_| MusicError::ConfigError("SPOTIFY_CLIENT_SECRET not set".to_string()))?;

        // Encode client ID and secret for Basic auth.
        let auth = BASE64_STANDARD.encode(format!("{}:{}", client_id, client_secret));
        let auth_header = format!("Basic {}", auth);

        let params = [("grant_type", "client_credentials")];
        // Make the POST request to Spotify's token endpoint.
        let response = HTTP_CLIENT
            .post("https://accounts.spotify.com/api/token")
            .header(header::AUTHORIZATION, auth_header)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify token: {}", e))
            })?;

        // Check if the request was successful.
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Cannot read response".to_string());
            return Err(MusicError::ExternalApiError(format!(
                "Spotify API error: {} - {}",
                status, text
            )));
        }

        // Parse the successful JSON response.
        let token_response = response.json::<SpotifyToken>().await.map_err(|e| {
            MusicError::ExternalApiError(format!("Failed to parse Spotify token: {}", e))
        })?;

        let access_token = token_response.access_token.clone();
        // Store the new token in the cache (releasing the lock implicitly).
        *token_lock = Some(token_response);

        Ok(access_token)
    }

    /// Fetches detailed information for a single Spotify track by its ID.
    pub async fn get_track(track_id: &str) -> SpotifyResult<SpotifyTrack> {
        // Get a valid access token.
        let token = Self::get_access_token().await?;
        // Construct the API URL for the specific track.
        let url = format!("https://api.spotify.com/v1/tracks/{}", track_id);

        // Make the GET request to the Spotify API.
        let response = HTTP_CLIENT
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify track: {}", e))
            })?;

        // Handle potential HTTP errors.
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Cannot read response".to_string());
            return Err(MusicError::ExternalApiError(format!(
                "Spotify API error: {} - {}",
                status, text
            )));
        }

        // Parse the JSON response.
        let track_data: serde_json::Value = response.json().await.map_err(|e| {
            MusicError::ExternalApiError(format!("Failed to parse Spotify track data: {}", e))
        })?;

        // Extract the track name.
        let name = track_data["name"]
            .as_str()
            .ok_or_else(|| MusicError::ExternalApiError("Missing track name".to_string()))?
            .to_string();

        // Extract the list of artist names.
        let artists = track_data["artists"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(SpotifyTrack { name, artists })
    }

    /// Fetches all tracks from a Spotify playlist by its ID.
    /// Handles pagination automatically.
    pub async fn get_playlist_tracks(playlist_id: &str) -> SpotifyResult<Vec<SpotifyTrack>> {
        // Get a valid access token.
        let token = Self::get_access_token().await?;

        let mut tracks = Vec::new();

        // Initialize the URL for the first page of playlist tracks.
        let mut tracks_url = format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?limit=50",
            playlist_id
        );

        // Loop to handle pagination.
        loop {
            // Make the GET request for the current page of tracks.
            let response = HTTP_CLIENT
                .get(&tracks_url)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| {
                    MusicError::ExternalApiError(format!(
                        "Failed to request Spotify playlist: {}",
                        e
                    ))
                })?;

            // Handle potential HTTP errors.
            if !response.status().is_success() {
                let status = response.status();
                let text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Cannot read response".to_string());
                return Err(MusicError::ExternalApiError(format!(
                    "Spotify API error: {} - {}",
                    status, text
                )));
            }

            // Parse the JSON response for the current page.
            let playlist_data: serde_json::Value = response.json().await.map_err(|e| {
                MusicError::ExternalApiError(format!(
                    "Failed to parse Spotify playlist data: {}",
                    e
                ))
            })?;

            // Iterate through the items (tracks) on the current page.
            if let Some(items) = playlist_data["items"].as_array() {
                for item in items {
                    // Ensure the item contains track data.
                    if let Some(track) = item["track"].as_object() {
                        // Skip tracks without a valid Spotify ID (e.g., local files).
                        if track.get("id").is_none() || track["id"].is_null() {
                            continue; // Skip local tracks that don't have Spotify IDs
                        }

                        // Extract track name.
                        let name = track["name"]
                            .as_str()
                            .ok_or_else(|| {
                                MusicError::ExternalApiError("Missing track name".to_string())
                            })?
                            .to_string();

                        // Extract artist names.
                        let artists = track["artists"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        // Add the extracted track info to the results.
                        tracks.push(SpotifyTrack { name, artists });
                    }
                }
            }

            // Check if there's a URL for the next page.
            if let Some(next_url) = playlist_data["next"].as_str() {
                // Update the URL for the next iteration.
                tracks_url = next_url.to_string();
            } else {
                // No more pages, break the loop.
                break;
            }
        }

        Ok(tracks)
    }

    /// Fetches all tracks from a Spotify album by its ID.
    /// Handles pagination automatically.
    pub async fn get_album_tracks(album_id: &str) -> SpotifyResult<Vec<SpotifyTrack>> {
        // Get a valid access token.
        let token = Self::get_access_token().await?;
        let mut tracks = Vec::new();

        // Fetch basic album details first (might be useful for album name/image later, though not used currently).
        let album_url = format!("https://api.spotify.com/v1/albums/{}", album_id);
        let album_response = HTTP_CLIENT
            .get(&album_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify album: {}", e))
            })?;

        // Handle potential HTTP errors during album detail fetch.
        if !album_response.status().is_success() {
            let status = album_response.status();
            let text = album_response
                .text()
                .await
                .unwrap_or_else(|_| "Cannot read response".to_string());
            return Err(MusicError::ExternalApiError(format!(
                "Spotify API error: {} - {}",
                status, text
            )));
        }

        // Initialize the URL for the first page of album tracks.
        let mut url = format!(
            "https://api.spotify.com/v1/albums/{}/tracks?limit=50",
            album_id
        );

        // Loop to handle pagination for tracks.
        loop {
            // Make the GET request for the current page of tracks.
            let response = HTTP_CLIENT
                .get(&url)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| {
                    MusicError::ExternalApiError(format!(
                        "Failed to request Spotify album tracks: {}",
                        e
                    ))
                })?;

            // Handle potential HTTP errors during track fetch.
            if !response.status().is_success() {
                let status = response.status();
                let text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Cannot read response".to_string());
                return Err(MusicError::ExternalApiError(format!(
                    "Spotify API error: {} - {}",
                    status, text
                )));
            }

            // Parse the JSON response for the current page of tracks.
            let tracks_data: serde_json::Value = response.json().await.map_err(|e| {
                MusicError::ExternalApiError(format!(
                    "Failed to parse Spotify album tracks data: {}",
                    e
                ))
            })?;

            // Iterate through the items (tracks) on the current page.
            if let Some(items) = tracks_data["items"].as_array() {
                for track in items {
                    // Skip tracks without a valid Spotify ID.
                    if track.get("id").is_none() || track["id"].is_null() {
                        continue;
                    }

                    // Extract track name.
                    let name = track["name"]
                        .as_str()
                        .ok_or_else(|| {
                            MusicError::ExternalApiError("Missing track name".to_string())
                        })?
                        .to_string();

                    // Extract artist names.
                    let artists = track["artists"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    // Add the extracted track info to the results.
                    tracks.push(SpotifyTrack { name, artists });
                }
            }

            // Check if there's a URL for the next page of tracks.
            if let Some(next_url) = tracks_data["next"].as_str() {
                // Update the URL for the next iteration.
                url = next_url.to_string();
            } else {
                // No more pages, break the loop.
                break;
            }
        }

        Ok(tracks)
    }

    /// Creates a suitable YouTube search query string from Spotify track details
    /// (e.g., "Track Name by Artist1, Artist2 audio").
    /// Get search query for YouTube from a Spotify track
    pub fn get_youtube_search_query(track: &SpotifyTrack) -> String {
        let artists_str = track.artists.join(", ");
        format!("{} by {} audio", track.name, artists_str)
    }
}

#[async_trait]
impl AudioApi for SpotifyApi {
    /// Implementation of `is_valid_url` for the `AudioApi` trait.
    /// Delegates to the `is_spotify_url` helper function.
    fn is_valid_url(&self, url: &str) -> bool {
        SpotifyApi::is_spotify_url(url)
    }

    /// Implementation of `get_metadata` for the `AudioApi` trait.
    ///
    /// Determines if the URL is a track, playlist, or album, fetches the corresponding
    /// data using Spotify API methods, and converts the results into `TrackMetadata`.
    /// For tracks/playlists/albums, it generates YouTube search queries.
    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError> {
        info!("Creating audio source from Spotify URL: {}", url);

        // Check if it's a single track URL.
        if let Some(track_id) = SpotifyApi::extract_track_id(url) {
            // Fetch single track data.
            let track = SpotifyApi::get_track(&track_id).await?;
            // Attempt to convert Spotify track data to unified TrackMetadata.
            match TrackMetadata::try_from(track) {
                Ok(metadata) => return Ok(vec![metadata]),
                Err(e) => return Err(e),
            }
        // Check if it's a playlist URL.
        } else if let Some(playlist_id) = SpotifyApi::extract_playlist_id(url) {
            // Fetch all tracks from the playlist.
            let tracks = SpotifyApi::get_playlist_tracks(&playlist_id).await?;
            // Handle empty playlist case.
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify playlist is empty".to_string(),
                ));
            }

            // Convert each Spotify track in the playlist to TrackMetadata.
            let metadata = tracks
                .into_iter()
                .map(|track| {
                    TrackMetadata::from_spotify(track, requestor_name.clone()).unwrap_or_default()
                })
                .collect();

            return Ok(metadata);
        // Check if it's an album URL.
        } else if let Some(album_id) = SpotifyApi::extract_album_id(url) {
            // Fetch all tracks from the album.
            let tracks = SpotifyApi::get_album_tracks(&album_id).await?;
            // Handle empty album case.
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify album is empty".to_string(),
                ));
            }

            // Convert each Spotify track in the album to TrackMetadata.
            let metadata = tracks
                .into_iter()
                .map(|track| {
                    TrackMetadata::from_spotify(track, requestor_name.clone()).unwrap_or_default()
                })
                .collect();

            return Ok(metadata);
        }

        // If the URL doesn't match any known Spotify pattern.
        Err(MusicError::AudioSourceError(
            "Invalid Spotify URL".to_string(),
        ))
    }
}
