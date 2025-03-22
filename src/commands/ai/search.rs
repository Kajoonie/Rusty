use super::*;

/// Search the web with AI.
#[poise::command(slash_command, category = "AI")]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Your search query"]
    #[rest]
    query: String,
) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();
    let author_str = format!("{}{}", author.name, author.id);
    let model = database::get_user_model(&author.id.to_string());

    // Perform the search using Brave Search API
    match brave::search(&query).await {
        Ok(results) => {
            let formatted_results = brave::format_search_results(&results, &query);
            
            // Create a prompt for the LLM
            let prompt = format!(
                "I need you to analyze these search results and provide a helpful summary:\n\n{}\n\nPlease provide a concise summary of the information, highlighting the most relevant points related to the query.",
                formatted_results
            );
            
            // Get conversation history
            let chat_history = get_conversation_history(&author_str);
            
            // Send to LLM
            let response = send_request_with_model(prompt, chat_history, &model).await?;
            
            let content = response.message.content;
            let full_message = format!("**Search Query**: {query}\n\n**AI Summary**: {content}");
            
            chunk_response(ctx, full_message).await
        },
        Err(e) => {
            ctx.say(format!("Error performing search: {}", e)).await?;
            Ok(())
        }
    }
}