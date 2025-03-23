pub(crate) mod play;
pub(crate) mod queue;
pub(crate) mod skip;
pub(crate) mod stop;
pub(crate) mod leave;

use poise::{serenity_prelude as serenity, CreateReply};
use serenity::all::CreateEmbed;

use crate::{CommandResult, Context};