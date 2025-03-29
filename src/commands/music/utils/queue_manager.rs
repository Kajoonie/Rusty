use super::{audio_sources::TrackMetadata, button_controls::RepeatState}; // Import RepeatState
use super::music_manager::MusicError;
use crate::commands::music::utils::music_manager;
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom; // Import shuffle
use rand::thread_rng; // Import thread_rng
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::MessageId;
use songbird::tracks::TrackHandle;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Result type for queue operations
pub type QueueResult<T> = Result<T, MusicError>;

/// Manages the queue of tracks for each guild
pub struct QueueManager {
    // Map of guild ID to queue of track metadata
    queues: HashMap<GuildId, VecDeque<TrackMetadata>>,
    // Map of guild ID to current track handle and its metadata
    current_tracks: HashMap<GuildId, (TrackHandle, TrackMetadata)>,
    // Map of guild ID to track history metadata (most recent first)
    history: HashMap<GuildId, VecDeque<TrackMetadata>>,
    // Map of guild ID to queue view toggle state
    show_queue: HashMap<GuildId, bool>,
    // Map of guild ID to periodic update task handle
    update_tasks: HashMap<GuildId, JoinHandle<()>>,
    // Map of guild ID to repeat state
    repeat_state: HashMap<GuildId, RepeatState>,
    // Map of guild ID to shuffle state
    shuffle_enabled: HashMap<GuildId, bool>,
}

