use poise::serenity_prelude as serenity;
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId};
use songbird::{Call, Songbird};
use std::sync::Arc;
use thiserror::Error;
use serenity::prelude::Mutex as SerenityMutex;

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

    #[error("Playback failed: {0}")]
    PlaybackFailed(String),
}

/// Result type for music operations
pub type MusicResult<T> = Result<T, MusicError>;

/// Manages Songbird instances and voice connections
pub struct MusicManager;

impl MusicManager {
    /// Get the Songbird voice client from the context
    pub async fn get_songbird(ctx: &Context) -> MusicResult<Arc<Songbird>> {
        let songbird = songbird::get(ctx)
            .await
            .ok_or(MusicError::NoVoiceManager)?;
        
        Ok(songbird)
    }
    
    /// Join a voice channel
    pub async fn join_channel(
        ctx: &Context, 
        guild_id: GuildId, 
        channel_id: ChannelId
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        let songbird = Self::get_songbird(ctx).await?;
        
        // Join the voice channel
        let handle = songbird.join(guild_id, channel_id).await
            .map_err(|e| MusicError::JoinError(e.to_string()))?;
        
        Ok(handle)
    }
    
    /// Leave a voice channel
    pub async fn leave_channel(
        ctx: &Context, 
        guild_id: GuildId
    ) -> MusicResult<()> {
        let songbird = Self::get_songbird(ctx).await?;
        
        // Check if we're in a voice channel
        if songbird.get(guild_id).is_none() {
            return Err(MusicError::NotConnected);
        }
        
        // Leave the voice channel
        songbird.remove(guild_id).await
            .map_err(|_| MusicError::JoinError("Failed to leave voice channel".to_string()))?;
        
        Ok(())
    }
    
    /// Get the current voice channel call handle
    pub async fn get_call(
        ctx: &Context, 
        guild_id: GuildId
    ) -> MusicResult<Arc<SerenityMutex<Call>>> {
        let songbird = Self::get_songbird(ctx).await?;
        
        let call = songbird.get(guild_id)
            .ok_or(MusicError::NotConnected)?;
        
        Ok(call)
    }
    
    /// Get the voice channel ID that the user is currently in
    pub fn get_user_voice_channel(
        ctx: &Context, 
        guild_id: GuildId, 
        user_id: serenity::UserId
    ) -> MusicResult<ChannelId> {
        // Get the guild
        let guild = ctx.cache.guild(guild_id)
            .ok_or(MusicError::NotInGuild)?;
        
        // Get the voice state of the user
        let voice_state = guild.voice_states.get(&user_id)
            .ok_or(MusicError::UserNotInVoiceChannel)?;
        
        // Get the channel ID
        let channel_id = voice_state.channel_id
            .ok_or(MusicError::UserNotInVoiceChannel)?;
        
        Ok(channel_id)
    }
}
