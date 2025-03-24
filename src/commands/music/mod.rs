#[cfg(feature = "music")]
pub(crate) mod play;
#[cfg(feature = "music")]
pub(crate) mod queue;
#[cfg(feature = "music")]
pub(crate) mod skip;
#[cfg(feature = "music")]
pub(crate) mod stop;
#[cfg(feature = "music")]
pub(crate) mod leave;
#[cfg(feature = "music")]
pub(crate) mod remove;
#[cfg(feature = "music")]
pub(crate) mod pause;
#[cfg(feature = "music")]
pub(crate) mod utils;

#[cfg(feature = "music")]
use poise::{serenity_prelude as serenity, CreateReply};
#[cfg(feature = "music")]
use serenity::all::CreateEmbed;

#[cfg(feature = "music")]
use crate::{CommandResult, Context};