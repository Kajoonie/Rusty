//! Defines the `/chat` command for interacting with the AI model.

use crate::utils::ollama_client::OLLAMA_CLIENT;

use super::*;
use tracing::{debug, error, info};

/// Sends a message to the configured AI model and displays the response.
///
/// This command takes a user's message, sends it to the Ollama client,
/// formats the user's message and the AI's response, and sends it back
/// to the channel, handling potential message length limits by chunking.
#[poise::command(slash_command, category = "AI")]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your chat message"]
    #[rest]
    message: String,
) -> CommandResult {
    let author = ctx.author();
    debug!("Chat request received from user {}", author.name);

    // Defer the response to indicate the bot is processing.
    ctx.defer().await?;

    info!("Processing chat request from {}: {}", author.name, message);

    // Call the Ollama client to get the chat response.
    match OLLAMA_CLIENT.clone().chat(author, &message).await {
        Ok(response) => {
            let content = response.message.content;
            debug!("Received AI response for {}", author.name);

            // Format the message including the user's prompt and the AI's response.
            let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);
            info!("Sending formatted response to {}", author.name);

            // Send the potentially long response, chunking if necessary.
            chunk_response(ctx, full_message).await
        }
        Err(e) => {
            error!("Failed to get AI response for {}: {}", author.name, e);
            // Log the error and return it.
            Err(e.into())
        }
    }
}
