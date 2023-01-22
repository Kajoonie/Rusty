use reqwest::{header, header::HeaderMap};
use serde_json::{json, Error, Value};

use crate::{openai_api_key, CommandResult, Context};

#[poise::command(
    slash_command,
    prefix_command,
    aliases("q", "Rusty,", "Hey Rusty,"),
    category = "General"
)]
pub async fn question(
    ctx: Context<'_>,
    #[description = "question"]
    #[rest]
    question: String,
) -> CommandResult {
    ctx.defer().await?;
    let answer = send_request(&question).await;
    if let Some(answer_text) = answer {
        let result = ctx.say(answer_text).await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
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
