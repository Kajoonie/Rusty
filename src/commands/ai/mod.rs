pub(crate) mod chat;
pub(crate) mod get_model;
pub(crate) mod list_models;
#[cfg(feature = "brave_search")]
pub(crate) mod search;
pub(crate) mod set_model;

mod utils;

#[cfg(feature = "brave_search")]
use crate::brave;
use crate::database::{self, UserPreference};
use crate::CommandResult;
use crate::Context;

const MAX_MESSAGE_LENGTH: usize = 2000;

pub async fn chunk_response<S: AsRef<str>>(ctx: Context<'_>, response: S) -> CommandResult {
    let response = response.as_ref();
    let mut iter = response.chars();
    let mut pos = 0;
    while pos < response.len() {
        let mut len = 0;
        for ch in iter.by_ref().take(MAX_MESSAGE_LENGTH) {
            len += ch.len_utf8();
        }
        ctx.say(&response[pos..pos + len]).await?;
        pos += len;
    }

    Ok(())
}
