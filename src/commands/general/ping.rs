use std::env;
use std::time::Duration;
use serenity::{
    framework::standard::{
        CommandResult,
        macros::command,
    },
    model::channel::Message,
    client::Context,
    http::AttachmentType
};
use crate::ShardManagerContainer;
use serenity::client::bridge::gateway::ShardId;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    let mut path = env::current_dir()?;
    let latency = get_shard_latency(ctx).await.unwrap_or_default().as_millis();

    let result = msg.channel_id.send_message(&ctx.http, |m| {
        m.embed(|e| e
            .title("Pong!")
            .thumbnail("attachment://pong.png")
            .field("API Latency", &format!("{} ms", latency), false));

        path.push("src/img/pong.png");
        m.add_file(AttachmentType::Path(&*path));

        m
    }).await;

    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }

    Ok(())
}

async fn get_shard_latency(ctx: &Context) -> Option<Duration> {
    // The shard manager is an interface for mutating, stopping, restarting, and
    // retrieving information about shards.
    let data = ctx.data.read().await;

    let shard_manager = data.get::<ShardManagerContainer>()?;

    let manager = shard_manager.lock().await;
    let runners = manager.runners.lock().await;

    // Shards are backed by a "shard runner" responsible for processing events
    // over the shard, so we'll get the information about the shard runner for
    // the shard this command was sent over.
    let runner = runners.get(&ShardId(ctx.shard_id))?;

    return runner.latency;
}
