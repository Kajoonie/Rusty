use crate::{commands::music::utils::audio_sources::TrackMetadata, HTTP_CLIENT}; // Correct import path
use dashmap::DashMap;
use std::sync::LazyLock; // Use LazyLock instead of once_cell::sync::Lazy
use songbird::input::{Input, YoutubeDl};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tracing::{debug, error, info, warn};
use url::Url;

const CACHE_DIR: &str = ".track_cache";
const CACHE_INDEX_FILE: &str = "metadata_index.json";

// In-memory cache mapping YouTube URL -> TrackMetadata
static CACHE_INDEX: LazyLock<DashMap<String, TrackMetadata>> = LazyLock::new(|| { // Use LazyLock
    match load_cache_from_disk() {
        Ok(index) => index,
        Err(e) => {
            warn!("Failed to load track cache from disk: {}. Starting with empty cache.", e);
            DashMap::new()
        }
    }
});

// Mutex to prevent concurrent writes to the cache file
static CACHE_SAVE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(())); // Use LazyLock

fn get_cache_file_path() -> PathBuf {
    PathBuf::from(CACHE_DIR).join(CACHE_INDEX_FILE)
}

/// Loads the cache index from the JSON file on disk.
fn load_cache_from_disk() -> Result<DashMap<String, TrackMetadata>, Box<dyn std::error::Error>> {
    let cache_file_path = get_cache_file_path();
    if !cache_file_path.exists() {
        info!("Cache index file not found. Creating new cache.");
        return Ok(DashMap::new());
    }

    let file_content = fs::read_to_string(&cache_file_path)?;
    if file_content.trim().is_empty() {
        info!("Cache index file is empty. Starting with new cache.");
        return Ok(DashMap::new());
    }

    let deserialized_map: HashMap<String, TrackMetadata> = serde_json::from_str(&file_content)?;
    info!("Successfully loaded {} items from cache index.", deserialized_map.len());
    Ok(deserialized_map.into_iter().collect())
}

/// Saves the current in-memory cache index to the JSON file on disk.
fn save_cache_to_disk() -> Result<(), Box<dyn std::error::Error>> {
    // Acquire lock to prevent concurrent writes
    let _lock = CACHE_SAVE_LOCK.lock().map_err(|e| e.to_string())?;

    let cache_dir = Path::new(CACHE_DIR);
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir)?;
        info!("Created cache directory: {}", CACHE_DIR);
    }

    let cache_file_path = get_cache_file_path();
    let cache_map: HashMap<String, TrackMetadata> = CACHE_INDEX.clone().into_iter().collect();

    let serialized_data = serde_json::to_string_pretty(&cache_map)?;
    fs::write(&cache_file_path, serialized_data)?;
    debug!("Successfully saved cache index to disk at {:?}.", cache_file_path);
    Ok(())
}

/// Attempts to retrieve cached metadata for a given YouTube URL.
pub fn get_cached_metadata(url: &str) -> Option<TrackMetadata> {
    // Accessing the LazyLock ensures it's initialized if needed.
    if let Some(entry) = CACHE_INDEX.get(url) {
        debug!("Cache hit for URL: {}", url);
        Some(entry.value().clone())
    } else {
        debug!("Cache miss for URL: {}", url);
        None
    }
}

/// Caches the metadata for a given YouTube URL.
pub fn cache_metadata(url: &str, metadata: TrackMetadata) {
    // Accessing the LazyLock ensures it's initialized if needed before modifying.
    // Only cache if metadata seems valid (has a title and URL)
    if metadata.title.is_empty() || metadata.url.is_none() {
        warn!("Attempted to cache invalid metadata for URL: {}", url);
        return;
    }

    info!("Caching metadata for URL: {}", url);
    CACHE_INDEX.insert(url.to_string(), metadata);

    // Save the updated cache to disk asynchronously
    // Use spawn_blocking for synchronous file I/O
    tokio::task::spawn_blocking(|| {
        if let Err(e) = save_cache_to_disk() {
            error!("Failed to save track cache to disk: {}", e);
        }
    });
}

/// Creates a Songbird Input source from a YouTube URL.
/// Assumes the URL is valid and points to a playable YouTube video.
pub async fn create_input_from_url(url: &str) -> Result<Input, Box<dyn std::error::Error + Send + Sync>> {
    // Create the source using YoutubeDl
    let source = YoutubeDl::new(HTTP_CLIENT.clone(), url.to_string());
    // Optionally configure ytdl parameters if needed
    // source.youtube_dl_args = vec!["--format=bestaudio".to_string()];

    // Metadata is fetched and cached separately in the play command logic.
    // We only need to create the Input source here. Pre-initialization is not needed
    // and the aux_metadata call is removed.

    Ok(source.into())
}

/// Checks if a given string is a valid YouTube video URL.
pub fn is_youtube_url(query: &str) -> bool {
    match Url::parse(query) {
        Ok(url) => {
            url.host_str().is_some_and(|host| {
                host == "www.youtube.com" || host == "youtube.com" || host == "youtu.be"
            }) && url.path().starts_with("/watch") || url.host_str() == Some("youtu.be")
            // Basic check, might need refinement for shorts, playlists etc. if needed
        }
        Err(_) => false,
    }
}
