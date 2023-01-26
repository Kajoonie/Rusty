use serde_json::{json, Value};

use crate::{CommandResult, Context};

use super::*;

const ENDPOINT: &str = "https://api.openai.com/v1/engines/text-davinci-003/completions";

#[poise::command(slash_command, category = "General")]
pub async fn question(
    ctx: Context<'_>,
    #[description = "Your Question"]
    #[rest]
    question: String,
) -> CommandResult {
    ctx.defer().await?;

    let answer = send_request(&question).await?;
    ctx.say([question, answer].concat()).await?;

    Ok(())
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
