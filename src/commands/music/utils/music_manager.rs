use ::serenity::all::{CreateMessage, EditMessage, MessageId, User, UserId};
use poise::{CreateReply, serenity_prelude as serenity};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::Mutex as SerenityMutex;
use songbird::input::YoutubeDl;
use songbird::tracks::{Track, TrackHandle, TrackQueue};
use songbird::{Call, Songbird};
use std::collections::HashMap;
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
    // Map of guild ID to queue view toggle state
    show_queue: HashMap<GuildId, bool>,
    // Map of guild ID to periodic update task handle
    update_tasks: HashMap<GuildId, JoinHandle<()>>,
    // Map of guild ID to repeat state
    repeat_state: HashMap<GuildId, RepeatState>,
}

impl Default for MusicManager {
    fn default() -> Self {
        Self {
            queues: Default::default(),
            channel_ids: Default::default(),
            message_ids: Default::default(),
            show_queue: Default::default(),
            update_tasks: Default::default(),
            repeat_state: Default::default(),
        }
    }
}

static MUSIC_MANAGER: LazyLock<Arc<Mutex<MusicManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(MusicManager::default())));

pub async fn with_manager<F, R>(f: F) -> R
where
    F: FnOnce(&MusicManager) -> R,
{
    let manager = MUSIC_MANAGER.lock().await;
    f(&manager)
}

