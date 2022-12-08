use std::fmt::Display;

use serenity::{
    client::Context,
    framework::standard::CommandResult,
    model::channel::Message, builder::CreateMessage,
};

pub async fn send_message<'a, F>(ctx: &Context, msg: &Message, f: F) -> CommandResult
    where
        for<'b> F: FnOnce(&'b mut CreateMessage<'a>) -> &'b mut CreateMessage<'a>,
        {
            msg.channel_id
                .send_message(&ctx.http, f)
                .await
                .into_command_result()
        }

pub async fn reply(ctx: &Context, msg: &Message, reply: impl Display) -> CommandResult {
    msg
        .reply(&ctx.http, reply)
        .await
        .into_command_result()
}

trait IntoCommandResult {
    fn into_command_result(self) -> CommandResult;
}

impl IntoCommandResult for Result<Message, serenity::Error> {
    fn into_command_result(self) -> CommandResult {
        match self {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e))
        }
    }
}