//! Defines commands for interacting with the CoinGecko API, primarily fetching coin prices.

use std::sync::LazyLock;

use chrono::Utc;
use futures::Stream;
use poise::{CreateReply, serenity_prelude::Color};
use serenity::builder::CreateEmbedFooter;
use thousands::Separable;
use tokio::sync::RwLock;

use crate::{CommandResult, Context, serenity::CreateEmbed};

use super::*;

/// Commands for interacting with the CoinGecko API.
///
/// This command group currently only contains the `price` subcommand.
#[poise::command(slash_command, subcommands("price",), category = "General")]
pub async fn coin(_: Context<'_>) -> CommandResult {
    Ok(())
}

/// Fetches and displays the current price and 24h change for a given cryptocurrency.
///
/// Uses the CoinGecko API `/coins/{id}` endpoint. The `symbol` argument uses
/// `autocomplete_coin_id` to suggest valid CoinGecko coin IDs.
#[poise::command(slash_command)]
async fn price(
    ctx: Context<'_>,
    #[description = "Coin symbol"]
    #[autocomplete = "autocomplete_coin_id"]
    #[rest]
    symbol: String,
) -> CommandResult {
    // Defer response while fetching data.
    ctx.defer().await?;

    // Format the user input symbol for the API (e.g., lowercase).
    let formatted = to_api_format(&symbol);
    // Construct the API path and query parameters.
    let path = format!("coins/{}", formatted);
    let query = vec![
        ("localization", "false"),
        ("tickers", "false"),
        ("market_data", "true"),
        ("community_data", "false"),
        ("developer_data", "false"),
    ];

    // Send the request to the CoinGecko API.
    let result = send_request(super::API, &path, &query).await?;

    // Parse the JSON response into the CoinInfo struct.
    let coin_data = CoinInfo::from_json(&result);

    if let Some(coin_data) = coin_data {
        // Determine embed color based on 24h price change.
        let color = match coin_data.market_data.usd_change_24h {
            x if x > 0.0 => Color::DARK_GREEN,
            x if x < 0.0 => Color::RED,
            _ => Color::GOLD,
        };

        // Determine if the change is positive for formatting.
        let positive_change = if coin_data.market_data.perc_change_24h > 0.0 {
            "+"
        } else {
            ""
        };

        // Prepare fields for the embed.
        let fields = vec![
            (
                "Price",
                format!(
                    "${}",
                    coin_data.market_data.price_usd.separate_with_commas()
                ),
                false,
            ),
            (
                "Change ($)",
                format!(
                    "{positive_change}${}",
                    coin_data.market_data.usd_change_24h.separate_with_commas()
                ),
                true,
            ),
            (
                "Change (%)",
                format!(
                    "{positive_change}{:.4}%",
                    coin_data.market_data.perc_change_24h.separate_with_commas()
                ),
                true,
            ),
        ];

        // Build the response embed.
        let embed = CreateEmbed::new()
            .fields(fields)
            .color(color)
            .title(coin_data.name)
            .thumbnail(coin_data.icon)
            .timestamp(Utc::now())
            .footer(CreateEmbedFooter::new("via CoinGecko"));

        // Create the reply message with the embed.
        let reply = CreateReply::default().embed(embed).ephemeral(false);

        // Send the reply.
        ctx.send(reply).await?;

        Ok(())
    } else {
        // Handle cases where parsing the coin data failed.
        Err(Box::new(CoingeckoError::Invalid))
    }
}

/// Lazily initialized, thread-safe cache for CoinGecko coin IDs.
/// Used to avoid repeatedly fetching the full list for autocompletion.
static COIN_CACHE: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Fetches the list of all coin IDs from CoinGecko or retrieves it from the cache.
///
/// Checks the `COIN_CACHE` first. If empty, it fetches the list from the
/// `/coins/list` endpoint, populates the cache, and returns the list.
/// Subsequent calls will return the cached list directly.
async fn list_coin_ids() -> Vec<String> {
    // Attempt to read from the cache first.
    {
        let cache = COIN_CACHE.read().await;
        if !cache.is_empty() {
            return cache.clone();
        }
    } // Read lock is dropped here

    // Cache is empty or read lock failed; proceed to fetch.
    let mut results = Vec::new();
    let path = "coins/list";
    let query = vec![("localization", "false")];

    // Fetch the list from the API.
    let result = send_request(super::API, path, &query).await;

    if let Ok(value) = result {
        // Parse the JSON array response.
        if let Some(arr) = value.as_array() {
            results = arr
                .iter()
                .map(|coin| coin["id"].to_string().trim_matches('"').to_string())
                .collect();

            // Sort the results alphabetically before caching.
            results.sort_unstable();

            // Acquire a write lock to update the cache.
            let mut cache = COIN_CACHE.write().await;
            *cache = results.clone(); // Replace the cache content
        }
    } else if let Err(e) = result {
        println!("Error listing coins: {}", e);
    }

    results
}

/// Autocomplete function for the `symbol` argument in the `/coin price` command.
///
/// Filters the cached list of CoinGecko coin IDs based on the user's partial input.
async fn autocomplete_coin_id<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    // Get the list of coin IDs (potentially fetching/caching it).
    let coin_id_list = list_coin_ids().await;

    // Perform binary search to find the range of matching coin IDs.
    // `partition_point` finds the index `k` such that all elements at indices `< k` satisfy the
    // predicate and all elements at indices `>= k` do not.
    // `start` will be the index of the first element >= `partial`.
    let start = coin_id_list.partition_point(|id| id.as_str() < partial);

    // Find the end index (exclusive). This partitions the slice starting from `start`
    // based on whether the element starts with `partial`. The partition point is the index
    // (relative to the slice `[start..]`) of the first element that *doesn't* start with `partial`.
    // Adding `start` gives the absolute index in the original `coin_id_list`.
    let end = start + coin_id_list[start..].partition_point(|id| id.starts_with(partial));

    // Create a stream from the slice of matching IDs, limit to 25, and clone the strings.
    futures::stream::iter(
        coin_id_list[start..end].to_vec().into_iter().take(25), // Limit to 25 suggestions
    )
}
