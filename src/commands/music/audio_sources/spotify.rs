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

pub type SpotifyResult<T> = Result<T, MusicError>;

/// Data structure for Spotify track information
#[derive(Clone, Debug)]
pub struct SpotifyTrack {
    pub name: String,
    pub artists: Vec<String>,
}

/// Authentication tokens for Spotify API
#[derive(Debug, Serialize, Deserialize)]
struct SpotifyToken {
    access_token: String,
    token_type: String,
    expires_in: u64,
    #[serde(skip, default = "Instant::now")]
    created_at: Instant,
}

impl SpotifyToken {
    fn is_expired(&self) -> bool {
        let expiry = Duration::from_secs(self.expires_in);
        let elapsed = self.created_at.elapsed();
        // Consider it expired 30 seconds before actual expiry
        elapsed > expiry.saturating_sub(Duration::from_secs(30))
    }
}

static SPOTIFY_TOKEN: LazyLock<Arc<Mutex<Option<SpotifyToken>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

static SPOTIFY_TRACK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/track/([a-zA-Z0-9]+)(\?.*)?$").unwrap()
});

static SPOTIFY_PLAYLIST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/playlist/([a-zA-Z0-9]+)(\?.*)?$")
        .unwrap()
});

static SPOTIFY_ALBUM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(open\.spotify\.com|spotify)/album/([a-zA-Z0-9]+)(\?.*)?$").unwrap()
});

/// Spotify API client
pub struct SpotifyApi;

impl Default for SpotifyApi {
    fn default() -> Self {
        Self {}
    }
}

impl SpotifyApi {
    /// Check if the given URL is a Spotify URL
    pub fn is_spotify_url(url: &str) -> bool {
        SPOTIFY_TRACK_REGEX.is_match(url)
            || SPOTIFY_PLAYLIST_REGEX.is_match(url)
            || SPOTIFY_ALBUM_REGEX.is_match(url)
    }

    /// Extract track ID from a Spotify track URL
    pub fn extract_track_id(url: &str) -> Option<String> {
        SPOTIFY_TRACK_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Extract playlist ID from a Spotify playlist URL
    pub fn extract_playlist_id(url: &str) -> Option<String> {
        SPOTIFY_PLAYLIST_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Extract album ID from a Spotify album URL
    pub fn extract_album_id(url: &str) -> Option<String> {
        SPOTIFY_ALBUM_REGEX
            .captures(url)
            .and_then(|cap| cap.get(3))
            .map(|m| m.as_str().to_string())
    }

    /// Get an access token for Spotify API
    async fn get_access_token() -> SpotifyResult<String> {
        let mut token_lock = SPOTIFY_TOKEN.lock().await;

        // Return existing token if it's still valid
        if let Some(token) = &*token_lock {
            if !token.is_expired() {
                return Ok(token.access_token.clone());
            }
        }

        // Get client ID and secret from environment variables
        let client_id = env::var("SPOTIFY_CLIENT_ID")
            .map_err(|_| MusicError::ConfigError("SPOTIFY_CLIENT_ID not set".to_string()))?;
        let client_secret = env::var("SPOTIFY_CLIENT_SECRET")
            .map_err(|_| MusicError::ConfigError("SPOTIFY_CLIENT_SECRET not set".to_string()))?;

        // Create authorization header (Basic auth with client_id:client_secret)
        let auth = BASE64_STANDARD.encode(format!("{}:{}", client_id, client_secret));
        let auth_header = format!("Basic {}", auth);

        // Request new token
        let params = [("grant_type", "client_credentials")];
        let response = HTTP_CLIENT
            .post("https://accounts.spotify.com/api/token")
            .header(header::AUTHORIZATION, auth_header)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify token: {}", e))
            })?;

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

        // Parse token response
        let token_response = response.json::<SpotifyToken>().await.map_err(|e| {
            MusicError::ExternalApiError(format!("Failed to parse Spotify token: {}", e))
        })?;

        let access_token = token_response.access_token.clone();
        *token_lock = Some(token_response);

        Ok(access_token)
    }

