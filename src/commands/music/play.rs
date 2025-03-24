use super::*;
use crate::commands::music::utils::{
    music_manager::{MusicManager, MusicError},
    audio_sources::AudioSource,
    queue_manager::{QueueItem, add_to_queue, get_current_track, is_queue_empty, queue_length, get_next_track, set_current_track, get_queue},
};
use poise::serenity_prelude::{self as serenity, CreateEmbed};
use songbird::tracks::PlayMode;
use std::time::Duration;
use async_trait::async_trait;
use tracing::{info, error, warn, debug};

/// Play a song from YouTube or a direct URL
#[poise::command(slash_command, category = "Music")]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"] query: String,
) -> CommandResult {
    info!("Received play command with query: {}", query);
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the user's voice channel
    let user_id = ctx.author().id;
    let channel_id = match MusicManager::get_user_voice_channel(ctx.serenity_context(), guild_id, user_id) {
        Ok(channel_id) => channel_id,
        Err(err) => {
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("❌ Error")
                    .description(format!("You need to be in a voice channel: {}", err))
                    .color(0xff0000))
                .ephemeral(true))
                .await?;
            return Ok(());
        }
    };

    // Defer the response since audio processing might take time
    ctx.defer().await?;

    // Join the voice channel if not already connected
    let call = match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        Ok(call) => call,
        Err(_) => {
            // Not connected, so join the channel
            match MusicManager::join_channel(ctx.serenity_context(), guild_id, channel_id).await {
                Ok(call) => call,
                Err(err) => {
                    ctx.send(CreateReply::default()
                        .embed(CreateEmbed::new()
                            .title("❌ Error")
                            .description(format!("Failed to join voice channel: {}", err))
                            .color(0xff0000)))
                        .await?;
                    return Ok(());
                }
            }
        }
    };

    // Process the query to get an audio source
    info!("Processing audio source for query: {}", query);
    let (source, metadata) = match AudioSource::from_query(&query).await {
        Ok(result) => {
            let (src, meta) = result;
            info!("Successfully created audio source. Metadata: {:?}", meta);
            (src, meta)
        },
        Err(err) => {
            error!("Failed to create audio source: {}", err);
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("❌ Error")
                    .description(format!("Failed to process audio source: {}", err))
                    .color(0xff0000)))
                .await?;
            return Ok(());
        }
    };

    // Create a queue item
    debug!("Creating queue item with metadata: {:?}", metadata);
    let queue_item = QueueItem {
        input: source,
        metadata: metadata.clone(),
    };

    // Check if we're already playing something
    let is_empty = is_queue_empty(guild_id).await.unwrap_or(true);

    // Add the track to the queue
    if let Err(err) = add_to_queue(guild_id, queue_item).await {
        ctx.send(CreateReply::default()
            .embed(CreateEmbed::new()
                .title("❌ Error")
                .description(format!("Failed to add track to queue: {}", err))
                .color(0xff0000)))
            .await?;
        return Ok(());
    }

    // If nothing is currently playing, start playback
    if is_empty {
        play_next_track(ctx.serenity_context(), guild_id, call).await?;
    }

    // Get the queue length
    let position = queue_length(guild_id).await.unwrap_or(0);

    // Send a success message
    let title = metadata.title.clone();
    let url = metadata.url.clone().unwrap_or_else(|| "#".to_string());
    let duration_str = metadata.duration
        .map(format_duration)
        .unwrap_or_else(|| "Unknown duration".to_string());

    let mut embed = if position <= 1 {
        // Playing now
        CreateEmbed::new()
            .title("🎵 Now Playing")
            .description(format!("[{}]({})", title, url))
            .field("Duration", format!("`{}`", duration_str), true)
            .color(0x00ff00)
    } else {
        // Added to queue
        CreateEmbed::new()
            .title("🎵 Added to Queue")
            .description(format!("[{}]({})", title, url))
            .field("Duration", format!("`{}`", duration_str), true)
            .field("Position", format!("`#{}`", position), true)
            .color(0x00ff00)
    };

    // Add thumbnail if available
    if let Some(thumbnail) = metadata.thumbnail {
        embed = embed.thumbnail(thumbnail);
    }

    // Add queue information
    let queue_length = queue_length(guild_id).await?;
    if queue_length > 1 {
        let total_duration: Duration = get_queue(guild_id).await?
            .iter()
            .filter_map(|track| track.duration)
            .sum();
        
        if total_duration.as_secs() > 0 {
            embed = embed.field(
                "Queue Info", 
                format!("`{} tracks` • Total Length: `{}`", 
                    queue_length,
                    format_duration(total_duration)
                ),
                false
            );
        } else {
            embed = embed.field(
                "Queue Info",
                format!("`{} tracks`", queue_length),
                false
            );
        }
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// Helper function to play the next track in the queue
async fn play_next_track(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
) -> CommandResult {
    // Get the next track from the queue
    let queue_item = match get_next_track(guild_id).await? {
        Some(item) => item,
        None => return Ok(()),
    };

    info!("Attempting to play track: {:?}", queue_item.metadata.title);

    // Get a lock on the call
    let mut handler = call.lock().await;
    info!("Obtained lock on voice handler, preparing to play audio");

    // Play the track and verify it started successfully
    debug!("Starting playback of audio input");
    let track_handle = handler.play_input(queue_item.input);
    info!("Track handle created, waiting to verify playback");
    
    // Wait a short moment and check if playback started
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    match track_handle.get_info().await {
        Ok(info) => {
            info!("Track info retrieved - State: {:?}, Position: {:?}", info.playing, info.position);
            if info.playing == PlayMode::Play {
                info!("Track playback started successfully");
            } else {
                warn!("Track not playing after initialization. PlayMode: {:?}", info.playing);
                return Err(Box::new(MusicError::PlaybackFailed("Track failed to start playing".into())));
            }
        },
        Err(e) => {
            error!("Failed to get track info: {}", e);
            return Err(Box::new(MusicError::PlaybackFailed(format!("Failed to verify playback: {}", e))));
        }
    }

    // Store the current track
    set_current_track(guild_id, track_handle.clone(), queue_item.metadata.clone()).await?;

    // Set up a handler for when the track ends
    let ctx = ctx.clone();
    let call = call.clone();

    let _ = track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        SongEndNotifier {
            ctx,
            guild_id,
            call,
        },
    );

    Ok(())
}

/// Event handler for when a song ends
struct SongEndNotifier {
    ctx: serenity::Context,
    guild_id: serenity::GuildId,
    call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, _ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        // Check if the track ended naturally (not paused or stopped)
        if let Ok(Some((track, _))) = get_current_track(self.guild_id).await {
            let track_info = track.get_info().await;
            if let Ok(track_state) = track_info {
                if track_state.playing == PlayMode::End {
                    let _ = play_next_track(&self.ctx, self.guild_id, self.call.clone()).await;
                }
            }
        }

        None
    }
}

/// Format a duration into a human-readable string
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    if minutes >= 60 {
        let hours = minutes / 60;
        let minutes = minutes % 60;
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}