impl QueueManager {
    /// Create a new queue manager
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            current_tracks: HashMap::new(),
            history: HashMap::new(),
            show_queue: HashMap::new(),
            update_tasks: HashMap::new(),
            repeat_state: HashMap::new(),
            shuffle_enabled: HashMap::new(),
        }
    }

    /// Add track metadata to the queue for a guild
    pub fn add(&mut self, guild_id: GuildId, metadata: TrackMetadata) {
        // Changed parameter name and type
        // Get or create the queue for this guild
        let queue = self.queues.entry(guild_id).or_default();
        queue.push_back(metadata); // Use metadata
    }

    /// Get the next track's metadata from the queue for a guild
    pub fn next(&mut self, guild_id: GuildId) -> Option<TrackMetadata> {
        // Changed return type
        // Get the queue for this guild
        // The current track will be overwritten by set_current_track when the new track starts
        if let Some(queue) = self.queues.get_mut(&guild_id) {
            queue.pop_front() // This now returns TrackMetadata
        } else {
            None
        }
    }

    /// Clear the queue for a guild
    pub async fn clear(&mut self, guild_id: GuildId) {
        // Remove the queue
        self.queues.remove(&guild_id);
        // Remove the current track
        self.current_tracks.remove(&guild_id);
        // Remove the history
        self.history.remove(&guild_id);
        // Reset queue view state
        self.show_queue.remove(&guild_id);
        // Reset repeat state
        self.repeat_state.remove(&guild_id);
        // Reset shuffle state
        self.shuffle_enabled.remove(&guild_id);
        // Stop update task if running
        self.stop_update_task(guild_id).await;
    }

    /// Get the current queue metadata for a guild
    pub fn get_queue(&self, guild_id: GuildId) -> Vec<&TrackMetadata> {
        if let Some(queue) = self.queues.get(&guild_id) {
            // The queue now directly stores TrackMetadata
            queue.iter().collect()
        } else {
            Vec::new()
        }
    }

    /// Set the current track for a guild, moving the previous current track's metadata (if any) to history
    pub fn set_current_track(
        &mut self,
        guild_id: GuildId,
        track_handle: TrackHandle,
        metadata: TrackMetadata, // Changed parameter name
    ) {
        // If there was a previous track playing, add its metadata to the history
        if let Some((_old_handle, old_metadata)) = self.current_tracks.remove(&guild_id) {
            let history_queue = self.history.entry(guild_id).or_default();
            // Keep history size manageable (e.g., max 50)
            if history_queue.len() >= 50 {
                history_queue.pop_back(); // Remove the oldest item
            }
            history_queue.push_front(old_metadata); // Add the just-finished track's metadata
            debug!(
                "Added track '{}' to history for guild {}",
                history_queue.front().unwrap().title, // Access title directly
                guild_id
            );
        }

        // Insert the new track's handle and metadata as the current one
        self.current_tracks
            .insert(guild_id, (track_handle, metadata));
    }

    /// Get the current track handle and its metadata for a guild
    pub fn get_current_track(&self, guild_id: GuildId) -> Option<&(TrackHandle, TrackMetadata)> {
        self.current_tracks.get(&guild_id)                                                                                  
    }                                                                                                                       
                                                                                                                            
    /// Get the previous track's metadata from history. Does NOT modify the main queue or current track state.              
    /// Returns the TrackMetadata of the track retrieved from history.                                                      
    pub fn previous(&mut self, guild_id: GuildId) -> Option<TrackMetadata> {                                                
        // Get the history queue, return None if no history                                                                 
        let history_queue = self.history.get_mut(&guild_id)?;                                                               
        // Pop the most recent metadata from history                                                                        
        let previous_metadata = history_queue.pop_front()?;                                                                 
        debug!(                                                                                                             
            "Retrieved track '{}' from history for guild {} (state not modified yet)",                                      
            previous_metadata.title, guild_id                                                                               
        );                                                                                                                  
                                                                                                                            
        // Return the metadata retrieved from history                                                                       
        Some(previous_metadata)
    }

    /// Check if there is any track history for the guild
    pub fn has_history(&self, guild_id: GuildId) -> bool {
        self.history.get(&guild_id).is_some_and(|h| !h.is_empty())
    }

    /// Toggle the queue view state for a guild (async)
    pub fn toggle_queue_view(&mut self, guild_id: GuildId) {
        let current_state = self.show_queue.entry(guild_id).or_insert(true);
        *current_state = !*current_state;
        info!(
            "Toggled queue view for guild {}: {}",
            guild_id, *current_state
        );
    }

    /// Check if the queue view is enabled for a guild (async)
    pub fn is_queue_view_enabled(&self, guild_id: GuildId) -> bool {
        *self.show_queue.get(&guild_id).unwrap_or(&true) // Default to true (show queue)
    }

    /// Get the current repeat state for a guild
    pub fn get_repeat_state(&self, guild_id: GuildId) -> RepeatState {
        self.repeat_state
            .get(&guild_id)
            .cloned()
            .unwrap_or(RepeatState::Disabled) // Default to Disabled
    }

    /// Cycle the repeat state for a guild
    pub fn cycle_repeat_state(&mut self, guild_id: GuildId) -> RepeatState {
        let current_state = self.get_repeat_state(guild_id);
        let next_state = match current_state {
            RepeatState::Disabled => RepeatState::RepeatAll,
            RepeatState::RepeatAll => RepeatState::RepeatOne,
            RepeatState::RepeatOne => RepeatState::Disabled,
        };
        self.repeat_state.insert(guild_id, next_state.clone());
        info!(
            "Cycled repeat state for guild {}: {:?}",
            guild_id, next_state
        );
        next_state
    }

    /// Check if shuffle is enabled for a guild
    pub fn is_shuffle_enabled(&self, guild_id: GuildId) -> bool {
        *self.shuffle_enabled.get(&guild_id).unwrap_or(&false) // Default to false
    }

    /// Toggle the shuffle state for a guild
    pub fn toggle_shuffle(&mut self, guild_id: GuildId) -> bool {
        let current_state = self.is_shuffle_enabled(guild_id);
        let next_state = !current_state;
        self.shuffle_enabled.insert(guild_id, next_state);
        info!(
            "Toggled shuffle state for guild {}: {}",
            guild_id, next_state
        );
        next_state
    }

    /// Shuffle the current queue for a guild
    pub fn shuffle_queue(&mut self, guild_id: GuildId) {
        if let Some(queue) = self.queues.get_mut(&guild_id) {
            if queue.len() > 1 {
                let mut rng = thread_rng();
                // VecDeque doesn't directly support shuffle, so convert to Vec and back
                // Or use make_contiguous if efficiency is critical and possible
                let mut contiguous_slice = queue.make_contiguous();
                contiguous_slice.shuffle(&mut rng);
                info!("Shuffled queue for guild {}", guild_id);
            }
        }
    }

    /// Start the periodic update task for a guild (async)
    pub async fn start_update_task(&mut self, ctx: Arc<serenity::Context>, guild_id: GuildId) {
        // Stop existing task if any
        self.stop_update_task(guild_id).await;

        info!("Starting update task for guild {}", guild_id);
        // tokio::spawn handles pinning the future
        let task = tokio::spawn(async move {
            loop {
                let ctx_clone = ctx.clone();
                debug!("Attempting to send/update message for guild {}", guild_id); // Added debug log
                match music_manager::send_or_update_message(&ctx_clone, guild_id).await {
                    Ok(_) => info!("Successfully updated player message for guild {}", guild_id),
                    Err(e) => {
                        warn!(
                            "Error updating music player message for guild {}: {}",
                            guild_id, e
                        );
                        // Consider stopping the task if updates consistently fail
                        // For now, just log and continue
                    }
                }
                // Check if the task should stop (e.g., if the bot left the channel or stopped playing)
                // This check could be more sophisticated, e.g., using a channel or atomic flag
                let should_continue = {
                    let manager = QUEUE_MANAGER.lock().await;
                    manager.current_tracks.contains_key(&guild_id)
                };
                debug!(
                    "Update task continue check for guild {}: {}",
                    guild_id, should_continue
                ); // Added debug log

                if !should_continue {
                    info!(
                        "Stopping update task for guild {} as no track is playing.",
                        guild_id
                    );
                    break;
                }

                debug!("Update task sleeping for 5s for guild {}", guild_id); // Added debug log
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            info!("Update task loop finished for guild {}", guild_id);
        });
        self.update_tasks.insert(guild_id, task);
    }

    /// Stop the periodic update task for a guild (async)
    pub async fn stop_update_task(&mut self, guild_id: GuildId) {
        if let Some(task) = self.update_tasks.remove(&guild_id) {
            info!("Aborting update task for guild {}", guild_id); // Changed log message slightly
            task.abort();
            // Optionally, await the task handle to ensure it's fully stopped, though abort() is usually sufficient
            // if let Err(e) = task.await {
            //     if !e.is_cancelled() {
            //         error!("Update task for guild {} panicked: {:?}", guild_id, e);
            //     }
            // }
        } else {
            info!("No active update task found to stop for guild {}", guild_id); // Added log for case where no task exists
        }
    }

    /// Remove a track's metadata at a specific position in the queue (0-based index)
    /// Returns the removed track's metadata if successful
    pub fn remove_track(&mut self, guild_id: GuildId, position: usize) -> Option<TrackMetadata> {
        if let Some(queue) = self.queues.get_mut(&guild_id) {
            if position < queue.len() {
                // Remove and return the metadata at the specified position
                queue.remove(position) // VecDeque::remove returns Option<T> directly
            } else {
                None
            }
        } else {
            None
        }
    }
}