pub async fn with_manager_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut MusicManager) -> R,
{
    let mut manager = MUSIC_MANAGER.lock().await;
    f(&mut manager)
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
    pub async fn get_queue(guild_id: &GuildId) -> Option<TrackQueue> {
        with_manager(|m| m.queues.get(guild_id).cloned()).await
    }

    pub async fn store_queue(guild_id: GuildId, queue: TrackQueue) {
        with_manager_mut(|m| m.queues.insert(guild_id, queue)).await;
    }

    // Get the active embedded music-player message for this guild
    pub async fn get_message_id(guild_id: GuildId) -> Option<MessageId> {
        with_manager(|m| m.message_ids.get(&guild_id).copied()).await
    }

    pub async fn store_message_id(guild_id: GuildId, message_id: MessageId) {
        with_manager_mut(|m| m.message_ids.insert(guild_id, message_id)).await;
    }

    pub async fn get_channel_id(guild_id: GuildId) -> Option<ChannelId> {
        with_manager(|m| m.channel_ids.get(&guild_id).copied()).await
    }

    pub async fn store_channel_id(guild_id: GuildId, channel_id: ChannelId) {
        with_manager_mut(|m| m.channel_ids.insert(guild_id, channel_id)).await;
    }

    pub async fn store_update_task(guild_id: GuildId, task: JoinHandle<()>) {
        with_manager_mut(|m| m.update_tasks.insert(guild_id, task)).await;
    }

    pub async fn drop_update_task(guild_id: &GuildId) -> Option<JoinHandle<()>> {
        with_manager_mut(|m| m.update_tasks.remove(guild_id)).await
    }

    pub async fn get_repeat_state(guild_id: GuildId) -> RepeatState {
        with_manager(|m| {
            m.repeat_state
                .get(&guild_id)
                .cloned()
                .unwrap_or(RepeatState::Disabled)
        })
        .await
    }

    pub async fn set_repeat_state(guild_id: GuildId, state: RepeatState) {
        with_manager_mut(|m| m.repeat_state.insert(guild_id, state)).await;
    }

    pub async fn drop_all(guild_id: &GuildId) {
        with_manager_mut(|m| {
            m.queues.remove(guild_id);
            m.message_ids.remove(guild_id);
            m.channel_ids.remove(guild_id);
            m.repeat_state.remove(guild_id);
        })
        .await;
    }

    // Convenience method to get the currently-playing track for this guild via its queue
    pub async fn get_current_track(guild_id: &GuildId) -> Option<TrackHandle> {
        let queue = Self::get_queue(guild_id).await?;
        queue.current()
    }

    async fn send_and_store_new_message(
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

        // store the channel and message id
        Self::store_channel_id(guild_id, channel_id).await;
        Self::store_message_id(guild_id, message.id).await;

        Ok(message.id)
    }

    pub async fn get_player_message_data(guild_id: &GuildId) -> PlayerMessageData {
        let (queue, show_queue, repeat_state) = with_manager(|m| {
            (
                m.queues.get(&guild_id).cloned(),
                m.show_queue.get(&guild_id).copied().unwrap_or(true),
                m.repeat_state
                    .get(&guild_id)
                    .copied()
                    .unwrap_or(RepeatState::Disabled),
            )
        })
        .await;

        PlayerMessageData {
            queue,
            show_queue,
            repeat_state,
        }
    }

    pub async fn send_or_update_message(
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Result<MessageId, Error> {
        let reply = embedded_messages::music_player_message(guild_id).await?;

        let message_id = match Self::get_message_id(guild_id).await {
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
                    Self::send_and_store_new_message(http, guild_id, channel_id, reply).await?
                } else {
                    message_id
                }
            }
            None => {
                debug!("No existing message ID, sending new one.");
                Self::send_and_store_new_message(http, guild_id, channel_id, reply).await?
            }
        };

        Ok(message_id)
    }

    /// Toggle the queue view state for a guild (async)
    pub async fn toggle_queue_view(guild_id: GuildId) {
        with_manager_mut(|m| {
            let current_state = m.show_queue.entry(guild_id).or_insert(true);
            *current_state = !*current_state;
            info!(
                "Toggled queue view for guild {}: {}",
                guild_id, *current_state
            );
        })
        .await;
    }

    pub async fn shuffle_queue(guild_id: &GuildId) {
        with_manager_mut(|m| {
            if let Some(queue) = m.queues.get(guild_id) {
                if queue.len() <= 1 {
                    return;
                }

                let mut rng = thread_rng();

                queue.modify_queue(|q| {
                    // Keep the first track (currently playing)
                    let current = q.pop_front();

                    // Convert remaining queue to Vec for shuffling
                    let mut remaining: Vec<_> = q.drain(..).collect();
                    remaining.shuffle(&mut rng);

                    // Put back the current track
                    if let Some(current_track) = current {
                        q.push_back(current_track);
                    }

                    // Add shuffled tracks back
                    q.extend(remaining);
                });
            }
        })
        .await;
    }

    /// Start the periodic update task for a guild (async)
    async fn start_update_task(
        ctx: &Context,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) {
        Self::stop_update_task(guild_id).await;

        let ctx = Arc::new(ctx.clone());

        info!("Starting update task for guild {}", guild_id);

        let task = tokio::spawn(async move {
            loop {
                debug!("Attempting to send/update message for guild {}", guild_id);

                // Create a new scope for the lock to ensure it's released after use
                let message_result =
                    Self::send_or_update_message(http.clone(), guild_id, channel_id).await;

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

        Self::store_update_task(guild_id, task).await;
    }

    /// Stop the periodic update task for a guild (async)
    async fn stop_update_task(guild_id: GuildId) {
        if let Some(task) = Self::drop_update_task(&guild_id).await {
            info!("Aborting update task for guild {}", guild_id); // Changed log message slightly
            task.abort();
        } else {
            info!("No active update task found to stop for guild {}", guild_id); // Added log for case where no task exists
        }
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

        Self::try_join_voice(ctx, &manager, guild_id.clone(), user.id).await?;

        let inputs = Self::query_to_youtube_inputs(&input, user.name.clone()).await?;
        let number_of_tracks = inputs.len();
        let first_track = inputs[0].clone();

        if let Some(handler_lock) = manager.get(guild_id) {
            for metadata in inputs.into_iter() {
                Self::add_to_queue(handler_lock.clone(), metadata).await;
            }
            let handler = handler_lock.lock().await;
            Self::store_queue(guild_id, handler.queue().clone()).await;
        }

        Self::start_update_task(ctx, ctx.http.clone(), guild_id, channel_id).await;

        Ok((first_track, number_of_tracks))
    }

    pub fn play_success_response(metadata: TrackMetadata, number_of_tracks: usize) -> CreateReply {
        let reply_content = if number_of_tracks > 1 {
            format!("✅ Added playlist: with {} tracks", number_of_tracks)
        } else {
            format!("✅ Added to queue: {}", metadata.title)
        };

        embedded_messages::generic_success("Music", &reply_content)
    }

    pub async fn add_to_queue(call: Arc<Mutex<Call>>, metadata: TrackMetadata) {
        let input = YoutubeDl::new(HTTP_CLIENT.clone(), metadata.clone().url.unwrap());
        let mut track = Track::from(input);
        track.user_data = Arc::new(metadata);
        call.lock().await.enqueue(track).await;
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

    async fn try_join_voice(
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
