use super::*;
use utils::ollama_client::*;

/// Search the web with AI.
#[cfg(feature = "brave_search")]
#[poise::command(slash_command, category = "AI")]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Your search query"]
    #[rest]
    query: String,
) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();

    // Perform the search using Brave Search API
    match brave::search(&query).await {
        Ok(results) => {
            let formatted_results = brave::format_search_results(&results, &query);
            
            // Create a prompt for the LLM
            let prompt = format!(
                "I need you to analyze these search results and provide a helpful summary:\n\n{}\n\nPlease provide a concise summary of the information, highlighting the most relevant points related to the query.",
                formatted_results
            );
            
            let response = OLLAMA_CLIENT.clone().chat(&author, &prompt).await?;
            
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

#[cfg(not(feature = "brave_search"))]
#[poise::command(slash_command, category = "AI")]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Your search query"]
    #[rest]
    _query: String,
) -> CommandResult {
    ctx.say("The search command requires the 'brave_search' feature to be enabled. Please check the bot configuration.").await?;
    Ok(())
}