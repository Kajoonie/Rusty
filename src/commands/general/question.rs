use reqwest::{header, header::HeaderMap};
use serde_json::{json, Error, Value};
use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

use crate::openai_api_key;

#[command]
#[aliases("question", "q")]
#[sub_commands(sarcastic, neato)]
#[description = "Ask OpenAI's GPT-3 DaVinci model a question"]
async fn question(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer = if let Some(question) = args.remains() {
        send_request(question).await
    } else {
        send_request("Please think of a good question to ask an AI, then provide me an answer to that question.").await
    };

    reply(ctx, msg, answer).await
}

#[command]
async fn sarcastic(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer = if let Some(question) = args.remains() {
        send_request(["Give me a sarcastic answer: ", question].concat().as_str()).await
    } else {
        send_request("Tell me something sarcastic.").await
    };

    reply(ctx, msg, answer).await
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

    reply(ctx, msg, answer).await
}

fn build_api_auth_header() -> HeaderMap {
    let api_auth = ["Bearer ", openai_api_key().as_str()].concat();

    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, api_auth.parse().unwrap());

    headers
}

fn build_request_body(prompt: &str) -> Value {
    json!({
        "prompt": format!("{}", prompt),
        "max_tokens": 250
    })
}

async fn send_request(question: &str) -> Option<String> {
    let body = build_request_body(question);

    let client = reqwest::Client::new();
    let request_builder = client
        .post("https://api.openai.com/v1/engines/text-davinci-003/completions")
        .headers(build_api_auth_header())
        .json(&body);

    if let Ok(response) = request_builder.send().await {
        if let Ok(text) = response.text().await {
            let result: Result<Value, Error> = serde_json::from_str(&text);

            return match result {
                Ok(json) => match &json["choices"][0]["text"] {
                    Value::String(text) => Some(text.to_owned()),
                    _ => match &json["error"]["message"] {
                        Value::String(error_message) => Some(error_message.to_owned()),
                        _ => None,
                    },
                },
                _ => None,
            };
        }
    }

    None
}

async fn reply(ctx: &Context, msg: &Message, answer: Option<String>) -> CommandResult {
    if let Some(answer_text) = answer {
        let result = msg.reply(&ctx.http, answer_text).await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }
    }

    Ok(())
}
