use super::*;
use utils::ollama_client::*;

/// Chat with the AI
#[poise::command(slash_command, category = "AI")]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your chat message"]
    #[rest]
    message: String,
) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();

    let response = OLLAMA_CLIENT.clone().chat(author, &message).await?;

    let content = response.message.content;
    
    let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);

    chunk_response(ctx, full_message).await
}