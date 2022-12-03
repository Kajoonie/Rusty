use reqwest::{header, header::HeaderMap};
use serde_json::{json, Error, Value};
use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

use crate::openai_api_key;

#[command]
#[description = "Generate an image with OpenAI's DALL-E"]
async fn imgen(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let img = if let Some(question) = args.remains() {
        send_request(question).await
    } else {
        send_request("Me and my robot friend holding hands").await
    };

    if let Some(img) = img {
        let result = msg.reply(&ctx.http, img).await;

        if let Err(why) = result {
            println!("Unable to send message: {why:?}");
        }
    }

    Ok(())
}

fn build_api_auth_header() -> HeaderMap {
    let api_auth = ["Bearer ", openai_api_key().as_str()].concat();

    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, api_auth.parse().unwrap());

    headers
}

fn build_request_body(prompt: &str) -> Value {
    json!({
        "prompt": format!("{prompt}"),
        "n": 1,
        "size": "1024x1024"
    })
}

async fn send_request(question: &str) -> Option<String> {
    let body = build_request_body(question);

    let client = reqwest::Client::new();
    let request_builder = client
        .post("https://api.openai.com/v1/images/generations")
        .headers(build_api_auth_header())
        .json(&body);

    if let Ok(response) = request_builder.send().await {
        if let Ok(text) = response.text().await {
            let result: Result<Value, Error> = serde_json::from_str(&text);

            return match result {
                Ok(json) => {
                    let json_str = json["data"][0]["url"].as_str().to_owned();
                    json_str.map(String::from)
                }
                _ => None,
            };
        }
    }

    None
}
