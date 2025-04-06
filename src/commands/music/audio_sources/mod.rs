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
    /// Performs a basic check if the input string can be parsed as a URL.
    /// Does not validate if the URL is actually reachable or supported by any specific API.
    pub fn is_url(input: &str) -> bool {
        Url::parse(input).is_ok()
    }
}
