//! Defines the `/search` command, which performs a web search using the Brave Search API
//! and then uses an AI model to summarize the results.
//! This command requires the `brave_search` feature to be enabled.

use crate::utils::ollama_client::OLLAMA_CLIENT;
use std::env;

use super::*;
use tracing::{debug, error, info};

/// Search the web and get an AI summary of the results.
///
/// Uses the Brave Search API for web results based on the user's query.
/// The results are then passed to an AI model for summarization.
/// Requires the `BRAVE_API_KEY` environment variable.
/// The final summary is sent back to Discord, chunked if necessary.
#[poise::command(slash_command, category = "AI")]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Your search query"]
    #[rest]
    query: String,
) -> CommandResult {
    // Defer the response immediately as search and AI processing can take time.
    ctx.defer().await?;

    let author = ctx.author();

    info!("Processing search request from {}: {}", author.name, query);

    // Retrieve the Brave Search API key from environment variables.
    let api_key = match env::var("BRAVE_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            error!("BRAVE_API_KEY not found in environment for search command");
            // Inform the user and exit if the API key is missing.
            ctx.say("Search functionality is disabled: Missing API key configuration.")
                .await?;
            return Ok(());
        }
    };

    // Perform the web search using the utility function.
    match brave::search(&query, "https://api.search.brave.com", &api_key).await {
        // Pass api_key
        Ok(results) => {
            debug!("Received search results for query: {}", query);
            // Format the raw search results into a string suitable for the AI.
            let formatted_results = brave::format_search_results(&results, &query);

            // Construct the prompt for the AI model, including the formatted search results.
            let prompt = format!(
                "I need you to analyze these search results and provide a helpful summary:\n\n{}\n\nPlease provide a concise summary of the information, highlighting the most relevant points related to the query.",
                formatted_results
            );

            info!(
                "Requesting AI analysis of search results for {}",
                author.name
            );
            // Send the prompt to the AI model for summarization.
            match OLLAMA_CLIENT.clone().chat(author, &prompt).await {
                Ok(response) => {
                    let content = response.message.content;
                    debug!("Received AI summary for search query");

                    let full_message =
                    // Format the final response including the original query and the AI summary.
                        format!("**Search Query**: {query}\n\n**AI Summary**: {content}");
                    info!("Sending formatted search response to {}", author.name);

                    // Send the potentially long response, chunking if necessary.
                    chunk_response(ctx, full_message).await
                }
                Err(e) => {
                    error!("Failed to get AI analysis for search results: {}", e);
                    // Handle errors during AI processing.
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("Failed to perform search for {}: {}", author.name, e);
            // Handle errors during the Brave search itself.
            ctx.say(format!("Error performing search: {}", e)).await?;
            Ok(())
        }
    }
}
