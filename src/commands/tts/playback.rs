//! Voice channel handling and playback for `/say`.
//!
//! Clips are played directly on the guild's voice call, next to (not in) the music
//! queue, so Songbird mixes them over whatever song is playing. Each guild has its own
//! small queue so clips are spoken one after another in the order they were requested.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::async_trait;
use serenity::client::Context;
use serenity::model::id::{ChannelId, GuildId, UserId};
use serenity::prelude::Mutex as SerenityMutex;
use songbird::events::EventData;
use songbird::tracks::{PlayMode, Track, TrackHandle};
use songbird::{Call, Event, EventContext, Songbird, TrackEvent};
use tokio::sync::{Mutex, Notify, OwnedMutexGuard, mpsc};
use tracing::{debug, error, warn};

use crate::commands::music::utils::music_manager::MusicManager;

use super::{TtsError, TtsResult, dectalk::Clip};

/// Volume factor applied to the current song while a clip is spoken.
pub const DUCKED_MUSIC_VOLUME: f32 = 0.25;
/// How many clips may wait in a guild's queue, not counting the one being spoken.
pub const MAX_PENDING_CLIPS: usize = 10;
/// Extra time allowed for a clip to finish before it is given up on.
const CLIP_END_GRACE: Duration = Duration::from_secs(5);

/// What has to happen before a clip can be played in the caller's channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceAction {
    /// The bot is already in the caller's voice channel.
    AlreadyConnected,
    /// The bot is not in a voice channel and should join this one.
    Join(ChannelId),
}

/// Decides how to reach the caller's voice channel without ever moving the bot.
///
/// * `user_channel` - the voice channel the caller is in, if any.
/// * `bot_channel` - the voice channel the bot is connected to in the guild, if any.
pub fn voice_action(
    user_channel: Option<ChannelId>,
    bot_channel: Option<ChannelId>,
) -> TtsResult<VoiceAction> {
    let user_channel = user_channel.ok_or(TtsError::UserNotInVoiceChannel)?;

    match bot_channel {
        None => Ok(VoiceAction::Join(user_channel)),
        Some(bot_channel) if bot_channel == user_channel => Ok(VoiceAction::AlreadyConnected),
        Some(bot_channel) => Err(TtsError::InDifferentChannel(bot_channel)),
    }
}

/// Returns the voice channel the bot is connected (or connecting) to in a guild.
async fn bot_voice_channel(songbird: &Songbird, guild_id: GuildId) -> Option<ChannelId> {
    let call = songbird.get(guild_id)?;
    let channel = call.lock().await.current_channel()?;

    Some(ChannelId::new(channel.0.get()))
}

/// Checks the caller's and the bot's voice channels and decides what to do.
///
/// Has no side effects, so it can be used for an early check before any work is done.
pub async fn check_voice(
    ctx: &Context,
    guild_id: GuildId,
    user_id: UserId,
) -> TtsResult<VoiceAction> {
    let songbird = MusicManager::get_songbird(ctx)
        .await
        .map_err(|_| TtsError::NoVoiceManager)?;

    let user_channel = MusicManager::get_user_voice_channel(ctx, guild_id, user_id).ok();
    let bot_channel = bot_voice_channel(&songbird, guild_id).await;

    voice_action(user_channel, bot_channel)
}

/// Makes sure the bot is in the caller's voice channel, joining it if the bot is not
/// connected anywhere, and returns the guild's voice call.
pub async fn connect(
    ctx: &Context,
    guild_id: GuildId,
    user_id: UserId,
) -> TtsResult<Arc<SerenityMutex<Call>>> {
    let songbird = MusicManager::get_songbird(ctx)
        .await
        .map_err(|_| TtsError::NoVoiceManager)?;

    match check_voice(ctx, guild_id, user_id).await? {
        VoiceAction::AlreadyConnected => songbird
            .get(guild_id)
            .ok_or_else(|| TtsError::JoinError("the voice connection was closed".to_string())),
        VoiceAction::Join(channel_id) => match songbird.join(guild_id, channel_id).await {
            Ok(call) => Ok(call),
            Err(e) => {
                error!(
                    "Failed to join voice channel {} for guild {}: {}",
                    channel_id, guild_id, e
                );
                // A failed join leaves a half-open call behind; reset it so the next
                // request starts from a clean state.
                let _ = songbird.leave(guild_id).await;
                Err(TtsError::JoinError(e.to_string()))
            }
        },
    }
}

/// A clip waiting to be spoken, with the voice call to play it on.
pub struct QueuedClip {
    /// The guild's voice call.
    pub call: Arc<SerenityMutex<Call>>,
    /// The synthesized speech.
    pub clip: Clip,
}

/// Handle to a guild's clip queue.
#[derive(Clone)]
pub struct GuildQueue {
    /// Held while a request is prepared, so requests are queued in the order they arrive.
    order: Arc<Mutex<()>>,
    /// Feeds the task that speaks the guild's clips one at a time.
    sender: mpsc::Sender<QueuedClip>,
}

