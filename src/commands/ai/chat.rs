use super::*;
use tracing::{debug, error, info};
use utils::ollama_client::*;

/// Chat with the AI
#[poise::command(slash_command, category = "AI")]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your chat message"]
    #[rest]
    message: String,
) -> CommandResult {
    let author = ctx.author();
    debug!("Chat request received from user {}", author.name);

    ctx.defer().await?;

    info!("Processing chat request from {}: {}", author.name, message);

    match OLLAMA_CLIENT.clone().chat(author, &message).await {
        Ok(response) => {
            let content = response.message.content;
            debug!("Received AI response for {}", author.name);

            let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);
            info!("Sending formatted response to {}", author.name);

            chunk_response(ctx, full_message).await
        }
        Err(e) => {
            error!("Failed to get AI response for {}: {}", author.name, e);
            Err(e.into())
        }
    }
}
