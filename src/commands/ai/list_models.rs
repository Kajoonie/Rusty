use super::*;
use utils::ollama_client::*;

/// List all local models.
#[poise::command(slash_command, category = "AI")]
pub async fn list_models(ctx: Context<'_>) -> CommandResult {
    ctx.defer().await?;

    let models = OLLAMA_CLIENT.clone().list_models().await?;

    let mut model_list = "Here are the available models:\n".to_string();
    for model in models.iter() {
        model_list.push_str(&format!("- {}\n", model.name));
    }

    ctx.say(model_list).await?;
    Ok(())
}