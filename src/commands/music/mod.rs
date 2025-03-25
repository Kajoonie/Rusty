pub(crate) mod autoplay;
pub(crate) mod leave;
pub(crate) mod pause;
pub(crate) mod play;
pub(crate) mod queue;
pub(crate) mod remove;
pub(crate) mod skip;
pub(crate) mod stop;

#[cfg(feature = "music")]
pub(crate) mod utils;

#[cfg(feature = "music")]
use poise::{serenity_prelude as serenity, CreateReply};
#[cfg(feature = "music")]
use serenity::all::CreateEmbed;

#[cfg(feature = "music")]
use crate::{CommandResult, Context};