/// A reserved place in a guild's clip queue.
pub struct QueueSlot(mpsc::OwnedPermit<QueuedClip>);

impl GuildQueue {
    /// Waits for this request's turn to be prepared. Turns are given in arrival order.
    pub async fn wait_turn(&self) -> OwnedMutexGuard<()> {
        self.order.clone().lock_owned().await
    }

    /// Reserves a place in the queue, or fails with `QueueFull`.
    pub fn reserve(&self) -> TtsResult<QueueSlot> {
        self.sender
            .clone()
            .try_reserve_owned()
            .map(QueueSlot)
            .map_err(|_| TtsError::QueueFull)
    }
}

impl QueueSlot {
    /// Puts a clip in the reserved place. It is spoken once the clips before it are done.
    pub fn fill(self, clip: QueuedClip) {
        self.0.send(clip);
    }
}

/// Maps GuildId to the guild's clip queue.
static GUILD_QUEUES: LazyLock<StdMutex<HashMap<GuildId, GuildQueue>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

/// Returns the clip queue for a guild, starting its playback task on first use.
pub fn guild_queue(guild_id: GuildId) -> GuildQueue {
    let mut queues = GUILD_QUEUES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    queues
        .entry(guild_id)
        .or_insert_with(|| GuildQueue {
            order: Arc::new(Mutex::new(())),
            sender: spawn_sequential(MAX_PENDING_CLIPS, move |queued| play_clip(guild_id, queued)),
        })
        .clone()
}

/// Starts a task that handles items strictly one after another, in the order sent.
///
/// At most `capacity` items can wait behind the one being handled.
fn spawn_sequential<T, F, Fut>(capacity: usize, mut handle: F) -> mpsc::Sender<T>
where
    T: Send + 'static,
    F: FnMut(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let (sender, mut receiver) = mpsc::channel(capacity);

    tokio::spawn(async move {
        while let Some(item) = receiver.recv().await {
            handle(item).await;
        }
    });

    sender
}

/// A Songbird event handler that signals when a clip has finished playing.
struct ClipEndNotifier {
    /// The ID of the guild the clip is played in.
    guild_id: GuildId,
    /// Notified once when the clip ends, is stopped, or fails.
    finished: Arc<Notify>,
}

#[async_trait]
impl songbird::EventHandler for ClipEndNotifier {
    /// Called for the clip's `TrackEvent::End`, which Songbird also fires after an error.
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(tracks) = ctx {
            for (state, _handle) in *tracks {
                if let PlayMode::Errored(e) = &state.playing {
                    error!("TTS clip failed to play in guild {}: {}", self.guild_id, e);
                }
            }
        }

        self.finished.notify_one();
        None
    }
}

/// The song whose volume was lowered for a clip, and the volume to put back.
struct DuckedMusic {
    track: TrackHandle,
    original_volume: f32,
}

/// The volume a song is lowered to while a clip is spoken.
fn ducked_volume(original_volume: f32) -> f32 {
    original_volume * DUCKED_MUSIC_VOLUME
}

/// Lowers the volume of the music queue's current track, if there is one.
async fn duck_music(current: Option<TrackHandle>) -> Option<DuckedMusic> {
    let track = current?;
    let original_volume = track.get_info().await.ok()?.volume;

    track.set_volume(ducked_volume(original_volume)).ok()?;

    Some(DuckedMusic {
        track,
        original_volume,
    })
}

impl DuckedMusic {
    /// Puts the song's volume back. Does nothing if the song has ended meanwhile.
    fn restore(self) {
        if let Err(e) = self.track.set_volume(self.original_volume) {
            debug!("Song ended before its volume was restored: {}", e);
        }
    }
}

