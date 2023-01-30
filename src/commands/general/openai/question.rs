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

    let author = ctx.author();
    let user_name_and_id = format!("{}{}", author.name, author.id.0);
    let answer = send_request(&question, user_name_and_id).await?;

    let mut iter = answer.chars();
    let mut pos = 0;
    while pos < answer.len() {
        let mut len = 0;
        for ch in iter.by_ref().take(2000) {
            len += ch.len_utf8();
        }
        ctx.say(&answer[pos..pos + len]).await?;
        pos += len;
    }

    Ok(())
}

async fn send_request(question: &str, user_id: String) -> Result<String, OpenAiError> {
    let body = json!({
        "prompt": format!("{question}"),
        "max_tokens": 2048,
        "echo": true,
        "user": user_id,
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
