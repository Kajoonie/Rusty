//! This module contains all commands and utilities related to music playback.
//! Requires the `music` feature flag to be enabled.

/// Submodule defining the `/autoplay` command.
pub(crate) mod autoplay;
/// Submodule defining the `/play` command.
pub(crate) mod play;
/// Submodule defining the `/remove` command.
pub(crate) mod remove;

/// Submodule containing logic for different audio sources (YouTube, Spotify, etc.).
pub(crate) mod audio_sources;
/// Submodule containing shared utilities for music commands (managers, handlers, etc.).
pub(crate) mod utils;

use crate::{CommandResult, Context};
