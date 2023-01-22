// use serenity::client::Context;
// use serenity::model::channel::Message;
// use serenity::framework::standard::{CommandResult, macros::command};
// use crate::CommandCounter;
// use std::fmt::Write;

// #[command]
// // Options are passed via subsequent attributes.
// // Make this command use the "complicated" bucket.
// #[bucket = "complicated"]
// async fn commands(ctx: &Context, msg: &Message) -> CommandResult {
//     let mut contents = "Commands used:\n".to_string();

//     let data = ctx.data.read().await;
//     let counter = data.get::<CommandCounter>().expect("Expected CommandCounter in TypeMap.");

//     for (k, v) in counter {
//         writeln!(contents, "- {name}: {amount}", name = k, amount = v)?;
//     }

//     msg.channel_id.say(&ctx.http, &contents).await?;

//     Ok(())
// }
