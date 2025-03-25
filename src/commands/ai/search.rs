use super::*;
use utils::ollama_client::*;
use tracing::{debug, info, error};

/// Search the web with AI.
#[cfg(feature = "brave_search")]
#[poise::command(slash_command, category = "AI")]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Your search query"]
    #[rest]
    query: String,
) -> CommandResult {
    let author = ctx.author();
    debug!("Search request received from user {}", author.name);
    
    ctx.defer().await?;

    info!("Processing search request from {}: {}", author.name, query);

    // Perform the search using Brave Search API
    match brave::search(&query).await {
        Ok(results) => {
            debug!("Received search results for query: {}", query);
            let formatted_results = brave::format_search_results(&results, &query);
            
            // Create a prompt for the LLM
            let prompt = format!(
                "I need you to analyze these search results and provide a helpful summary:\n\n{}\n\nPlease provide a concise summary of the information, highlighting the most relevant points related to the query.",
                formatted_results
            );
            
            info!("Requesting AI analysis of search results for {}", author.name);
            match OLLAMA_CLIENT.clone().chat(author, &prompt).await {
                Ok(response) => {
                    let content = response.message.content;
                    debug!("Received AI summary for search query");
                    
                    let full_message = format!("**Search Query**: {query}\n\n**AI Summary**: {content}");
                    info!("Sending formatted search response to {}", author.name);
                    
                    chunk_response(ctx, full_message).await
                },
                Err(e) => {
                    error!("Failed to get AI analysis for search results: {}", e);
                    Err(e.into())
                }
            }
        },
        Err(e) => {
            error!("Failed to perform search for {}: {}", author.name, e);
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
    debug!("Search command called but brave_search feature is disabled");
    ctx.say("The search command requires the 'brave_search' feature to be enabled. Please check the bot configuration.").await?;
    Ok(())
}