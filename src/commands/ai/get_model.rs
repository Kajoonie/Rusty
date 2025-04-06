//! Defines the `/get_model` command for retrieving the user's current AI model.

use super::*;
use tracing::{debug, info};

/// Retrieves and displays the AI model currently configured for the user.
///
/// It fetches the user's preferred model from the database. If no specific model
/// is set for the user, it might fall back to a default (though the current implementation
/// informs the user if none is set).
#[poise::command(slash_command, category = "AI")]
pub async fn get_model(ctx: Context<'_>) -> CommandResult {
    let author = ctx.author();
    debug!("Get model request received from user {}", author.name);

    // Defer response while fetching data.
    ctx.defer().await?;

    info!("Fetching model preference for user {}", author.name);
    // Attempt to retrieve the user's preferred model from the database.
    let model = match database::get_user_model(author) {
        Some(model) => model,
        None => {
            // Inform the user if no model is set and exit.
            ctx.say(
                "You do not have a user-defined model set or a default model available".to_string(),
            )
            .await?;
            return Ok(());
        }
    };

    debug!("Retrieved model '{}' for user {}", model, author.name);
    
    // Send the retrieved model name back to the user.
    ctx.say(format!("Your currently active model is: **{}**", model))
        .await?;
    Ok(())
}
