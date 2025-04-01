use rand::thread_rng;
use ::serenity::all::{CreateMessage, EditMessage, MessageId, UserId};
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::Mutex as SerenityMutex;
use songbird::input::YoutubeDl;
use songbird::{Call, Songbird};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::Error;
use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::audio_sources::youtube::YoutubeApi;
use crate::commands::music::audio_sources::{AUDIO_APIS, AudioSource};

use super::button_controls::RepeatState;
use super::embedded_messages;

use crate::HTTP_CLIENT;
use tracing::{debug, error, info, warn};

/// Errors that can occur during music operations
#[derive(Error, Debug)]
pub enum MusicError {
    #[error("Not in a guild")]
    NotInGuild,

    #[error("Failed to join voice channel: {0}")]
    JoinError(String),

    #[error("Not connected to a voice channel")]
    NotConnected,

    #[error("Failed to get voice manager")]
    NoVoiceManager,

    #[error("User is not in a voice channel")]
    UserNotInVoiceChannel,

    #[error("Audio source error: {0}")]
    AudioSourceError(String),

    #[error("External API error: {0}")]
    ExternalApiError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Cache error: {0}")]
    CacheError(Box<dyn std::error::Error + Send + Sync>),
}

/// Result type for music operations
pub type MusicResult<T> = Result<T, MusicError>;

/// Manages Songbird instances and voice connections
pub struct MusicManager {
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
    // Store channel IDs for each guild
    channel_ids: HashMap<GuildId, ChannelId>,
    // Store message IDs for each guild
    message_ids: HashMap<GuildId, MessageId>,
    // Track whether a guild has manually stopped playback
    manual_stop_flags: HashMap<GuildId, bool>,
    // Flags to indicate a "previous track" action is in progress
    previous_action_flags: HashMap<GuildId, bool>,
}

pub static MUSIC_MANAGER: LazyLock<Arc<Mutex<MusicManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(MusicManager::default())));

impl Default for MusicManager {
    fn default() -> Self {
        Self {
            history: Default::default(),
            show_queue: Default::default(),
            update_tasks: Default::default(),
            repeat_state: Default::default(),
            shuffle_enabled: Default::default(),
            channel_ids: Default::default(),
            message_ids: Default::default(),
            manual_stop_flags: Default::default(),
            previous_action_flags: Default::default(),
        }
    }
}

impl MusicManager {
    pub async fn get_message_id(&self, guild_id: GuildId) -> Option<MessageId> {
        self.message_ids.get(&guild_id).copied()
    }

    pub async fn store_message_id(&mut self, guild_id: GuildId, message_id: MessageId) {
        self.message_ids.insert(guild_id, message_id);
    }

    async fn send_and_store_new_message(
        &mut self,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
        reply: CreateReply,
    ) -> Result<(), Error> {
        // send new message
        let create_message = CreateMessage::new()
            .embeds(reply.embeds)
            .components(reply.components.unwrap_or_default());

        let message = channel_id.send_message(http, create_message).await?;
        // store the new message id
        self.store_message_id(guild_id, message.id).await;

        Ok(())
    }

