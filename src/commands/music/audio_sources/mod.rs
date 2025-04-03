pub(crate) mod related_songs;
pub(crate) mod spotify;
pub(crate) mod track_metadata;
pub(crate) mod youtube;

use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use spotify::SpotifyApi;
use std::sync::LazyLock;
use track_metadata::TrackMetadata;
use url::Url;
use youtube::YoutubeApi;

/// Result type for audio source operations
pub type AudioSourceResult<T> = Result<T, MusicError>;

pub static AUDIO_APIS: LazyLock<[Box<dyn AudioApi>; 2]> =
    LazyLock::new(|| [Box::new(YoutubeApi), Box::new(SpotifyApi)]);

#[async_trait]
pub trait AudioApi: Send + Sync {
    fn is_valid_url(&self, url: &str) -> bool;
    async fn get_metadata(
        &self,
        url: &str,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError>;
}

/// Audio source utilities for handling different types of audio inputs
pub struct AudioSource;

impl AudioSource {
    /// Check if a string is a valid URL
    pub fn is_url(input: &str) -> bool {
        Url::parse(input).is_ok()
    }
}
