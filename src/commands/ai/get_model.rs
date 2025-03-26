use super::*;
use tracing::{debug, info};

/// Get the current AI model being used by the user.
#[poise::command(slash_command, category = "AI")]
pub async fn get_model(ctx: Context<'_>) -> CommandResult {
    let author = ctx.author();
    debug!("Get model request received from user {}", author.name);

    ctx.defer().await?;

    info!("Fetching model preference for user {}", author.name);
    let model = match database::get_user_model(author) {
        Some(model) => model,
        None => {
            ctx.say(format!(
                "You do not have a user-defined model set or a default model available"
            ))
            .await?;
            return Ok(());
        }
    };

    debug!("Retrieved model '{}' for user {}", model, author.name);
    info!("Sending model information to {}", author.name);
    ctx.say(format!("Your currently active model is: **{}**", model))
        .await?;
    Ok(())
}
