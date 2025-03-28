use ::serenity::all::{CreateMessage, EditMessage};
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::Mutex as SerenityMutex;
use songbird::{Call, Songbird};
use std::sync::Arc;
use thiserror::Error;

use crate::Error;

use super::{embedded_messages, queue_manager};

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
}

/// Result type for music operations
pub type MusicResult<T> = Result<T, MusicError>;

/// Manages Songbird instances and voice connections
pub struct MusicManager;

impl MusicManager {
    /// Get the Songbird voice client from the context
    pub async fn get_songbird(ctx: &Context) -> MusicResult<Arc<Songbird>> {
        let songbird = songbird::get(ctx).await.ok_or(MusicError::NoVoiceManager)?;

        Ok(songbird)
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

    /// Get the current voice channel call handle
    pub async fn get_call(
        ctx: &Context,
        guild_id: GuildId,
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        let songbird = Self::get_songbird(ctx).await?;

        let call = songbird.get(guild_id).ok_or(MusicError::NotConnected)?;

        Ok(call)
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
}

pub async fn send_or_update_message(
    ctx: &Context,
    guild_id: GuildId,
    // metadata: &TrackMetadata,
) -> Result<(), Error> {
    let reply = embedded_messages::music_player_message(guild_id).await?;

    let channel_id = match queue_manager::get_channel_id(guild_id).await {
        Some(channel_id) => channel_id,
        None => {
            return Err(Box::new(serenity::Error::Other("No channel id found")));
        }
    };

    let message_id = queue_manager::get_message_id(guild_id).await;
    if let Some(message_id) = message_id {
        let message = EditMessage::new()
            .embeds(reply.embeds.clone())
            .components(reply.components.clone().unwrap());
        let result = channel_id.edit_message(ctx, message_id, message).await;

        if result.is_err() {
            send_and_store_new_message(ctx, guild_id, channel_id, reply).await?;
        }
    } else {
        send_and_store_new_message(ctx, guild_id, channel_id, reply).await?;
    }

    Ok(())
}

async fn send_and_store_new_message(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    reply: CreateReply,
) -> Result<(), Error> {
    // send new message
    let create_message = CreateMessage::new()
        .embeds(reply.embeds)
        .components(reply.components.unwrap());
    let message = channel_id.send_message(ctx, create_message).await?;
    // store the new message id
    queue_manager::store_message_id(guild_id, message.id).await;

    Ok(())
}
