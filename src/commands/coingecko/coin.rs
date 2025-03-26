use std::sync::LazyLock;

use chrono::Utc;
use futures::{Stream, StreamExt};
use poise::{CreateReply, serenity_prelude::Color};
use serenity::builder::CreateEmbedFooter;
use thousands::Separable;
use tokio::sync::RwLock;

use crate::{CommandResult, Context, serenity::CreateEmbed};

use super::*;

/// Coingecko API interactions
#[poise::command(slash_command, subcommands("price",), category = "General")]
pub async fn coin(_: Context<'_>) -> CommandResult {
    Ok(())
}

/// Get the current price of a cryptocurrency
#[poise::command(slash_command)]
async fn price(
    ctx: Context<'_>,
    #[description = "Coin symbol"]
    #[autocomplete = "autocomplete_coin_id"]
    symbol: String,
) -> CommandResult {
    ctx.defer().await?;

    let formatted = to_api_format(&symbol);
    let url = [API, "coins/", &formatted].concat();
    let query = vec![
        ("localization", "false"),
        ("tickers", "false"),
        ("market_data", "true"),
        ("community_data", "false"),
        ("developer_data", "false"),
    ];

    let result = send_request(&url, &query).await?;

    let coin_data = CoinInfo::from_json(&result);

    if let Some(coin_data) = coin_data {
        let color = match coin_data.market_data.usd_change_24h {
            x if x > 0.0 => Color::DARK_GREEN,
            x if x < 0.0 => Color::RED,
            _ => Color::GOLD,
        };

        let positive_change = if coin_data.market_data.perc_change_24h > 0.0 {
            "+"
        } else {
            ""
        };

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

        let embed = CreateEmbed::new()
            .fields(fields)
            .color(color)
            .title(coin_data.name)
            .thumbnail(coin_data.icon)
            .timestamp(Utc::now())
            .footer(CreateEmbedFooter::new("via CoinGecko"));

        let reply = CreateReply::default().embed(embed).ephemeral(false);

        ctx.send(reply).await?;

        Ok(())
    } else {
        Err(Box::new(CoingeckoError::Invalid))
    }
}

static COIN_CACHE: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| RwLock::new(Vec::new()));

async fn list_coin_ids() -> Vec<String> {
    // First check if cache is already populated
    {
        let cache = COIN_CACHE.read().await;
        if !cache.is_empty() {
            return cache.clone();
        }
    } // Read lock is dropped here

    // If cache is empty, fetch and populate it
    let mut results = Vec::new();
    let url = [API, "coins/list"].concat();
    let query = vec![("localization", "false")];

    let result = send_request(&url, &query).await;

    if let Ok(value) = result {
        if let Some(arr) = value.as_array() {
            results = arr
                .iter()
                .map(|coin| coin["id"].to_string().trim_matches('"').to_string())
                .collect();

            // Now update the cache with a write lock
            let mut cache = COIN_CACHE.write().await;
            *cache = results.clone(); // Replace the cache content
        }
    } else if let Err(e) = result {
        println!("Error listing coins: {}", e);
    }

    results
}

async fn autocomplete_coin_id<'a>(
    _ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    let coin_id_list = list_coin_ids().await;

    futures::stream::iter(
        coin_id_list
            .into_iter()
            .filter(move |id| id.starts_with(partial))
    )
}
