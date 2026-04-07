//! This module defines the structure and traits for handling different audio sources.
//! It includes implementations for specific sources like YouTube and Spotify (if enabled),
//! and provides a common interface (`AudioApi`) for fetching track metadata.

/// Submodule for finding related songs, potentially used by autoplay.
pub(crate) mod related_songs;
/// Submodule implementing the `AudioApi` trait for Spotify.
pub(crate) mod spotify;
/// Submodule defining the `TrackMetadata` struct used across audio sources.
pub(crate) mod track_metadata;
/// Submodule implementing the `AudioApi` trait for YouTube.
pub(crate) mod youtube;

use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use spotify::SpotifyApi;
use std::sync::LazyLock;
use track_metadata::TrackMetadata;
use url::Url;
use youtube::YoutubeApi;

/// A specialized `Result` type for operations within the `audio_sources` module.
pub type AudioSourceResult<T> = Result<T, MusicError>;

/// Lazily initialized static array holding instances of available `AudioApi` implementations.
/// This allows iterating over all supported APIs to find one that matches a given URL.
pub static AUDIO_APIS: LazyLock<[Box<dyn AudioApi>; 2]> =
    LazyLock::new(|| [Box::new(YoutubeApi), Box::new(SpotifyApi)]);

/// Trait defining the common interface for all audio source APIs (e.g., YouTube, Spotify).
/// Requires `Send + Sync` to be safely used across async tasks.
#[async_trait]
pub trait AudioApi: Send + Sync {
    /// Checks if the given URL string is valid and recognized by this specific audio API implementation.
    fn is_valid_url(&self, url: &str) -> bool;

    /// Asynchronously fetches metadata for one or more tracks from the given URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch metadata from (e.g., YouTube video/playlist, Spotify track/album/playlist).
    /// * `requestor_name` - The name of the user who requested the track(s).
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<TrackMetadata>` on success, or a `MusicError` on failure.
    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError>;
}

/// A utility struct providing general helper functions related to audio sources.
pub struct AudioSource;

impl AudioSource {
    /// Performs a basic check if the input string can be parsed as a URL,
    /// or if it matches any specifically supported format by an `AudioApi`
    /// (e.g., schemeless `spotify/track/...` links).
    pub fn is_url(input: &str) -> bool {
        Url::parse(input).is_ok() || AUDIO_APIS.iter().any(|api| api.is_valid_url(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_url() {
        // Valid generic URLs
        assert!(AudioSource::is_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(AudioSource::is_url("http://example.com"));
        assert!(AudioSource::is_url("ftp://ftp.is.co.za/rfc/rfc1808.txt"));

        // Valid API-specific links without standard URL schemes
        assert!(AudioSource::is_url("spotify/track/4uLU6hMCjMI75M1A2tKUQC"));
        assert!(AudioSource::is_url("open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M"));

        // Invalid URLs
        assert!(!AudioSource::is_url("just a string"));
        assert!(!AudioSource::is_url("www.google.com")); // Missing scheme and not recognized by any AudioApi
        assert!(!AudioSource::is_url(""));
    }
}
