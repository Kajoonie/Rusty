use std::env;

use reqwest::{header, header::HeaderMap};
use serde_json::{Error, json, Value};
use serenity::{client::Context, framework::standard::{
    Args, CommandResult,
    macros::command,
}, model::channel::Message};

#[command]
#[aliases("question", "q")]
#[sub_commands(sarcastic, neato)]
#[description = "Ask OpenAI's GPT-3 DaVinci model a question"]
async fn question(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let answer;
    if let Some(question) = args.remains() {
        answer = send_request(question).await;
    } else {
        answer = send_request("Please think of a good question to ask an AI, then provide me an answer to that question.").await;
    }

    reply(ctx, msg, answer).await
}

#[command]
async fn sarcastic(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer;
    if let Some(question) = args.remains() {
        answer = send_request(["Give me a sarcastic answer: ", question].concat().as_str()).await;
    } else {
        answer = send_request("Tell me something sarcastic.").await;
    }

    reply(ctx, msg, answer).await
}

#[command]
async fn neato(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let answer;
    if let Some(question) = args.remains() {
        answer = send_request(["Incorporate the word 'neato' into your answer: ", question].concat().as_str()).await;
    } else {
        answer = send_request("Tell me something neato.").await;
    }

    reply(ctx, msg, answer).await
}

async fn build_api_auth_header() -> HeaderMap {
    let api_key = env::var("OPENAI_API_KEY").expect("Unable to obtain OpenAI Auth");
    let api_auth = ["Bearer ", &api_key].concat();


    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, api_auth.parse().unwrap());

    headers
}

async fn build_request_body(prompt: &str) -> Value {
    json!({
        "prompt": format!("{}", prompt),
        "max_tokens": 250
    })
}

async fn send_request(question: &str) -> Option<String> {
    let body = build_request_body(question).await;

    let client = reqwest::Client::new();
    let request_builder = client.post("https://api.openai.com/v1/engines/text-davinci-002/completions")
        .headers(build_api_auth_header().await)
        .json(&body);

    if let Ok(response) = request_builder.send().await {
        if let Ok(text) = response.text().await {

            let result: Result<Value, Error> = serde_json::from_str(&text);

            return match result {
                Ok(json) => {
                    let json_str = json["choices"][0]["text"].as_str().to_owned();
                    json_str.map(String::from)
                },
                _ => None
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