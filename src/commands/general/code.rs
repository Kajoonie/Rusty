use reqwest::{header, header::HeaderMap};
use serde_json::{json, Error, Value};
use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

use crate::openai_api_key;

const PROMPT: &str = r#"
        <|endoftext|>/* I start with a blank HTML page, and incrementally modify it via <script> injection. Written for Chrome. */
        /* Command: Add "Hello World", by adding an HTML DOM node */
        var helloWorld = document.createElement('div');
        helloWorld.innerHTML = 'Hello World';
        document.body.appendChild(helloWorld);
        /* Command: Clear the page. */
        while (document.body.firstChild) {
        document.body.removeChild(document.body.firstChild);
        }
    "#;

#[command]
#[description = "Generate code snippets with OpenAI"]
async fn code(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let request = format!(
        "{}\n\n/* Command: {} */\n\n",
        PROMPT,
        args.remains().unwrap_or("Make something up")
    );

    let code = send_request(&request).await;

    if let Some(code) = code {
        let result = msg.reply(&ctx.http, format!("```{code}```")).await;

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
        "max_tokens": 500,
    })
}

async fn send_request(request: &str) -> Option<String> {
    let body = build_request_body(request);

    let client = reqwest::Client::new();
    let request_builder = client
        .post("https://api.openai.com/v1/engines/code-davinci-002/completions")
        .headers(build_api_auth_header())
        .json(&body);

    if let Ok(response) = request_builder.send().await {
        if let Ok(text) = response.text().await {
            let result: Result<Value, Error> = serde_json::from_str(&text);

            return match result {
                Ok(json) => match &json["choices"][0]["text"] {
                    Value::String(code) => Some(code.to_owned()),
                    _ => None,
                },
                _ => None,
            };
        }
    }

    None
}
