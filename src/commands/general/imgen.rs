use std::borrow::Cow;

use poise::serenity_prelude::AttachmentType;
use rusty::OpenAiRequest;
use serde_json::{json, Value};

use crate::{Context, CommandResult};

const ENDPOINT: &str = "https://api.openai.com/v1/images/generations";

#[poise::command(slash_command, category = "General")]
pub async fn imgen(
    ctx: Context<'_>,
    #[description = "request"]
    #[rest]
    request: String,
) -> CommandResult {
    ctx.defer().await?;

    let body = json!({
        "prompt": format!("{request}"),
        "n": 1,
        "size": "1024x1024",
        "response_format": "b64_json",
    });

    let request = OpenAiRequest::new(valid_json_path, error_json_path);
    let response = request.send_request(ENDPOINT, body).await;

    match response {
        Ok(b64) => {
            if let Ok(decoded_b64) = base64::decode(b64) {
                let result = ctx
                    .send(|m| {
                        m.attachment(AttachmentType::Bytes {
                            data: Cow::from(&decoded_b64[..]),
                            filename: "image.png".into(),
                        })
                    })
                    .await;

                if let Err(why) = result {
                    println!("Unable to send message: {why:?}");
                }
            }
        }
        Err(e) => {
            let result = ctx.say(format!("{e:?}")).await;

            if let Err(why) = result {
                println!("Unable to send message {why:?}");
            }
        }
    }

    Ok(())
}

fn valid_json_path(json: &Value) -> &Value {
    &json["data"][0]["b64_json"]
}

fn error_json_path(json: &Value) -> &Value {
    &json["error"]["message"]
}
