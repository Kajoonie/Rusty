//! The core manager for music playback functionality.
//! Handles voice connections, queues, track metadata, state management (repeat, queue view),
//! message updates, and interaction with the Songbird library.
//! Provides a globally accessible, thread-safe instance.

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

/// Represents errors specific to music operations.
#[derive(Error, Debug)]
pub enum MusicError {
    /// Operation requires being in a guild context.
    #[error("Not in a guild")]
    NotInGuild,

    /// Failed to join the voice channel.
    #[error("Failed to join voice channel: {0}")]
    JoinError(String),

    /// Bot is not currently connected to a voice channel in the guild.
    #[error("Not connected to a voice channel")]
    NotConnected,

    /// Failed to retrieve the Songbird voice manager.
    #[error("Failed to get voice manager")]
    NoVoiceManager,

    /// The user invoking the command is not in a voice channel.
    #[error("User is not in a voice channel")]
    UserNotInVoiceChannel,

    /// Error related to fetching or processing audio source data.
    #[error("Audio source error: {0}")]
    AudioSourceError(String),

    /// Error communicating with an external API (e.g., Spotify, YouTube).
    #[error("External API error: {0}")]
    ExternalApiError(String),

    /// Missing or invalid configuration (e.g., API keys).
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Expected a queue to exist, but none was found for the guild.
    #[error("No queue")]
    NoQueue,
}

/// A specialized `Result` type for music operations.
pub type MusicResult<T> = Result<T, MusicError>;

/// Central struct managing the state of music playback across guilds.
#[derive(Default)]
pub struct MusicManager {
    /// Maps GuildId to the corresponding Songbird `TrackQueue`.
    queues: HashMap<GuildId, TrackQueue>,
    /// Maps GuildId to the `ChannelId` where the player message is located.
    channel_ids: HashMap<GuildId, ChannelId>,
    /// Maps GuildId to the `MessageId` of the active player message.
    message_ids: HashMap<GuildId, MessageId>,
    /// Maps GuildId to whether the detailed queue view is shown (true) or hidden (false).
    show_queue: HashMap<GuildId, bool>,
    /// Maps GuildId to the handle of the task responsible for periodically updating the player message.
    update_tasks: HashMap<GuildId, JoinHandle<()>>,
    /// Maps GuildId to the current repeat state (`Disabled` or `Track`).
    repeat_state: HashMap<GuildId, RepeatState>,
}


/// Global, thread-safe instance of the `MusicManager`.
/// Lazily initialized and wrapped in `Arc<Mutex>` for safe concurrent access.
static MUSIC_MANAGER: LazyLock<Arc<Mutex<MusicManager>>> =
    LazyLock::new(|| Arc::new(Mutex::new(MusicManager::default())));

/// Provides read-only access to the global `MusicManager` within a closure.
pub async fn with_manager<F, R>(f: F) -> R
where
    F: FnOnce(&MusicManager) -> R,
{
    // Lock the mutex to get access.
    let manager = MUSIC_MANAGER.lock().await;
    // Execute the closure with the manager reference.
    f(&manager)
}

/// Provides mutable access to the global `MusicManager` within a closure.
pub async fn with_manager_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut MusicManager) -> R,
{
    // Lock the mutex to get mutable access.
    let mut manager = MUSIC_MANAGER.lock().await;
    // Execute the closure with the mutable manager reference.
    f(&mut manager)
}

impl MusicManager {
    /// Retrieves the global Songbird instance from the Serenity context's type map.
    pub async fn get_songbird(ctx: &Context) -> MusicResult<Arc<Songbird>> {
        songbird::get(ctx).await.ok_or(MusicError::NoVoiceManager)
    }

    /// Retrieves the Songbird `Call` handler for a specific guild, if one exists.
    /// Returns `MusicError::NotConnected` if the bot is not in a voice channel in that guild.
    pub async fn get_call(
        ctx: &Context,
        guild_id: GuildId,
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        // Get the global Songbird instance.
        let songbird = Self::get_songbird(ctx).await?;
        // Get the call handler for the guild.
        songbird.get(guild_id).ok_or(MusicError::NotConnected)
    }

    /// Retrieves a clone of the `TrackQueue` for a given guild, if it exists in the manager.
    pub async fn get_queue(guild_id: &GuildId) -> Option<TrackQueue> {
        with_manager(|m| m.queues.get(guild_id).cloned()).await
    }