    pub async fn send_or_update_message(
        &mut self,
        ctx: &Context,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Result<(), Error> {
        let reply = embedded_messages::music_player_message(ctx, guild_id).await?;

        // let channel_id = match queue_manager::get_channel_id(guild_id).await {
        //     Some(channel_id) => channel_id,
        //     None => {
        //         return Err(Box::new(serenity::Error::Other("No channel id found")));
        //     }
        // };

        let message_id = self.get_message_id(guild_id).await;
        if let Some(message_id) = message_id {
            let message = EditMessage::new()
                .embeds(reply.embeds.clone())
                .components(reply.components.clone().unwrap_or_default());

            let result = channel_id
                .edit_message(http.clone(), message_id, message)
                .await;

            if result.is_err() {
                self.send_and_store_new_message(http, guild_id, channel_id, reply)
                    .await?;
            }
        } else {
            self.send_and_store_new_message(http, guild_id, channel_id, reply)
                .await?;
        }

        Ok(())
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

    // /// Shuffle the current queue for a guild
    // pub fn shuffle_queue(&mut self, guild_id: GuildId) {
    //     if let Some(queue) = self.queues.get_mut(&guild_id) {
    //         if queue.len() > 1 {
    //             let mut rng = thread_rng();
    //             // VecDeque doesn't directly support shuffle, so convert to Vec and back
    //             // Or use make_contiguous if efficiency is critical and possible
    //             let contiguous_slice = queue.make_contiguous(); // Remove mut
    //             contiguous_slice.shuffle(&mut rng);
    //             info!("Shuffled queue for guild {}", guild_id);
    //         }
    //     }
    // }

    // /// Start the periodic update task for a guild (async)
    // pub async fn start_update_task(&mut self, http: Arc<serenity::Http>, guild_id: GuildId) {
    //     // Stop existing task if any
    //     self.stop_update_task(guild_id).await;

    //     info!("Starting update task for guild {}", guild_id);
    //     // tokio::spawn handles pinning the future
    //     let task = tokio::spawn(async move {
    //         loop {
    //             debug!("Attempting to send/update message for guild {}", guild_id); // Added debug log
    //             match self.send_or_update_message(http.clone(), guild_id).await {
    //                 Ok(_) => info!("Successfully updated player message for guild {}", guild_id),
    //                 Err(e) => {
    //                     warn!(
    //                         "Error updating music player message for guild {}: {}",
    //                         guild_id, e
    //                     );
    //                     // Consider stopping the task if updates consistently fail
    //                     // For now, just log and continue
    //                 }
    //             }
    //             // Check if the task should stop (e.g., if the bot left the channel or stopped playing)
    //             // This check could be more sophisticated, e.g., using a channel or atomic flag
    //             let should_continue = {
    //                 let manager = QUEUE_MANAGER.lock().await;
    //                 manager.current_tracks.contains_key(&guild_id)
    //             };
    //             debug!(
    //                 "Update task continue check for guild {}: {}",
    //                 guild_id, should_continue
    //             ); // Added debug log

    //             if !should_continue {
    //                 info!(
    //                     "Stopping update task for guild {} as no track is playing.",
    //                     guild_id
    //                 );
    //                 break;
    //             }

    //             debug!("Update task sleeping for 5s for guild {}", guild_id); // Added debug log
    //             tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    //         }
    //         info!("Update task loop finished for guild {}", guild_id);
    //     });
    //     self.update_tasks.insert(guild_id, task);
    // }

    // /// Stop the periodic update task for a guild (async)
    // pub async fn stop_update_task(&mut self, guild_id: GuildId) {
    //     if let Some(task) = self.update_tasks.remove(&guild_id) {
    //         info!("Aborting update task for guild {}", guild_id); // Changed log message slightly
    //         task.abort();
    //         // Optionally, await the task handle to ensure it's fully stopped, though abort() is usually sufficient
    //         // if let Err(e) = task.await {
    //         //     if !e.is_cancelled() {
    //         //         error!("Update task for guild {} panicked: {:?}", guild_id, e);
    //         //     }
    //         // }
    //     } else {
    //         info!("No active update task found to stop for guild {}", guild_id); // Added log for case where no task exists
    //     }
    // }
}

/// Get the Songbird voice client from the context
pub async fn get_songbird(ctx: &Context) -> MusicResult<Arc<Songbird>> {
    let songbird = songbird::get(ctx).await.ok_or(MusicError::NoVoiceManager)?;

    Ok(songbird)
}

/// Get the current voice channel call handle
pub async fn get_call(ctx: &Context, guild_id: GuildId) -> MusicResult<Arc<SerenityMutex<Call>>> {
    let songbird = get_songbird(ctx).await?;

    let call = songbird.get(guild_id).ok_or(MusicError::NotConnected)?;

    Ok(call)
}

/// Join a voice channel
pub async fn join_channel(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> MusicResult<Arc<SerenityMutex<Call>>> {
    let songbird = get_songbird(ctx).await?;

    // Join the voice channel
    let handle = songbird
        .join(guild_id, channel_id)
        .await
        .map_err(|e| MusicError::JoinError(e.to_string()))?;

    Ok(handle)
}

/// Leave a voice channel
pub async fn leave_channel(ctx: &Context, guild_id: GuildId) -> MusicResult<()> {
    let songbird = get_songbird(ctx).await?;

    // Check if we're in a voice channel
    if songbird.get(guild_id).is_none() {
        return Err(MusicError::NotConnected);
    }

    // Leave the voice channel
    songbird
        .remove(guild_id)
        .await
        .map_err(|_| MusicError::JoinError("Failed to leave voice channel".to_string()))?;

    Ok(())
}

/// Get the voice channel ID that the user is currently in
pub fn get_user_voice_channel(
    ctx: &Context,
    guild_id: GuildId,
    user_id: serenity::UserId,
) -> MusicResult<ChannelId> {
    // Get the guild
    let guild = ctx.cache.guild(guild_id).ok_or(MusicError::NotInGuild)?;

    // Get the voice state of the user
    let voice_state = guild
        .voice_states
        .get(&user_id)
        .ok_or(MusicError::UserNotInVoiceChannel)?;

    // Get the channel ID
    let channel_id = voice_state
        .channel_id
        .ok_or(MusicError::UserNotInVoiceChannel)?;

    Ok(channel_id)
}

/// Processes the request to play or queue a track/playlist.
/// Handles joining voice, fetching metadata, caching, queueing, and starting playback if needed.
pub async fn process_play_request(
    ctx: &Context,
    guild_id: GuildId,
    user_id: UserId,
    input: String,
) -> Result<(TrackMetadata, usize), MusicError> {
    // Get songbird manager
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in scope at initialization.")
        .clone();

    // let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    confirm_voice_connection(ctx, &manager, guild_id.clone(), user_id.clone()).await?;

    let inputs = query_to_youtube_inputs(&input).await?;
    let number_of_tracks = inputs.len();
    let first_track = inputs[0].clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        // for input in inputs.into_iter() {
        for metadata in inputs.into_iter() {
            let input = YoutubeDl::new(HTTP_CLIENT.clone(), metadata.url.unwrap());
            handler.enqueue_input(input.into()).await;
        }
    }

    Ok((first_track, number_of_tracks))
}

async fn query_to_youtube_inputs(input: &String) -> Result<Vec<TrackMetadata>, MusicError> {
    match input {
        url if AudioSource::is_url(url) => {
            for api_handler in AUDIO_APIS.iter() {
                if api_handler.is_valid_url(url) {
                    return api_handler.get_metadata(url).await;
                }
            }
            Err(MusicError::AudioSourceError(format!(
                "Unable to resolve URL to valid provider: {}",
                url
            )))
        }
        query => {
            let metadata = YoutubeApi::from_search(query)?;
            Ok(vec![metadata])
        }
    }
}

async fn confirm_voice_connection(
    ctx: &Context,
    manager: &Songbird,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<(), MusicError> {
    // Get the user's voice channel ID
    let channel_id = match get_user_voice_channel(ctx, guild_id, user_id) {
        Ok(id) => id,
        Err(_) => return Err(MusicError::UserNotInVoiceChannel),
    };

    // Join the voice channel if not already connected, or get the existing call
    if manager.get(guild_id).is_none() {
        // Not connected, attempt to join
        if let Err(err) = manager.join(guild_id, channel_id).await {
            error!(
                "Failed to join voice channel {} for guild {}: {}",
                channel_id, guild_id, err
            );
            return Err(MusicError::JoinError(err.to_string()));
        }
    }

    Ok(())
}
