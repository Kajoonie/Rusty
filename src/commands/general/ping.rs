//! Defines the `/ping` command for checking bot latency.

use ::serenity::all::CreateEmbed;
use poise::{CreateReply, serenity_prelude as serenity};
use std::time::Duration;

use crate::{CommandResult, Context};

/// Responds with the bot's current API latency for the relevant shard.
///
/// Calculates the latency using the `get_shard_latency` helper function and
/// displays it in an embed message.
#[poise::command(slash_command, category = "General")]
pub async fn ping(ctx: Context<'_>) -> CommandResult {
    // Get the latency of the shard the command was invoked on.
    let latency = get_shard_latency(&ctx)
        .await
        .unwrap_or_default()
        .as_millis();

    // Create an embed to display the latency.
    let embed = CreateEmbed::new()
        .title("Pong!")
        .thumbnail("attachment://pong.png")
        .field("API Latency", format!("{} ms", latency), false);

    // Build the reply message with the embed.
    let reply = CreateReply::default().embed(embed).ephemeral(false);

    // Send the reply.
    ctx.send(reply).await?;

    Ok(())
}

/// Retrieves the latency of the current shard, if available.
/// 
/// The shard manager is an interface for mutating, stopping, restarting, and
/// retrieving information about shards.
/// 
/// Shards are backed by a "shard runner" responsible for processing events
/// over the shard, so we'll get the information about the shard runner for
/// the shard this command was sent over.
async fn get_shard_latency(ctx: &Context<'_>) -> Option<Duration> {
    // Get a handle to the shard manager.
    let shard_manager = ctx.framework().shard_manager();

    let manager = shard_manager.clone();
    // Lock the runners map to access shard runner information.
    let runners = manager.runners.lock().await;
    
    // Get the runner for the specific shard ID associated with the context.
    let runner = runners.get(&serenity::ShardId(ctx.serenity_context().shard_id.0))?;

    // Return the latency reported by the runner.
    runner.latency
}
