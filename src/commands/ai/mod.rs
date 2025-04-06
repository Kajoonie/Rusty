//! This module contains all commands related to Artificial Intelligence features,
//! primarily interacting with an Ollama client and potentially other services like Brave Search.

/// Submodule defining the `/chat` command.
pub(crate) mod chat;
/// Submodule defining the `/get_model` command.
pub(crate) mod get_model;
/// Submodule defining the `/list_models` command.
pub(crate) mod list_models;
/// Submodule defining the `/set_model` command.
pub(crate) mod set_model;

/// Submodule defining the `/search` command (requires `brave_search` feature).
#[cfg(feature = "brave_search")]
pub(crate) mod search;

use crate::CommandResult;
use crate::Context;
#[cfg(feature = "brave_search")]
use crate::utils::brave;
use crate::utils::database;

/// The maximum character length allowed for a single Discord message.
const MAX_MESSAGE_LENGTH: usize = 2000;

/// Sends a potentially long response string by splitting it into chunks
/// that respect Discord's message length limit.
///
/// # Arguments
///
/// * `ctx` - The command context.
/// * `response` - The response string to send. Can be any type that implements `AsRef<str>`.
pub async fn chunk_response<S: AsRef<str>>(ctx: Context<'_>, response: S) -> CommandResult {
    let response = response.as_ref();
    // Create a character iterator for the response string.
    let mut iter = response.chars();
    let mut pos = 0;
    // Loop while there are still characters left to process.
    while pos < response.len() {
        let mut len = 0;
        // Iterate through the next chunk of characters up to the max length.
        for ch in iter.by_ref().take(MAX_MESSAGE_LENGTH) {
            len += ch.len_utf8();
        }
        // Send the current chunk.
        ctx.say(&response[pos..pos + len]).await?;
        // Update the position marker.
        pos += len;
    }

    Ok(())
}