    /// Stores or updates the `TrackQueue` for a given guild in the manager.
    pub async fn store_queue(guild_id: GuildId, queue: TrackQueue) {
        with_manager_mut(|m| m.queues.insert(guild_id, queue)).await;
    }

    /// Retrieves the `MessageId` of the player message for a given guild, if stored.
    pub async fn get_message_id(guild_id: GuildId) -> Option<MessageId> {
        with_manager(|m| m.message_ids.get(&guild_id).copied()).await
    }

    /// Stores or updates the `MessageId` of the player message for a given guild.
    pub async fn store_message_id(guild_id: GuildId, message_id: MessageId) {
        with_manager_mut(|m| m.message_ids.insert(guild_id, message_id)).await;
    }

    /// Retrieves the `ChannelId` where the player message is located for a given guild, if stored.
    pub async fn get_channel_id(guild_id: GuildId) -> Option<ChannelId> {
        with_manager(|m| m.channel_ids.get(&guild_id).copied()).await
    }

    /// Stores or updates the `ChannelId` of the player message for a given guild.
    pub async fn store_channel_id(guild_id: GuildId, channel_id: ChannelId) {
        with_manager_mut(|m| m.channel_ids.insert(guild_id, channel_id)).await;
    }

    /// Stores the `JoinHandle` for the player message update task for a given guild.
    pub async fn store_update_task(guild_id: GuildId, task: JoinHandle<()>) {
        with_manager_mut(|m| m.update_tasks.insert(guild_id, task)).await;
    }

    /// Removes and returns the `JoinHandle` for the update task for a given guild, if it exists.
    pub async fn drop_update_task(guild_id: &GuildId) -> Option<JoinHandle<()>> {
        with_manager_mut(|m| m.update_tasks.remove(guild_id)).await
    }

    /// Retrieves the current `RepeatState` for a given guild, defaulting to `Disabled` if not set.
    pub async fn get_repeat_state(guild_id: GuildId) -> RepeatState {
        with_manager(|m| {
            m.repeat_state
                .get(&guild_id)
                .cloned()
                .unwrap_or(RepeatState::Disabled)
        })
        .await
    }

    /// Sets the `RepeatState` for a given guild.
    pub async fn set_repeat_state(guild_id: GuildId, state: RepeatState) {
        with_manager_mut(|m| m.repeat_state.insert(guild_id, state)).await;
    }

    /// Removes all stored state (queue, message ID, channel ID, repeat state) for a given guild.
    /// Typically called when the bot leaves a voice channel or stops playback.
    pub async fn drop_all(guild_id: &GuildId) {
        with_manager_mut(|m| {
            m.queues.remove(guild_id);
            m.message_ids.remove(guild_id);
            m.channel_ids.remove(guild_id);
            m.repeat_state.remove(guild_id);
        })
        .await;
    }

    /// Convenience method to get the `TrackHandle` of the currently playing track for a guild.
    /// Returns `None` if no queue exists or if the queue is empty/stopped.
    pub async fn get_current_track(guild_id: &GuildId) -> Option<TrackHandle> {
        // Get the queue.
        let queue = Self::get_queue(guild_id).await?;
        // Get the current track from the queue.
        queue.current()
    }

    /// Sends a new player message and stores its ID and channel ID.
    async fn send_and_store_new_message(
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
        reply: CreateReply,
    ) -> Result<MessageId, Error> {
        // Build the message content.
        let create_message = CreateMessage::new()
            .embeds(reply.embeds)
            .components(reply.components.unwrap_or_default());

        // Send the message.
        let message = channel_id.send_message(http, create_message).await?;

        // Store IDs in the manager.
        Self::store_channel_id(guild_id, channel_id).await;
        Self::store_message_id(guild_id, message.id).await;

        Ok(message.id)
    }

    /// Fetches the necessary data required to build the player message embed.
    pub async fn get_player_message_data(guild_id: &GuildId) -> PlayerMessageData {
        // Access the manager to get queue, show_queue, and repeat_state.
        let (queue, show_queue, repeat_state) = with_manager(|m| {
            (
                m.queues.get(guild_id).cloned(),
                m.show_queue.get(guild_id).copied().unwrap_or(true),
                m.repeat_state
                    .get(guild_id)
                    .copied()
                    .unwrap_or(RepeatState::Disabled),
            )
        })
        .await;

        // Construct the data struct.
        PlayerMessageData {
            queue,
            show_queue,
            repeat_state,
        }
    }