// Create a global queue manager wrapped in a mutex for thread safety
pub static QUEUE_MANAGER: LazyLock<Arc<Mutex<QueueManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(QueueManager::new())));

// Track whether a guild has been manually stopped
static MANUAL_STOP_FLAGS: LazyLock<Mutex<HashMap<GuildId, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// Store channel IDs for each guild
static CHANNEL_IDS: LazyLock<Mutex<HashMap<GuildId, ChannelId>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// Store message IDs for each guild
static MESSAGE_IDS: LazyLock<Mutex<HashMap<GuildId, MessageId>>> =                                                          
    LazyLock::new(|| Mutex::new(HashMap::new()));                                                                           
                                                                                                                            
// Flag to indicate a "previous track" action is in progress                                                                
static PREVIOUS_ACTION_FLAGS: LazyLock<Mutex<HashMap<GuildId, bool>>> =                                                     
    LazyLock::new(|| Mutex::new(HashMap::new()));                                                                           
                                                                                                                            
/// Helper functions for working with the global queue manager                                                              
pub async fn add_to_queue(guild_id: GuildId, metadata: TrackMetadata) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.add(guild_id, metadata);
    Ok(())
}

pub async fn get_next_track(guild_id: GuildId) -> QueueResult<Option<TrackMetadata>> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.next(guild_id))
}

pub async fn clear_queue(guild_id: GuildId) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.clear(guild_id).await;
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
    track_handle: TrackHandle,
    metadata: TrackMetadata, // Changed parameter name
) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.set_current_track(guild_id, track_handle, metadata);
    Ok(())
}

/// Gets a clone of the current track handle and its metadata
pub async fn get_current_track(
    guild_id: GuildId,
) -> QueueResult<Option<(TrackHandle, TrackMetadata)>> {
    let manager = QUEUE_MANAGER.lock().await;
    // Cloning TrackHandle is cheap (Arc), TrackMetadata is Clone.
    Ok(manager.get_current_track(guild_id).cloned()) // This now works                                                      
}                                                                                                                           
                                                                                                                            
/// Gets the previous track's metadata from history. Does NOT modify the main queue or current track state.                 
pub async fn get_previous_track(guild_id: GuildId) -> QueueResult<Option<TrackMetadata>> {                                  
    let mut manager = QUEUE_MANAGER.lock().await;                                                                           
    Ok(manager.previous(guild_id)) // Calls the modified method                                                             
}                                                                                                                           
                                                                                                                            
/// Checks if track history exists for the guild.
pub async fn has_history(guild_id: GuildId) -> bool {
    let manager = QUEUE_MANAGER.lock().await;
    manager.has_history(guild_id)
}

