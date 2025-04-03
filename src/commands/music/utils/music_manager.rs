use ::serenity::all::{CreateMessage, EditMessage, MessageId, User, UserId};
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::Mutex as SerenityMutex;
use songbird::input::YoutubeDl;
use songbird::tracks::{Track, TrackHandle, TrackQueue};
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
use super::embedded_messages::{self, PlayerMessageData};

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

    #[error("No queue")]
    NoQueue,
}

/// Result type for music operations
pub type MusicResult<T> = Result<T, MusicError>;

/// Manages Songbird instances and voice connections
pub struct MusicManager {
    // Map of guild ID to queue
    queues: HashMap<GuildId, TrackQueue>,
    // Store channel IDs for each guild
    channel_ids: HashMap<GuildId, ChannelId>,
    // Store message IDs for each guild
    message_ids: HashMap<GuildId, MessageId>,
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
    // Track whether a guild has manually stopped playback
    // manual_stop_flags: HashMap<GuildId, bool>,
    // Flags to indicate a "previous track" action is in progress
    // previous_action_flags: HashMap<GuildId, bool>,
}

pub static MUSIC_MANAGER: LazyLock<Arc<Mutex<MusicManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(MusicManager::default())));

impl Default for MusicManager {
    fn default() -> Self {
        Self {
            queues: Default::default(),
            history: Default::default(),
            show_queue: Default::default(),
            update_tasks: Default::default(),
            repeat_state: Default::default(),
            shuffle_enabled: Default::default(),
            channel_ids: Default::default(),
            message_ids: Default::default(),
            // manual_stop_flags: Default::default(),
            // previous_action_flags: Default::default(),
        }
    }
}

impl MusicManager {
    /// Get the Songbird voice client from the context
    pub async fn get_songbird(ctx: &Context) -> MusicResult<Arc<Songbird>> {
        songbird::get(ctx).await.ok_or(MusicError::NoVoiceManager)
    }

    /// Get the current voice channel call handle
    pub async fn get_call(
        ctx: &Context,
        guild_id: GuildId,
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        let songbird = Self::get_songbird(ctx).await?;
        songbird.get(guild_id).ok_or(MusicError::NotConnected)
    }

    // Get the current queue for this guild
    pub fn get_queue(&self, guild_id: &GuildId) -> Option<&TrackQueue> {
        self.queues.get(guild_id)
    }

    pub fn store_queue(&mut self, guild_id: GuildId, queue: TrackQueue) {
        self.queues.insert(guild_id, queue);
    }

    pub fn drop_queue(&mut self, guild_id: &GuildId) {
        self.queues.remove(guild_id);
    }

    // Get the active embedded music-player message for this guild
    pub fn get_message_id(&self, guild_id: GuildId) -> Option<MessageId> {
        self.message_ids.get(&guild_id).copied()
    }

    pub fn store_message_id(&mut self, guild_id: GuildId, message_id: MessageId) {
        self.message_ids.insert(guild_id, message_id);
    }

    pub fn drop_message_id(&mut self, guild_id: &GuildId) {
        self.message_ids.remove(guild_id);
    }

    pub fn get_channel_id(&self, guild_id: GuildId) -> Option<ChannelId> {
        self.channel_ids.get(&guild_id).copied()
    }

    pub fn store_channel_id(&mut self, guild_id: GuildId, message_id: ChannelId) {
        self.channel_ids.insert(guild_id, message_id);
    }

    pub fn drop_channel_id(&mut self, guild_id: &GuildId) {
        self.channel_ids.remove(guild_id);
    }

    pub fn drop_all(&mut self, guild_id: &GuildId) {
        self.drop_queue(guild_id);
        self.drop_message_id(guild_id);
        self.drop_channel_id(guild_id);
    }

    // Convenience method to get the currently-playing track for this guild via its queue
    pub fn get_current_track(&self, guild_id: &GuildId) -> Option<TrackHandle> {
        let queue = self.get_queue(guild_id)?;
        queue.current()
    }