    /// Sends a new player message or updates an existing one.
    ///
    /// Checks if a message ID is stored for the guild. If yes, attempts to edit it.
    /// If editing fails (e.g., message deleted) or no ID is stored, sends a new message
    /// and stores the new ID.
    pub async fn send_or_update_message(
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Result<MessageId, Error> {
        // Flip the boolean value.
        // Generate the message content (embeds + components).
        let reply = embedded_messages::music_player_message(guild_id).await?;

        // Check if an existing message ID is stored.
        let message_id = match Self::get_message_id(guild_id).await {
            Some(message_id) => {
                // Attempt to edit the existing message.
                debug!("Found existing message ID, attempting to update.");
                let message = EditMessage::new()
                    .embeds(reply.embeds.clone())
                    .components(reply.components.clone().unwrap_or_default());

                let result = channel_id
                    .edit_message(http.clone(), message_id, message)
                    .await;

                // If editing failed, send a new message instead.
                if result.is_err() {
                    debug!("Failed to update existing message, sending new one.");
                    Self::send_and_store_new_message(http, guild_id, channel_id, reply).await?
                } else {
                    // Return the existing (successfully edited) message ID.
                    message_id
                }
            }
            None => {
                // No existing message ID, send a new one.
                debug!("No existing message ID, sending new one.");
                Self::send_and_store_new_message(http, guild_id, channel_id, reply).await?
            }
        };

        Ok(message_id)
    }

    /// Toggles the `show_queue` state for a given guild.
    pub async fn toggle_queue_view(guild_id: GuildId) {
        with_manager_mut(|m| {
        // Get mutable access to the show_queue map, insert default (true) if not present.
            let current_state = m.show_queue.entry(guild_id).or_insert(true);
            // Flip the boolean value.
            *current_state = !*current_state;
            info!(
                "Toggled queue view for guild {}: {}",
                guild_id, *current_state
            );
        })
        .await;
    }

    /// Shuffles the track queue for a given guild, keeping the currently playing track (if any) at the front.
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

    /// Spawns and manages a background task that periodically updates the player message.
    /// Stops any existing update task for the guild before starting a new one.
    /// The task runs every 5 seconds and stops automatically if the queue becomes empty
    /// or the bot leaves the voice channel.
    async fn start_update_task(
        ctx: &Context,
        http: Arc<serenity::Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) {
        Self::stop_update_task(guild_id).await;

        // Clone context for use in the async task.
        let ctx = Arc::new(ctx.clone());

        info!("Starting update task for guild {}", guild_id);

        // Spawn the asynchronous task.
        let task = tokio::spawn(async move {
            // Loop indefinitely until a break condition is met.
            loop {
                debug!("Attempting to send/update message for guild {}", guild_id);

                // Create a new scope for the lock to ensure it's released after use
                let message_result =
                    Self::send_or_update_message(http.clone(), guild_id, channel_id).await;

                // Log success or warning on failure.
                match message_result {
                    Ok(_) => info!("Successfully updated player message for guild {}", guild_id),
                    Err(e) => {
                        warn!(
                            "Error updating music player message for guild {}: {}",
                            guild_id, e
                        );
                    }
                }

                // Check if the task should continue running.
                let should_continue = match Self::get_call(&ctx, guild_id).await {
                    // Continue if a call handler exists and its queue is not empty.
                    Ok(call_handler) => !call_handler.lock().await.queue().is_empty(),
                    // Stop if no call handler exists (bot left channel).
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

                // If the loop should not continue, break out.
                if !should_continue {
                    info!(
                        "Stopping update task for guild {} as no track is playing.",
                        guild_id
                    );
                    break;
                }

                debug!("Update task sleeping for 5s for guild {}", guild_id);
                // Wait before the next update attempt.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            info!("Update task loop finished for guild {}", guild_id);
        });

        // Store the handle of the newly spawned task.
        Self::store_update_task(guild_id, task).await;
    }

    /// Stops the background player message update task for a guild, if it's running.
    async fn stop_update_task(guild_id: GuildId) {
        // Remove the task handle from the manager.
        if let Some(task) = Self::drop_update_task(&guild_id).await {
            info!("Aborting update task for guild {}", guild_id); // Changed log message slightly
            task.abort();
        } else {
            info!("No active update task found to stop for guild {}", guild_id); // Added log for case where no task exists
        }
    }

    /// Makes the bot leave the voice channel for a given guild.
    pub async fn leave_channel(ctx: &Context, guild_id: GuildId) -> MusicResult<()> {
        // Get the Songbird instance.
        let songbird = Self::get_songbird(ctx).await?;

        // Check if the bot is actually connected in this guild.
        if songbird.get(guild_id).is_none() {
            return Err(MusicError::NotConnected);
        }

        // Attempt to remove the call handler (leave the channel).
        songbird
            .remove(guild_id)
            .await
            .map_err(|_| MusicError::JoinError("Failed to leave voice channel".to_string()))?;

        Ok(())
    }

    /// Finds the voice channel ID that a given user is currently in within a specific guild.
    /// Uses the Serenity cache.
    /// Gets the `ChannelId` of the voice channel a user is currently in.
    /// Returns `UserNotInVoiceChannel` if the user is not in a voice channel in the specified guild.
    pub fn get_user_voice_channel(
        ctx: &Context,
        guild_id: GuildId,
        user_id: serenity::UserId,
    ) -> MusicResult<ChannelId> {
        // Get guild data from cache.
        let guild = ctx.cache.guild(guild_id).ok_or(MusicError::NotInGuild)?;

        // Get the user's voice state from the guild data.
        let voice_state = guild
            .voice_states
            .get(&user_id)
            .ok_or(MusicError::UserNotInVoiceChannel)?;

        // Extract the channel ID from the voice state.
        let channel_id = voice_state
            .channel_id
            .ok_or(MusicError::UserNotInVoiceChannel)?;

        Ok(channel_id)
    }

    /// High-level function to process a `/play` request.
    ///
    /// 1. Ensures the bot is joined to the user's voice channel.
    /// 2. Converts the user's input (URL or search query) into `TrackMetadata`.
    /// 3. Adds the fetched track(s) to the guild's queue.
    /// 4. Starts the player message update task.
    /// 5. Returns metadata of the first added track and the total number added.
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

        Self::try_join_voice(ctx, &manager, guild_id, user.id).await?;

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

    /// If it's a URL, it iterates through `AUDIO_APIS` to find a handler.
    /// If it's not a URL, it performs a YouTube search.
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

    /// Gets the user's current voice channel and joins it using Songbird.
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

#[cfg(test)]
mod tests {
    use super::*;
    

    // Helper to create a dummy GuildId for testing
    fn test_guild_id() -> GuildId {
        GuildId::new(1)
    }

    #[tokio::test]
    async fn test_toggle_queue_view() {
        let guild_id = test_guild_id();

        // Initial state should default to true (or whatever the default is, let's assume true)
        let initial_state =
            with_manager(|m| m.show_queue.get(&guild_id).copied().unwrap_or(true)).await;
        assert!(initial_state, "Initial state should be true");

        // Toggle 1: true -> false
        MusicManager::toggle_queue_view(guild_id).await;
        let state_after_toggle1 =
            with_manager(|m| m.show_queue.get(&guild_id).copied().unwrap()).await;
        assert!(
            !state_after_toggle1,
            "State after first toggle should be false"
        );

        // Toggle 2: false -> true
        MusicManager::toggle_queue_view(guild_id).await;
        let state_after_toggle2 =
            with_manager(|m| m.show_queue.get(&guild_id).copied().unwrap()).await;
        assert!(
            state_after_toggle2,
            "State after second toggle should be true"
        );

        // Clean up state for other tests if necessary (though LazyLock persists)
        with_manager_mut(|m| m.show_queue.remove(&guild_id)).await;
    }

    #[tokio::test]
    async fn test_repeat_state() {
        let guild_id = test_guild_id();

        // Initial state should be Disabled
        let initial_state = MusicManager::get_repeat_state(guild_id).await;
        assert_eq!(initial_state, RepeatState::Disabled);

        // Set to Track
        MusicManager::set_repeat_state(guild_id, RepeatState::Track).await;
        let state_track = MusicManager::get_repeat_state(guild_id).await;
        assert_eq!(state_track, RepeatState::Track);

        // Set back to Disabled
        MusicManager::set_repeat_state(guild_id, RepeatState::Disabled).await;
        let state_disabled = MusicManager::get_repeat_state(guild_id).await;
        assert_eq!(state_disabled, RepeatState::Disabled);

        // Clean up
        with_manager_mut(|m| m.repeat_state.remove(&guild_id)).await;
    }
}
