use crate::utils::ollama_client::OLLAMA_CLIENT;

use super::*;
use tracing::{debug, error, info};

/// List all local models.
#[poise::command(slash_command, category = "AI")]
pub async fn list_models(ctx: Context<'_>) -> CommandResult {
    let author = ctx.author();
    debug!("List models request received from user {}", author.name);

    ctx.defer().await?;

    info!("Fetching list of available models");
    match OLLAMA_CLIENT.clone().list_models().await {
        Ok(models) => {
            debug!("Retrieved {} models", models.len());

            let mut model_list = "Here are the available models:\n".to_string();
            for model in models.iter() {
                model_list.push_str(&format!("- {}\n", model.name));
            }

            info!("Sending model list to {}", author.name);
            ctx.say(model_list).await?;
            Ok(())
        }
        Err(e) => {
            error!("Failed to fetch models list: {}", e);
            Err(e.into())
        }
    }
}