    async fn send_and_store_new_message(
        &mut self,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
        reply: CreateReply,
    ) -> Result<MessageId, Error> {
        // send new message
        let create_message = CreateMessage::new()
            .embeds(reply.embeds)
            .components(reply.components.unwrap_or_default());

        let message = channel_id.send_message(http, create_message).await?;
        // store the new message id
        self.store_message_id(guild_id, message.id);

        Ok(message.id)
    }

    pub fn get_player_message_data(&self, guild_id: GuildId) -> PlayerMessageData {
        let queue = self.get_queue(&guild_id);
        let show_queue = self.is_queue_view_enabled(guild_id);
        let has_history = self.has_history(guild_id);

        PlayerMessageData {
            queue,
            show_queue,
            has_history,
        }
    }

    pub async fn send_or_update_message(
        &mut self,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Result<MessageId, Error> {
        let data = self.get_player_message_data(guild_id);

        // Create the message without holding the lock
        let reply = embedded_messages::music_player_message(data).await?;

        let message_id = match self.get_message_id(guild_id) {
            Some(message_id) => {
                debug!("Found existing message ID, attempting to update.");
                let message = EditMessage::new()
                    .embeds(reply.embeds.clone())
                    .components(reply.components.clone().unwrap_or_default());

                let result = channel_id
                    .edit_message(http.clone(), message_id, message)
                    .await;

                if result.is_err() {
                    debug!("Failed to update existing message, sending new one.");
                    self.send_and_store_new_message(http, guild_id, channel_id, reply)
                        .await?
                } else {
                    message_id
                }
            }
            None => {
                debug!("No existing message ID, sending new one.");
                self.send_and_store_new_message(http, guild_id, channel_id, reply)
                    .await?
            }
        };

        Ok(message_id)
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
    // pub fn get_repeat_state(&self, guild_id: GuildId) -> RepeatState {
    //     self.repeat_state
    //         .get(&guild_id)
    //         .cloned()
    //         .unwrap_or(RepeatState::Disabled) // Default to Disabled
    // }

    // /// Cycle the repeat state for a guild
    // pub fn cycle_repeat_state(&mut self, guild_id: GuildId) -> RepeatState {
    //     let current_state = self.get_repeat_state(guild_id);
    //     let next_state = match current_state {
    //         RepeatState::Disabled => RepeatState::RepeatAll,
    //         RepeatState::RepeatAll => RepeatState::RepeatOne,
    //         RepeatState::RepeatOne => RepeatState::Disabled,
    //     };
    //     self.repeat_state.insert(guild_id, next_state.clone());
    //     info!(
    //         "Cycled repeat state for guild {}: {:?}",
    //         guild_id, next_state
    //     );
    //     next_state
    // }

    // /// Check if shuffle is enabled for a guild
    // pub fn is_shuffle_enabled(&self, guild_id: GuildId) -> bool {
    //     *self.shuffle_enabled.get(&guild_id).unwrap_or(&false) // Default to false
    // }

    // /// Toggle the shuffle state for a guild
    // pub fn toggle_shuffle(&mut self, guild_id: GuildId) -> bool {
    //     let current_state = self.is_shuffle_enabled(guild_id);
    //     let next_state = !current_state;
    //     self.shuffle_enabled.insert(guild_id, next_state);
    //     info!(
    //         "Toggled shuffle state for guild {}: {}",
    //         guild_id, next_state
    //     );
    //     next_state
    // }

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

    /// Start the periodic update task for a guild (async)
    async fn start_update_task(
        ctx: &Context,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) {
        // Stop existing task in a separate scope so the lock is released
        {
            let mut music_manager = MUSIC_MANAGER.lock().await;
            music_manager.stop_update_task(guild_id).await;
        }

        let music_manager = Arc::clone(&MUSIC_MANAGER);
        let ctx = Arc::new(ctx.clone());

        info!("Starting update task for guild {}", guild_id);

        let task = tokio::spawn(async move {
            loop {
                debug!("Attempting to send/update message for guild {}", guild_id);

                // Create a new scope for the lock to ensure it's released after use
                let message_result = {
                    let mut manager = music_manager.lock().await;
                    manager
                        .send_or_update_message(http.clone(), guild_id, channel_id)
                        .await
                };

                match message_result {
                    Ok(_) => info!("Successfully updated player message for guild {}", guild_id),
                    Err(e) => {
                        warn!(
                            "Error updating music player message for guild {}: {}",
                            guild_id, e
                        );
                    }
                }

                // Check if should continue in a separate scope
                let should_continue = match Self::get_call(&ctx, guild_id).await {
                    Ok(call_handler) => !call_handler.lock().await.queue().is_empty(),
                    Err(_) => {
                        info!(
                            "Stopping update task for guild {} as no call is available.",
                            guild_id
                        );
                        false
                    }
                };

                debug!(
                    "Update task continue check for guild {}: {}",
                    guild_id, should_continue
                );

                if !should_continue {
                    info!(
                        "Stopping update task for guild {} as no track is playing.",
                        guild_id
                    );
                    break;
                }

                debug!("Update task sleeping for 5s for guild {}", guild_id);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            info!("Update task loop finished for guild {}", guild_id);
        });

        // Store the task handle in a separate scope
        {
            let mut music_manager = MUSIC_MANAGER.lock().await;
            music_manager.update_tasks.insert(guild_id, task);
        }
    }

    /// Stop the periodic update task for a guild (async)
    async fn stop_update_task(&mut self, guild_id: GuildId) {
        if let Some(task) = self.update_tasks.remove(&guild_id) {
            info!("Aborting update task for guild {}", guild_id); // Changed log message slightly
            task.abort();
        } else {
            info!("No active update task found to stop for guild {}", guild_id); // Added log for case where no task exists
        }
    }

    /// Join a voice channel
    pub async fn join_channel(
        ctx: &Context,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        let songbird = Self::get_songbird(ctx).await?;

        // Join the voice channel
        let handle = songbird
            .join(guild_id, channel_id)
            .await
            .map_err(|e| MusicError::JoinError(e.to_string()))?;

        Ok(handle)
    }

    /// Leave a voice channel
    pub async fn leave_channel(ctx: &Context, guild_id: GuildId) -> MusicResult<()> {
        let songbird = Self::get_songbird(ctx).await?;

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
        channel_id: ChannelId,
        user: &User,
        input: String,
    ) -> Result<(TrackMetadata, usize), MusicError> {
        // Get songbird manager
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird Voice client placed in scope at initialization.")
            .clone();

        Self::confirm_voice_connection(ctx, &manager, guild_id.clone(), user.id).await?;

        let inputs = Self::query_to_youtube_inputs(&input, user.name.clone()).await?;
        let number_of_tracks = inputs.len();
        let first_track = inputs[0].clone();

        if let Some(handler_lock) = manager.get(guild_id) {
            let mut handler = handler_lock.lock().await;

            for metadata in inputs.into_iter() {
                // metadata.set_requestor(user_id.clone());
                let input = YoutubeDl::new(HTTP_CLIENT.clone(), metadata.clone().url.unwrap());
                let mut track = Track::from(input);
                track.user_data = Arc::new(metadata);
                handler.enqueue(track).await;
            }

            let mut manager = MUSIC_MANAGER.lock().await;
            manager.store_queue(guild_id, handler.queue().clone());
        }

        Self::start_update_task(ctx, ctx.http.clone(), guild_id, channel_id).await;

        Ok((first_track, number_of_tracks))
    }

    async fn query_to_youtube_inputs(
        input: &String,
        requestor_name: String,
    ) -> Result<Vec<TrackMetadata>, MusicError> {
        match input {
            url if AudioSource::is_url(url) => {
                for api_handler in AUDIO_APIS.iter() {
                    if api_handler.is_valid_url(url) {
                        return api_handler.get_metadata(url, requestor_name).await;
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
        let channel_id = match Self::get_user_voice_channel(ctx, guild_id, user_id) {
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
}
