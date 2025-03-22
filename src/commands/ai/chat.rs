use super::*;

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
    let author_str = format!("{}{}", author.name, author.id);
    let model = database::get_user_model(&author.id.to_string());

    let chat_history = get_conversation_history(&author_str);
    let response = send_request_with_model(message.clone(), chat_history, &model).await?;

    let content = response.message.content;
    let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);

    chunk_response(ctx, full_message).await
}