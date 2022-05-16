use std::env;
use reqwest::{header, header::HeaderMap};
use serde_json::Value;
use serenity::{client::Context, framework::standard::{
    Args, CommandResult,
    macros::command,
}, model::channel::Message};

#[command]
#[aliases("question", "q")]
#[description = "Ask OpenAI's GPT-3 DaVinci model a question"]
async fn question(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    const API_KEY: String = env::var("OPENAI_AUTH").expect("Unable to obtain OpenAI Auth");
    const CONTENT_TYPE: &str = "application/json";

    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, API_KEY.parse().unwrap());
    headers.insert(header::CONTENT_TYPE, CONTENT_TYPE.parse().unwrap());

    let body = format!["{{\
        \"prompt\": \"{}\", \
        \"max_tokens\": 512\
    }}", args.rest()];

    let v: Value = serde_json::from_str(&body)?;

    let client = reqwest::Client::new();
    let request_builder = client.post("https://api.openai.com/v1/engines/text-davinci-002/completions")
        .headers(headers)
        .json(&v);

    if let Ok(response) = request_builder.send().await {
        if let Ok(text) = response.text().await {

            let json: Value = serde_json::from_str(&text)?;

            let answer = json["choices"][0]["text"].as_str();

            if let Some(answer_text) = answer {
                let result = msg.reply(&ctx.http, answer_text).await;

                if let Err(why) = result {
                    println!("Unable to send message: {:?}", why);
                }
            }
        }
    }

    Ok(())
}