    /// Get track information from Spotify API
    pub async fn get_track(track_id: &str) -> SpotifyResult<SpotifyTrack> {
        let token = Self::get_access_token().await?;
        let url = format!("https://api.spotify.com/v1/tracks/{}", track_id);

        let response = HTTP_CLIENT
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify track: {}", e))
            })?;

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

        let track_data: serde_json::Value = response.json().await.map_err(|e| {
            MusicError::ExternalApiError(format!("Failed to parse Spotify track data: {}", e))
        })?;

        // Extract track data
        let name = track_data["name"]
            .as_str()
            .ok_or_else(|| MusicError::ExternalApiError("Missing track name".to_string()))?
            .to_string();

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

    /// Get tracks and name from a Spotify playlist
    pub async fn get_playlist_tracks(playlist_id: &str) -> SpotifyResult<Vec<SpotifyTrack>> {
        let token = Self::get_access_token().await?;

        let mut tracks = Vec::new();

        // Fetch the tracks
        let mut tracks_url = format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?limit=50",
            playlist_id
        );

        loop {
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

            let playlist_data: serde_json::Value = response.json().await.map_err(|e| {
                MusicError::ExternalApiError(format!(
                    "Failed to parse Spotify playlist data: {}",
                    e
                ))
            })?;

            // Process tracks in this page
            if let Some(items) = playlist_data["items"].as_array() {
                for item in items {
                    if let Some(track) = item["track"].as_object() {
                        if track.get("id").is_none() || track["id"].is_null() {
                            continue; // Skip local tracks that don't have Spotify IDs
                        }

                        let name = track["name"]
                            .as_str()
                            .ok_or_else(|| {
                                MusicError::ExternalApiError("Missing track name".to_string())
                            })?
                            .to_string();

                        let artists = track["artists"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        tracks.push(SpotifyTrack { name, artists });
                    }
                }
            }

            // Check if there are more pages
            if let Some(next_url) = playlist_data["next"].as_str() {
                tracks_url = next_url.to_string();
            } else {
                break;
            }
        }

        Ok(tracks)
    }

    /// Get tracks and name from a Spotify album
    pub async fn get_album_tracks(album_id: &str) -> SpotifyResult<Vec<SpotifyTrack>> {
        let token = Self::get_access_token().await?;
        let mut tracks = Vec::new();

        // First fetch album details to get album name and image
        let album_url = format!("https://api.spotify.com/v1/albums/{}", album_id);
        let album_response = HTTP_CLIENT
            .get(&album_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| {
                MusicError::ExternalApiError(format!("Failed to request Spotify album: {}", e))
            })?;

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

        // Now get tracks from album
        let mut url = format!(
            "https://api.spotify.com/v1/albums/{}/tracks?limit=50",
            album_id
        );

        loop {
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

            let tracks_data: serde_json::Value = response.json().await.map_err(|e| {
                MusicError::ExternalApiError(format!(
                    "Failed to parse Spotify album tracks data: {}",
                    e
                ))
            })?;

            // Process tracks in this page
            if let Some(items) = tracks_data["items"].as_array() {
                for track in items {
                    if track.get("id").is_none() || track["id"].is_null() {
                        continue; // Skip local tracks that don't have Spotify IDs
                    }

                    let name = track["name"]
                        .as_str()
                        .ok_or_else(|| {
                            MusicError::ExternalApiError("Missing track name".to_string())
                        })?
                        .to_string();

                    let artists = track["artists"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    tracks.push(SpotifyTrack { name, artists });
                }
            }

            // Check if there are more pages
            if let Some(next_url) = tracks_data["next"].as_str() {
                url = next_url.to_string();
            } else {
                break;
            }
        }

        Ok(tracks)
    }

    /// Get search query for YouTube from a Spotify track
    pub fn get_youtube_search_query(track: &SpotifyTrack) -> String {
        let artists_str = track.artists.join(", ");
        format!("{} by {} audio", track.name, artists_str)
    }
}

#[async_trait]
impl AudioApi for SpotifyApi {
    fn is_valid_url(&self, url: &str) -> bool {
        SpotifyApi::is_spotify_url(url)
    }

    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError> {
        info!("Creating audio source from Spotify URL: {}", url);

        // Determine the type of Spotify URL (track, playlist, album)
        if let Some(track_id) = SpotifyApi::extract_track_id(url) {
            // It's a track URL

            let track = SpotifyApi::get_track(&track_id).await?;
            match TrackMetadata::try_from(track) {
                Ok(metadata) => return Ok(vec![metadata]),
                Err(e) => return Err(e),
            }
        } else if let Some(playlist_id) = SpotifyApi::extract_playlist_id(url) {
            // It's a playlist URL

            let tracks = SpotifyApi::get_playlist_tracks(&playlist_id).await?;
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify playlist is empty".to_string(),
                ));
            }

            let metadata = tracks
                .into_iter()
                .map(|track| {
                    TrackMetadata::from_spotify(track, requestor_name.clone()).unwrap_or_default()
                })
                .collect();

            return Ok(metadata);
        } else if let Some(album_id) = SpotifyApi::extract_album_id(url) {
            // It's an album URL
            let tracks = SpotifyApi::get_album_tracks(&album_id).await?;
            if tracks.is_empty() {
                return Err(MusicError::AudioSourceError(
                    "Spotify album is empty".to_string(),
                ));
            }

            let metadata = tracks
                .into_iter()
                .map(|track| {
                    TrackMetadata::from_spotify(track, requestor_name.clone()).unwrap_or_default()
                })
                .collect();

            return Ok(metadata);
        }

        Err(MusicError::AudioSourceError(
            "Invalid Spotify URL".to_string(),
        ))
    }
}
