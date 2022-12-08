use rusty::reply;
use serde_json::{json, Value};
use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

use super::openai::{OpenAiError, OpenAiRequest};

const ENDPOINT: &str = "https://api.openai.com/v1/engines/text-davinci-003/completions";

#[command]
#[aliases("question", "q")]
#[sub_commands(sarcastic, neato)]
#[description = "Ask OpenAI's GPT-3 DaVinci model a question"]
async fn question(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let question = args.remains().unwrap_or(
        "Please think of a good question to ask an AI, then provide me an answer to that question.",
    );

    match send_request(question).await {
        Ok(answer) => reply(ctx, msg, answer).await,
        Err(e) => reply(ctx, msg, e).await,
    }
}

#[command]
async fn sarcastic(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer = if let Some(question) = args.remains() {
        send_request(["Give me a sarcastic answer: ", question].concat().as_str()).await
    } else {
        send_request("Tell me something sarcastic.").await
    };

    match answer {
        Ok(answer) => reply(ctx, msg, answer).await,
        Err(e) => reply(ctx, msg, e).await,
    }
}

#[command]
async fn neato(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer = if let Some(question) = args.remains() {
        send_request(
            ["Incorporate the word 'neato' into your answer: ", question]
                .concat()
                .as_str(),
        )
        .await
    } else {
        send_request("Tell me something neato.").await
    };

    match answer {
        Ok(answer) => reply(ctx, msg, answer).await,
        Err(e) => reply(ctx, msg, e).await,
    }
}

async fn send_request(question: &str) -> Result<String, OpenAiError> {
    let body = json!({
        "prompt": format!("{question}"),
        "max_tokens": 250
    });

    let request = OpenAiRequest::new(valid_json_path, error_json_path);
    request.send_request(ENDPOINT, body).await
}

fn valid_json_path(json: &Value) -> &Value {
    &json["choices"][0]["text"]
}

fn error_json_path(json: &Value) -> &Value {
    &json["error"]["message"]
}
