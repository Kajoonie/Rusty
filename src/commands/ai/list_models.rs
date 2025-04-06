//! Defines the `/list_models` command for displaying available AI models.

use crate::utils::ollama_client::OLLAMA_CLIENT;

use super::*;
use tracing::{debug, error, info};

/// Fetches and lists all AI models available locally via the Ollama client.
///
/// It retrieves the list of models, formats them into a readable string,
/// and sends the list back to the user in the channel.
#[poise::command(slash_command, category = "AI")]
pub async fn list_models(ctx: Context<'_>) -> CommandResult {
    let author = ctx.author();
    debug!("List models request received from user {}", author.name);

    // Defer response while fetching data.
    ctx.defer().await?;

    info!("Fetching list of available models");
    // Call the Ollama client to get the list of locally available models.
    match OLLAMA_CLIENT.clone().list_models().await {
        Ok(models) => {
            debug!("Retrieved {} models", models.len());

            // Format the list of model names.
            let mut model_list = "Here are the available models:\n".to_string();
            for model in models.iter() {
                model_list.push_str(&format!("- {}\n", model.name));
            }

            info!("Sending model list to {}", author.name);
            // Send the formatted list back to the user.
            ctx.say(model_list).await?;
            Ok(())
        }
        Err(e) => {
            // Log the error and return it.
            error!("Failed to fetch models list: {}", e);
            Err(e.into())
        }
    }
}
