//! This module aggregates all the command modules for the bot.

/// Commands related to AI interactions (e.g., chat, model management).
pub(crate) mod ai;
/// Commands for interacting with the CoinGecko API (e.g., fetching coin prices).
pub(crate) mod coingecko;
/// General purpose commands (e.g., ping, help).
pub(crate) mod general;

/// Commands related to music playback (requires the `music` feature).
#[cfg(feature = "music")]
pub(crate) mod music;
