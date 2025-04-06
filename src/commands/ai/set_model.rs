//! Defines the `/set_model` command for setting the user's preferred AI model.
//! Also includes helper functions for model listing and autocompletion.

use ollama_rs::models::LocalModel;

use crate::{
    utils::database::{self, UserPreference},
    utils::ollama_client::OLLAMA_CLIENT,
};

use super::*;
use futures::{Stream, StreamExt};
use tracing::{debug, error, info, warn};

/// Sets the user's preferred AI model for future interactions.
///
/// Validates the chosen model against the list of available local models.
/// If valid, saves the preference to the database. Uses `autocomplete_model`
/// for suggesting available models.
#[poise::command(slash_command, category = "AI")]
pub async fn set_model(
    ctx: Context<'_>,
    #[description = "The model to use"]
    #[autocomplete = "autocomplete_model"]
    model: String,
) -> CommandResult {
    let author = ctx.author();
    debug!("Set model request received from user {}", author.name);

    // Defer response while processing.
    ctx.defer().await?;

    info!("Fetching available models for validation");
    // Fetch the list of currently available models to validate the user's choice.
    let models = list_models().await;

    // Check if the requested model exists in the available list.
    if !models.iter().any(|m| m.name == model) {
        warn!(
            "User {} attempted to set invalid model: {}",
            author.name, model
        );
        // Inform the user if the model is invalid and exit.
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
    // Create a UserPreference struct to store in the database.
    let pref = UserPreference {
        user_id: author.id.to_string(),
        username: author.name.clone(),
        model: model.clone(),
    };

    // Attempt to save the user's preference to the database.
    match database::set_user_preference(&pref) {
        Ok(_) => {
            info!(
                "Successfully set model preference for {} to '{}'",
                author.name, model
            );
            // Confirm success to the user.
            ctx.say(format!("Your preferred model has been set to '{}'", model))
                .await?;
        }
        Err(e) => {
            error!("Failed to set model preference for {}: {}", author.name, e);
            // Inform the user about the failure.
            ctx.say(format!("Failed to set model preference: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Helper function to fetch the list of local models from the Ollama client.
/// Returns an empty vector if fetching fails.
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

/// Autocomplete function for the `model` argument in the `/set_model` command.
///
/// Filters the list of available local models based on the user's partial input.
async fn autocomplete_model<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    debug!("Processing model autocomplete for partial: '{}'", partial);
    // Fetch the full list of models.
    let model_list = list_models().await;

    // Create a stream from the model list.
    futures::stream::iter(model_list.into_iter())
        // Filter the stream: keep models whose names start with the partial input.
        .filter(move |model| futures::future::ready(model.name.starts_with(partial)))
        // Map the filtered models to just their names (as strings).
        .map(|model| model.name.to_string())
}
