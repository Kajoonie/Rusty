use ollama_rs::models::LocalModel;

use crate::{
    utils::database::{self, UserPreference},
    utils::ollama_client::OLLAMA_CLIENT,
};

use super::*;
use futures::{Stream, StreamExt};
use tracing::{debug, error, info, warn};

/// Set the AI model to use for chat.
#[poise::command(slash_command, category = "AI")]
pub async fn set_model(
    ctx: Context<'_>,
    #[description = "The model to use"]
    #[autocomplete = "autocomplete_model"]
    model: String,
) -> CommandResult {
    let author = ctx.author();
    debug!("Set model request received from user {}", author.name);

    ctx.defer().await?;

    info!("Fetching available models for validation");
    let models = list_models().await;

    if !models.iter().any(|m| m.name == model) {
        warn!(
            "User {} attempted to set invalid model: {}",
            author.name, model
        );
        ctx.say(format!(
            "Model '{}' is not available. Use `/ai list_models` to see available models.",
            model
        ))
        .await?;
        return Ok(());
    }

    info!(
        "Setting model preference for {} to '{}'",
        author.name, model
    );
    let pref = UserPreference {
        user_id: author.id.to_string(),
        username: author.name.clone(),
        model: model.clone(),
    };

    match database::set_user_preference(&pref) {
        Ok(_) => {
            info!(
                "Successfully set model preference for {} to '{}'",
                author.name, model
            );
            ctx.say(format!("Your preferred model has been set to '{}'", model))
                .await?;
        }
        Err(e) => {
            error!("Failed to set model preference for {}: {}", author.name, e);
            ctx.say(format!("Failed to set model preference: {}", e))
                .await?;
        }
    }

    Ok(())
}

async fn list_models() -> Vec<LocalModel> {
    debug!("Fetching list of available local models");
    let model_list = OLLAMA_CLIENT.clone().list_models().await;

    match model_list {
        Ok(models) => {
            debug!("Successfully retrieved {} local models", models.len());
            models
        }
        Err(e) => {
            error!("Error listing local models: {}", e);
            Vec::new()
        }
    }
}

async fn autocomplete_model<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    debug!("Processing model autocomplete for partial: '{}'", partial);
    let model_list = list_models().await;

    futures::stream::iter(model_list.into_iter())
        .filter(move |model| futures::future::ready(model.name.starts_with(partial)))
        .map(|model| model.name.to_string())
}
