use chrono::Utc;
use poise::{serenity_prelude::Color, CreateReply};
use serenity::builder::CreateEmbedFooter;
use thousands::Separable;

use crate::{serenity::CreateEmbed, CommandResult, Context};

use super::*;

#[poise::command(slash_command, subcommands("price",), category = "General")]
pub async fn coin(_: Context<'_>) -> CommandResult {
    Ok(())
}

#[poise::command(slash_command)]
async fn price(
    ctx: Context<'_>,
    #[description = "Coin symbol"]
    #[rest]
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

        let reply = CreateReply::default()
            .embed(embed)
            .ephemeral(false);

        ctx.send(reply).await?;

        Ok(())
    } else {
        Err(Box::new(CoingeckoError::Invalid))
    }
}
