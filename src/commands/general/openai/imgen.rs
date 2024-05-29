use std::borrow::Cow;

use base64::{engine::general_purpose, Engine as _};
use poise::{serenity_prelude::CreateAttachment, CreateReply};
use serde_json::{json, Value};

use crate::{CommandResult, Context};

use super::*;

const ENDPOINT: &str = "https://api.openai.com/v1/images/generations";

#[poise::command(slash_command, category = "General")]
pub async fn imgen(
    ctx: Context<'_>,
    #[description = "Image Description"]
    #[rest]
    image_description: String,
) -> CommandResult {
    ctx.defer().await?;

    let body = json!({
        "prompt": format!("{image_description}"),
        "n": 1,
        "size": "1024x1024",
        "response_format": "b64_json",
    });

    let request = OpenAiRequest::new(valid_json_path, error_json_path);
    let response = request.send_request(ENDPOINT, body).await?;

    if let Ok(decoded_b64) = general_purpose::STANDARD.decode(response) {
        let attachment = CreateAttachment::bytes(
            Cow::from(&decoded_b64[..]), 
            "image.png".to_string(),
        );
        
        let reply = CreateReply::default()
            .attachment(attachment);

        ctx.send(reply).await?;
    }

    Ok(())
}

fn valid_json_path(json: &Value) -> &Value {
    &json["data"][0]["b64_json"]
}

fn error_json_path(json: &Value) -> &Value {
    &json["error"]["message"]
}
