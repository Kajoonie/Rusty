//! This module contains the `/say` command and its helpers.
//! Text is spoken in a voice channel with DECtalk, the synthesizer known from Moonbase Alpha.
//! Requires the `tts` feature flag to be enabled. DECtalk itself is not part of the bot;
//! it is an external program located through the `DECTALK_PATH` environment variable.

/// Submodule that runs the external DECtalk program and builds playable clips.
pub(crate) mod dectalk;
/// Submodule handling voice channel checks, the per-guild clip queue and playback.
pub(crate) mod playback;
/// Submodule defining the `/say` command.
pub(crate) mod speak;
/// Submodule for cleaning user text and formatting the reply.
pub(crate) mod text;

use std::time::Duration;

use poise::serenity_prelude::ChannelId;
use thiserror::Error;
use tracing::{info, warn};

use crate::{CommandResult, Context};

/// Represents errors specific to text-to-speech operations.
///
/// The messages are shown to Discord users, so they contain no server paths
/// or program output. Details are logged where the error is created.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum TtsError {
    /// Operation requires being in a guild context.
    #[error("Not in a guild")]
    NotInGuild,

    /// The text was empty after cleaning.
    #[error("There is nothing to say. Give me some text.")]
    EmptyText,

    /// The text exceeds `text::MAX_TEXT_CHARS`.
    #[error("That text is too long ({length} characters). The limit is {limit}.")]
    TextTooLong {
        /// Number of characters in the cleaned text.
        length: usize,
        /// The maximum number of characters allowed.
        limit: usize,
    },

    /// The DECtalk executable was not found.
    #[error("Text-to-speech is unavailable: DECtalk is not installed on the bot's server.")]
    NotInstalled,

    /// The DECtalk process could not be started.
    #[error("Text-to-speech is unavailable: DECtalk could not be started.")]
    SpawnFailed,

    /// DECtalk did not finish within `dectalk::SYNTH_TIMEOUT`.
    #[error("DECtalk took longer than {} seconds and was stopped.", .0.as_secs())]
    Timeout(Duration),

    /// DECtalk exited with an error or its output could not be read.
    #[error("DECtalk failed to speak that text.")]
    SynthesisFailed,

    /// DECtalk ran successfully but produced no audio.
    #[error("DECtalk produced no audio for that text.")]
    NoAudio,

    /// The user invoking the command is not in a voice channel.
    #[error("You need to be in a voice channel to use text-to-speech.")]
    UserNotInVoiceChannel,

    /// The bot is already connected to another voice channel in the guild.
    #[error("I'm already in <#{0}>. Join that channel to use text-to-speech.")]
    InDifferentChannel(ChannelId),

    /// Failed to retrieve the Songbird voice manager.
    #[error("Failed to get voice manager")]
    NoVoiceManager,

    /// Failed to join the voice channel.
    #[error("Failed to join voice channel: {0}")]
    JoinError(String),

    /// Too many clips are already waiting to be spoken in the guild.
    #[error("Too many messages are waiting to be spoken. Try again in a moment.")]
    QueueFull,
}

/// A specialized `Result` type for text-to-speech operations.
pub type TtsResult<T> = Result<T, TtsError>;

/// Logs whether DECtalk is available.
///
/// Unlike the `yt-dlp` check this never panics: the bot starts without DECtalk
/// and `/say` replies with an error until it is installed.
pub fn check_dectalk() {
    let config = dectalk::config();

    if !config.is_installed() {
        warn!(
            "DECtalk not found at {} (set {} to the `say` executable); /say will reply with an error",
            config.say_path.display(),
            dectalk::DECTALK_PATH_ENV
        );
        return;
    }

    match config.run_as {
        Some(run_as) => info!(
            "DECtalk found at {}; it will run as uid {} gid {}",
            config.say_path.display(),
            run_as.uid,
            run_as.gid
        ),
        None => info!(
            "DECtalk found at {}; it will run as the bot's own user",
            config.say_path.display()
        ),
    }
}
