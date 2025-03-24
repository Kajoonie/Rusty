use poise::{serenity_prelude as serenity, CreateReply};
use ::serenity::all::CreateEmbed;
use std::time::Duration;

use crate::{CommandResult, Context};

/// Ping the bot to check its latency
#[poise::command(slash_command, category = "General")]
pub async fn ping(ctx: Context<'_>) -> CommandResult {
    let latency = get_shard_latency(&ctx)
        .await
        .unwrap_or_default()
        .as_millis();

    let embed = CreateEmbed::new()
        .title("Pong!")
        .thumbnail("attachment://pong.png")
        .field("API Latency", format!("{} ms", latency), false);

    let reply = CreateReply::default()
        .embed(embed)
        .ephemeral(false);

    ctx.send(reply).await?;

    Ok(())
}

async fn get_shard_latency(ctx: &Context<'_>) -> Option<Duration> {
    // The shard manager is an interface for mutating, stopping, restarting, and
    // retrieving information about shards.
    let shard_manager = ctx.framework().shard_manager();

    let manager = shard_manager.clone();
    let runners = manager.runners.lock().await;

    // Shards are backed by a "shard runner" responsible for processing events
    // over the shard, so we'll get the information about the shard runner for
    // the shard this command was sent over.
    let runner = runners.get(&serenity::ShardId(ctx.serenity_context().shard_id.0))?;

    runner.latency
}
