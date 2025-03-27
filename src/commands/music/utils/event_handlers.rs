use crate::commands::music::utils::{
    audio_sources::{AudioSource, TrackMetadata},
    autoplay_manager::is_autoplay_enabled,
    embedded_messages,
    queue_manager::{
        add_to_queue, clear_manual_stop_flag, get_channel_id, get_next_track,
        is_manual_stop_flag_set, set_current_track,
    },
};
use serenity::async_trait;
use poise::serenity_prelude as serenity;
use tracing::{error, info};

/// Event handler for when a song ends
pub struct SongEndNotifier {
    pub ctx: serenity::Context,
    pub guild_id: serenity::GuildId,
    pub call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
    pub track_metadata: TrackMetadata,
}

#[async_trait]
impl songbird::EventHandler for SongEndNotifier {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track(_) = ctx {
            self.handle_track_end().await;
        }
        None
    }
}

impl SongEndNotifier {
    async fn handle_track_end(&self) {
        info!("Track ended for guild {}", self.guild_id);

        let track_played = play_next_track(
            &self.ctx,
            self.guild_id,
            self.call.clone(),
            true,
        ).await.is_ok();

        if !track_played {
            self.handle_empty_queue().await;
        }
    }

    async fn handle_empty_queue(&self) {
        let manual_stop = is_manual_stop_flag_set(self.guild_id).await;

        if manual_stop {
            clear_manual_stop_flag(self.guild_id).await;
            return;
        }

        if is_autoplay_enabled(self.guild_id).await {
            if let Err(e) = self.attempt_autoplay().await {
                error!("Autoplay failed: {}", e);
            }
        }
    }

    async fn attempt_autoplay(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(url) = &self.track_metadata.url {
            let related_songs = AudioSource::get_related_songs(url).await?;

            for song in related_songs {
                if let Some(song_url) = &song.url {
                    if !AudioSource::is_youtube_video_url(song_url) {
                        continue;
                    }

                    let (source, _) = AudioSource::from_youtube_url(song_url).await?;
                    let queue_item = QueueItem {
                        input: source,
                        metadata: song.clone(),
                    };

                    add_to_queue(self.guild_id, queue_item).await?;
                    play_next_track(
                        &self.ctx,
                        self.guild_id,
                        self.call.clone(),
                        true,
                    ).await?;
                    
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Helper function to play the next track in the queue
/// Returns true if a track was played, false if the queue was empty
pub async fn play_next_track(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    call: std::sync::Arc<serenity::prelude::Mutex<songbird::Call>>,
    send_message: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    info!("Attempting to play next track for guild {}", guild_id);

    // Get the next track from the queue
    let queue_item = match get_next_track(guild_id).await? {
        Some(item) => item,
        None => {
            info!("No more tracks in queue for guild {}", guild_id);
            return Ok(false); // Indicate no track was played
        }
    };

    info!("Got next track from queue: {:?}", queue_item.metadata.title);

    // Get a lock on the call
    let mut handler = call.lock().await;
    info!("Obtained lock on voice handler, preparing to play audio");

    // Play the track and verify it started successfully
    let track_handle = handler.play_input(queue_item.input);
    info!("Track handle created");

    // Store the current track
    set_current_track(guild_id, track_handle.clone(), queue_item.metadata.clone()).await?;

    if send_message {
        // Send a now playing message
        if let Some(channel_id) = get_channel_id(guild_id).await {
            let embed = embedded_messages::now_playing(&queue_item.metadata);
            let message = serenity::CreateMessage::default().embeds(vec![embed]);
            channel_id.send_message(ctx, message).await?;
        }
    }

    // Set up a handler for when the track ends
    let ctx = ctx.clone();
    let call = call.clone();

    let _ = track_handle.add_event(
        songbird::Event::Track(songbird::TrackEvent::End),
        SongEndNotifier {
            ctx,
            guild_id,
            call,
            track_metadata: queue_item.metadata.clone(),
        },
    );

    Ok(true) // Indicate a track was played
}

/// Struct needed for QueueItem
pub use crate::commands::music::utils::queue_manager::QueueItem;
