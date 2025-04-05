use crate::utils::ollama_client::OLLAMA_CLIENT;
use std::env;

use super::*;
use tracing::{debug, error, info};

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

    info!("Processing search request from {}: {}", author.name, query);

    // Get API Key
    let api_key = match env::var("BRAVE_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            error!("BRAVE_API_KEY not found in environment for search command");
            ctx.say("Search functionality is disabled: Missing API key configuration.")
                .await?;
            return Ok(());
        }
    };

    // Perform the search using Brave Search API
    match brave::search(&query, "https://api.search.brave.com", &api_key).await {
        // Pass api_key
        Ok(results) => {
            debug!("Received search results for query: {}", query);
            let formatted_results = brave::format_search_results(&results, &query);

            // Create a prompt for the LLM
            let prompt = format!(
                "I need you to analyze these search results and provide a helpful summary:\n\n{}\n\nPlease provide a concise summary of the information, highlighting the most relevant points related to the query.",
                formatted_results
            );

            info!(
                "Requesting AI analysis of search results for {}",
                author.name
            );
            match OLLAMA_CLIENT.clone().chat(author, &prompt).await {
                Ok(response) => {
                    let content = response.message.content;
                    debug!("Received AI summary for search query");

                    let full_message =
                        format!("**Search Query**: {query}\n\n**AI Summary**: {content}");
                    info!("Sending formatted search response to {}", author.name);

                    chunk_response(ctx, full_message).await
                }
                Err(e) => {
                    error!("Failed to get AI analysis for search results: {}", e);
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("Failed to perform search for {}: {}", author.name, e);
            ctx.say(format!("Error performing search: {}", e)).await?;
            Ok(())
        }
    }
}