pub async fn remove_track(
    guild_id: GuildId,
    position: usize,
) -> QueueResult<Option<TrackMetadata>> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.remove_track(guild_id, position))
}

pub async fn is_queue_view_enabled(guild_id: GuildId) -> bool {
    let manager = QUEUE_MANAGER.lock().await;
    manager.is_queue_view_enabled(guild_id)
}

/// Toggle the queue view state for a guild
pub async fn toggle_queue_view(guild_id: GuildId) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.toggle_queue_view(guild_id);
    Ok(())
}

/// Get the current repeat state for a guild
pub async fn get_repeat_state(guild_id: GuildId) -> QueueResult<RepeatState> {
    let manager = QUEUE_MANAGER.lock().await;
    Ok(manager.get_repeat_state(guild_id))
}

/// Cycle the repeat state for a guild
pub async fn cycle_repeat_state(guild_id: GuildId) -> QueueResult<RepeatState> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.cycle_repeat_state(guild_id))
}

/// Check if shuffle is enabled for a guild
pub async fn is_shuffle_enabled(guild_id: GuildId) -> QueueResult<bool> {
    let manager = QUEUE_MANAGER.lock().await;
    Ok(manager.is_shuffle_enabled(guild_id))
}

/// Toggle the shuffle state for a guild
pub async fn toggle_shuffle(guild_id: GuildId) -> QueueResult<bool> {
    let mut manager = QUEUE_MANAGER.lock().await;
    Ok(manager.toggle_shuffle(guild_id))
}

/// Shuffle the current queue for a guild
pub async fn shuffle_queue(guild_id: GuildId) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.shuffle_queue(guild_id);
    Ok(())
}

/// Start the periodic update task for a guild
pub async fn start_update_task(ctx: Arc<serenity::Context>, guild_id: GuildId) -> QueueResult<()> {
    let mut manager = QUEUE_MANAGER.lock().await;
    manager.start_update_task(ctx, guild_id).await;
    Ok(())
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

/// Store the channel ID for a guild
pub async fn store_channel_id(guild_id: GuildId, channel_id: ChannelId) {
    let mut channels = CHANNEL_IDS.lock().await;
    channels.insert(guild_id, channel_id);
}

/// Get the channel ID for a guild
pub async fn get_channel_id(guild_id: GuildId) -> Option<ChannelId> {
    let channels = CHANNEL_IDS.lock().await;
    channels.get(&guild_id).copied()
}

/// Store the message ID for a guild
pub async fn store_message_id(guild_id: GuildId, message_id: MessageId) {
    let mut messages = MESSAGE_IDS.lock().await;
    messages.insert(guild_id, message_id);
}

/// Get the message ID for a guild
pub async fn get_message_id(guild_id: GuildId) -> Option<MessageId> {
    let messages = MESSAGE_IDS.lock().await;
    messages.get(&guild_id).copied()                                                                                        
}                                                                                                                           
                                                                                                                            
/// Set the previous action flag for a guild                                                                                
pub async fn set_previous_action_flag(guild_id: GuildId, value: bool) {                                                     
    let mut flags = PREVIOUS_ACTION_FLAGS.lock().await;                                                                     
    flags.insert(guild_id, value);                                                                                          
}                                                                                                                           
                                                                                                                            
/// Check if previous action flag is set                                                                                    
pub async fn is_previous_action_flag_set(guild_id: GuildId) -> bool {                                                       
    let flags = PREVIOUS_ACTION_FLAGS.lock().await;                                                                         
    *flags.get(&guild_id).unwrap_or(&false)                                                                                 
}                                                                                                                           
                                                                                                                            
/// Clear the previous action flag for a guild                                                                              
pub async fn clear_previous_action_flag(guild_id: GuildId) {                                                                
    let mut flags = PREVIOUS_ACTION_FLAGS.lock().await;                                                                     
    flags.remove(&guild_id);                                                                                                
}                                                                                                                           
                                                                                                                            
// Callback now only needs metadata, as Input is created later                                                              
pub type MetadataCallback = Box<dyn Fn(TrackMetadata) + Send + Sync>;

pub async fn get_queue_callback(guild_id: GuildId) -> MetadataCallback {
    Box::new(move |metadata| {
        // Changed signature: only metadata
        tokio::spawn(async move {
            // Add track metadata to queue
            // Directly use the metadata passed to the callback
            if let Err(err) = add_to_queue(guild_id, metadata.clone()).await {
                // Pass metadata directly
                error!("Failed to add track metadata to queue: {}", err); // Updated error message
                return;
            }

            info!("Added track metadata to queue: {}", metadata.title); // Updated info message
        });
    })
}
