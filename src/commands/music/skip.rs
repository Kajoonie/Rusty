use super::*;
use crate::commands::music::utils::{
    music_manager::{MusicManager, MusicError},
    queue_manager::{get_current_track, get_next_track, set_current_track, queue_length, get_queue},
};
use std::time::Duration;

/// Skip the currently playing song
#[poise::command(slash_command, category = "Music")]
pub async fn skip(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current voice call
    let call = match MusicManager::get_call(ctx.serenity_context(), guild_id).await {
        Ok(call) => call,
        Err(err) => {
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("❌ Error")
                    .description(format!("Not connected to a voice channel: {}", err))
                    .color(0xff0000)))
                .await?;
            return Ok(());
        }
    };

    // Get the current track
    let current_track = get_current_track(guild_id).await?;
    
    // Stop the current track if there is one
    if let Some((track, _)) = current_track {
        track.stop()?;
    }

    // Get the next track from the queue
    let next_track = match get_next_track(guild_id).await? {
        Some(queue_item) => queue_item,
        None => {
            ctx.send(CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("⏭️ Queue Empty")
                    .description("No more tracks in the queue")
                    .color(0xffaa00)))
                .await?;
            return Ok(());
        }
    };

    // Get a lock on the call and play the next track
    let mut handler = call.lock().await;
    let track_handle = handler.play_input(next_track.input);

    // Store the current track
    set_current_track(guild_id, track_handle.clone(), next_track.metadata.clone()).await?;

    // Set up a handler for when the track ends
    let serenity_ctx = ctx.serenity_context().clone();
    let call = call.clone();

    let _ = track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        SongEndNotifier {
            ctx: serenity_ctx,
            guild_id,
            call,
        },
    );

    // Send success message with the new track's info
    let mut embed = CreateEmbed::new()
        .title("⏭️ Skipped Track")
        .description(format!("**Now Playing:** [{}]({})",
            next_track.metadata.title,
            next_track.metadata.url.as_deref().unwrap_or("#")
        ))
        .color(0x00ff00);

    // Add duration if available
    if let Some(duration) = next_track.metadata.duration {
        embed = embed.field("Duration", format!("`{}`", format_duration(duration)), true);
    }

    // Add thumbnail if available
    if let Some(thumbnail) = next_track.metadata.thumbnail {
        embed = embed.thumbnail(thumbnail);
    }

    // Add remaining queue info
    let queue_length = queue_length(guild_id).await?;
    if queue_length > 0 {
        let queue = get_queue(guild_id).await?;
        let total_duration: Duration = queue.iter()
            .filter_map(|track| track.duration)
            .sum();
        
        if total_duration.as_secs() > 0 {
            embed = embed.field(
                "Up Next",
                format!("`{} tracks` • Total Length: `{}`",
                    queue_length,
                    format_duration(total_duration)
                ),
                true
            );
        } else {
            embed = embed.field(
                "Up Next",
                format!("`{} tracks`", queue_length),
                true
            );
        }
    }

    ctx.send(CreateReply::default()
        .embed(embed))
        .await?;

    Ok(())
}

/// Event handler for when a song ends
struct SongEndNotifier {
    ctx: serenity::Context,
    guild_id: serenity::GuildId,
    call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
}

#[async_trait::async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, _ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        // Check if the track ended naturally (not paused or stopped)
        if let Ok(Some((track, _))) = get_current_track(self.guild_id).await {
            let track_info = track.get_info().await;
            if let Ok(track_state) = track_info {
                if track_state.playing == songbird::tracks::PlayMode::End {
                    let _ = play_next_track(&self.ctx, self.guild_id, self.call.clone()).await;
                }
            }
        }
        None
    }
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

    // Get a lock on the call
    let mut handler = call.lock().await;

    // Play the track
    let track_handle = handler.play_input(queue_item.input);

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

/// Format a duration into a human-readable string
fn format_duration(duration: std::time::Duration) -> String {
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