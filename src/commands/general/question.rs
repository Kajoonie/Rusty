use std::env;
use reqwest::{header, header::HeaderMap};
use serde_json::{json, Value};
use serenity::{client::Context, framework::standard::{
    Args, CommandResult,
    macros::command,
}, model::channel::Message};

#[command]
#[aliases("question", "q")]
#[description = "Ask OpenAI's GPT-3 DaVinci model a question"]
async fn question(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let api_auth = "Bearer ".to_owned() + &env::var("OPENAI_AUTH")
        .expect("Unable to obtain OpenAI Auth");

    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, api_auth.parse().unwrap());

    let body = json!({
        "prompt": format!("{}", args.rest()),
        "max_tokens": 512
    });

    let client = reqwest::Client::new();
    let request_builder = client.post("https://api.openai.com/v1/engines/text-davinci-002/completions")
        .headers(headers)
        .json(&body);

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