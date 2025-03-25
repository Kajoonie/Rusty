use lazy_static::lazy_static;
use serenity::model::id::GuildId;
use songbird::input::Input;
use songbird::tracks::TrackHandle;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::audio_sources::TrackMetadata;
use super::music_manager::MusicError;

/// A queue item containing the audio input and metadata
pub struct QueueItem {
    pub input: Input,
    pub metadata: TrackMetadata,
}

/// Result type for queue operations
pub type QueueResult<T> = Result<T, MusicError>;

/// Manages the queue of tracks for each guild
pub struct QueueManager {
    // Map of guild ID to queue
    queues: HashMap<GuildId, VecDeque<QueueItem>>,
    // Map of guild ID to current track handle
    current_tracks: HashMap<GuildId, (TrackHandle, TrackMetadata)>,
}

impl QueueManager {
    /// Create a new queue manager
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            current_tracks: HashMap::new(),
        }
    }

    /// Add a track to the queue for a guild
    pub fn add(&mut self, guild_id: GuildId, item: QueueItem) {
        // Get or create the queue for this guild
        let queue = self.queues.entry(guild_id).or_default();
        queue.push_back(item);
    }

    /// Get the next track in the queue for a guild
    pub fn next(&mut self, guild_id: GuildId) -> Option<QueueItem> {
        // Remove the current track handle if it exists
        self.current_tracks.remove(&guild_id);

        // Get the queue for this guild
        if let Some(queue) = self.queues.get_mut(&guild_id) {
            queue.pop_front()
        } else {
            None
        }
    }

    /// Clear the queue for a guild
    pub fn clear(&mut self, guild_id: GuildId) {
        // Remove the queue
        self.queues.remove(&guild_id);
        // Remove the current track
        self.current_tracks.remove(&guild_id);
    }

    /// Get the current queue for a guild
    pub fn get_queue(&self, guild_id: GuildId) -> Vec<&TrackMetadata> {
        if let Some(queue) = self.queues.get(&guild_id) {
            queue.iter().map(|item| &item.metadata).collect()
        } else {
            Vec::new()
        }
    }

    /// Set the current track for a guild
    pub fn set_current_track(
        &mut self,
        guild_id: GuildId,
        track: TrackHandle,
        metadata: TrackMetadata,
    ) {
        self.current_tracks.insert(guild_id, (track, metadata));
    }

    /// Get the current track for a guild
    pub fn get_current_track(&self, guild_id: GuildId) -> Option<&(TrackHandle, TrackMetadata)> {
        self.current_tracks.get(&guild_id)
    }

    /// Get the number of tracks in the queue for a guild
    pub fn len(&self, guild_id: GuildId) -> usize {
        match self.queues.get(&guild_id) {
            Some(queue) => queue.len(),
            None => 0,
        }
    }

    /// Remove a track at a specific position in the queue (0-based index)
    /// Returns the removed track's metadata if successful
    pub fn remove_track(&mut self, guild_id: GuildId, position: usize) -> Option<TrackMetadata> {
        if let Some(queue) = self.queues.get_mut(&guild_id) {
            if position < queue.len() {
                // Remove and return the track at the specified position
                let item = queue.remove(position)?;
                Some(item.metadata)
            } else {
                None
            }
        } else {
            None
        }
    }
}

// Create a global queue manager wrapped in a mutex for thread safety
lazy_static! {
    pub static ref QUEUE_MANAGER: Arc<Mutex<QueueManager>> = Arc::new(Mutex::new(QueueManager::new()));
    // Track whether a guild has been manually stopped
    static ref MANUAL_STOP_FLAGS: Mutex<HashMap<GuildId, bool>> = Mutex::new(HashMap::new());
}

/// Helper functions for working with the global queue manager
pub async fn add_to_queue(guild_id: GuildId, item: QueueItem) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.add(guild_id, item);
    Ok(())
}

pub async fn get_next_track(guild_id: GuildId) -> QueueResult<Option<QueueItem>> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.next(guild_id))
}

pub async fn clear_queue(guild_id: GuildId) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.clear(guild_id);
    Ok(())
}

pub async fn get_queue(guild_id: GuildId) -> QueueResult<Vec<TrackMetadata>> {
    let manager = QUEUE_MANAGER.lock().await;
    let queue = manager.get_queue(guild_id);
    // Clone the metadata to avoid lifetime issues
    let queue = queue.iter().map(|&metadata| metadata.clone()).collect();
    Ok(queue)
}

pub async fn set_current_track(
    guild_id: GuildId,
    track: TrackHandle,
    metadata: TrackMetadata,
) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.set_current_track(guild_id, track, metadata);
    Ok(())
}

pub async fn get_current_track(
    guild_id: GuildId,
) -> QueueResult<Option<(TrackHandle, TrackMetadata)>> {
    let manager = QUEUE_MANAGER.lock().await;
    Ok(manager.get_current_track(guild_id).cloned())
}

pub async fn queue_length(guild_id: GuildId) -> QueueResult<usize> {
    let manager = QUEUE_MANAGER.lock().await;
    Ok(manager.len(guild_id))
}

pub async fn remove_track(
    guild_id: GuildId,
    position: usize,
) -> QueueResult<Option<TrackMetadata>> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.remove_track(guild_id, position))
}

/// Set the manual stop flag for a guild
pub async fn set_manual_stop_flag(guild_id: GuildId, value: bool) {
    let mut flags = MANUAL_STOP_FLAGS.lock().await;
    flags.insert(guild_id, value);
}

/// Check if manual stop flag is set
pub async fn is_manual_stop_flag_set(guild_id: GuildId) -> bool {
    let flags = MANUAL_STOP_FLAGS.lock().await;
    *flags.get(&guild_id).unwrap_or(&false)
}

/// Clear the manual stop flag for a guild
pub async fn clear_manual_stop_flag(guild_id: GuildId) {
    let mut flags = MANUAL_STOP_FLAGS.lock().await;
    flags.remove(&guild_id);
}