/// Speaks one clip and waits until it has finished.
///
/// The clip is played outside the music queue, so it is mixed over the current song and
/// never shows up as a queue item. The song is turned down for the duration and turned
/// back up afterwards, whether the clip ended normally, failed, or timed out.
async fn play_clip(guild_id: GuildId, queued: QueuedClip) {
    let QueuedClip { call, clip } = queued;

    let current_song = {
        let call = call.lock().await;

        if call.current_channel().is_none() {
            warn!(
                "Dropping TTS clip for guild {}: no longer in a voice channel",
                guild_id
            );
            return;
        }

        call.queue().current()
    };

    let finished = Arc::new(Notify::new());
    let mut track = Track::from(clip.wav);
    track.events.add_event(
        EventData::new(
            Event::Track(TrackEvent::End),
            ClipEndNotifier {
                guild_id,
                finished: finished.clone(),
            },
        ),
        Duration::ZERO,
    );

    let ducked = duck_music(current_song).await;
    let clip_handle = call.lock().await.play(track);

    // The end event never arrives if the bot is disconnected mid-clip, so don't wait forever.
    let limit = clip.duration + CLIP_END_GRACE;
    if tokio::time::timeout(limit, finished.notified())
        .await
        .is_err()
    {
        warn!(
            "TTS clip in guild {} did not finish within {:?}; stopping it",
            guild_id, limit
        );
        let _ = clip_handle.stop();
    }

    if let Some(ducked) = ducked {
        ducked.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn channel(id: u64) -> ChannelId {
        ChannelId::new(id)
    }

    #[test]
    fn test_voice_action_requires_caller_in_voice() {
        assert_eq!(
            voice_action(None, None),
            Err(TtsError::UserNotInVoiceChannel)
        );
        assert_eq!(
            voice_action(None, Some(channel(1))),
            Err(TtsError::UserNotInVoiceChannel)
        );
    }

    #[test]
    fn test_voice_action_joins_when_bot_is_not_connected() {
        assert_eq!(
            voice_action(Some(channel(1)), None),
            Ok(VoiceAction::Join(channel(1)))
        );
    }

    #[test]
    fn test_voice_action_reuses_connection_in_same_channel() {
        assert_eq!(
            voice_action(Some(channel(1)), Some(channel(1))),
            Ok(VoiceAction::AlreadyConnected)
        );
    }

    #[test]
    fn test_voice_action_never_moves_the_bot() {
        let error = voice_action(Some(channel(1)), Some(channel(2))).unwrap_err();
        assert_eq!(error, TtsError::InDifferentChannel(channel(2)));
        assert_eq!(
            error.to_string(),
            "I'm already in <#2>. Join that channel to use text-to-speech."
        );
    }

    #[test]
    fn test_ducked_volume_is_a_quarter() {
        assert_eq!(ducked_volume(1.0), 0.25);
        assert_eq!(ducked_volume(0.5), 0.125);
    }

    #[tokio::test]
    async fn test_spawn_sequential_runs_in_order_without_overlap() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let running = Arc::new(AtomicUsize::new(0));
        let (done_tx, mut done_rx) = mpsc::unbounded_channel();

        let sender = {
            let log = log.clone();
            let running = running.clone();
            spawn_sequential(8, move |item: u32| {
                let log = log.clone();
                let running = running.clone();
                let done_tx = done_tx.clone();
                async move {
                    assert_eq!(running.fetch_add(1, Ordering::SeqCst), 0, "overlap");
                    // Earlier items take longer, so any overlap or reordering would show.
                    tokio::time::sleep(Duration::from_millis(u64::from(40 - item * 10))).await;
                    log.lock().unwrap().push(item);
                    running.fetch_sub(1, Ordering::SeqCst);
                    let _ = done_tx.send(());
                }
            })
        };

        for item in 0..4 {
            sender.try_send(item).unwrap();
        }
        for _ in 0..4 {
            done_rx.recv().await.unwrap();
        }

        assert_eq!(*log.lock().unwrap(), [0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn test_guild_queue_rejects_clips_when_full() {
        // A queue whose handler never finishes, so nothing drains.
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let queue = GuildQueue {
            order: Arc::new(Mutex::new(())),
            sender: spawn_sequential(MAX_PENDING_CLIPS, move |_clip: QueuedClip| {
                let _ = started_tx.send(());
                std::future::pending::<()>()
            }),
        };

        // Slots can be reserved without clips, which is all this test needs.
        let slots: Vec<_> = (0..MAX_PENDING_CLIPS)
            .map(|_| queue.reserve().expect("room in the queue"))
            .collect();
        assert!(matches!(queue.reserve(), Err(TtsError::QueueFull)));

        // Giving a slot back makes room again.
        drop(slots);
        assert!(queue.reserve().is_ok());
        assert!(started_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_wait_turn_is_first_come_first_served() {
        use tokio_test::{assert_pending, assert_ready, task};

        let queue = GuildQueue {
            order: Arc::new(Mutex::new(())),
            sender: mpsc::channel(1).0,
        };

        let first = queue.wait_turn().await;

        // Two more requests arrive, in this order, while the first holds the turn.
        let mut second = task::spawn(queue.wait_turn());
        let mut third = task::spawn(queue.wait_turn());
        assert_pending!(second.poll());
        assert_pending!(third.poll());

        // The turn passes to the second request, even if the third asks first.
        drop(first);
        assert_pending!(third.poll());
        let second_turn = assert_ready!(second.poll());
        assert_pending!(third.poll());

        drop(second_turn);
        assert_ready!(third.poll());
    }

    #[tokio::test]
    async fn test_guild_queue_is_shared_per_guild() {
        let a = guild_queue(GuildId::new(424_242));
        let b = guild_queue(GuildId::new(424_242));
        let other = guild_queue(GuildId::new(424_243));

        assert!(Arc::ptr_eq(&a.order, &b.order));
        assert!(a.sender.same_channel(&b.sender));
        assert!(!Arc::ptr_eq(&a.order, &other.order));
        assert!(!a.sender.same_channel(&other.sender));
    }
}
