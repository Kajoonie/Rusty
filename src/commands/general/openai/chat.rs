use std::{collections::HashMap, sync::Mutex};

use crate::{CommandResult, Context};
use once_cell::sync::Lazy;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::{json, Value};

use super::*;

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const MODEL: &str = "gpt-4";

static CONVO_MAP: Lazy<Mutex<HashMap<String, Vec<GptMessage>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[poise::command(slash_command, category = "General")]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your chat message"]
    #[rest]
    message: String,
) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();
    let openai_user = format!("{}{}", author.name, author.id);

    let new_message = GptMessage {
        role: "user".to_string(),
        content: message.clone(),
    };

    let convo = get_conversation_history(&openai_user, new_message);

    let response = send_request(&openai_user, convo).await?;

    let gpt_response = GptMessage {
        role: "assistant".to_string(),
        content: response.clone(),
    };
    add_response_to_conversation_history(&openai_user, gpt_response);

    let full_message = format!("**{}**: {message}\n\n**GPT**: {response}", author.name);

    chunk_response(ctx, full_message).await
}

async fn send_request(user_id: &str, convo: Vec<GptMessage>) -> Result<String, OpenAiError> {
    let body = json!({
        "model": MODEL,
        "max_tokens": 2048,
        "user": user_id,
        "messages": convo,
    });

    let request = OpenAiRequest::new(valid_json_path, error_json_path);
    request.send_request(ENDPOINT, body).await
}

fn valid_json_path(json: &Value) -> &Value {
    &json["choices"][0]["message"]["content"]
}

fn error_json_path(json: &Value) -> &Value {
    &json["error"]["message"]
}

#[derive(Debug, Clone)]
struct GptMessage {
    role: String,
    content: String,
}

impl Serialize for GptMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("GptMessage", 2)?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("content", &self.content)?;
        state.end()
    }
}

fn get_conversation_history(user: &str, new_message: GptMessage) -> Vec<GptMessage> {
    let mut map = CONVO_MAP.lock().unwrap();

    if map.get(user).is_none() {
        let prompt = GptMessage {
            role: "system".to_string(),
            content:
                "You are a discord bot assistant that answers questions and holds conversation"
                    .to_string(),
        };
        map.insert(user.to_string(), vec![prompt]);
    }

    let messages = map.get_mut(user).unwrap();
    add_message(messages, new_message);

    map.get(user).unwrap().to_owned().to_vec()
}

fn add_response_to_conversation_history(user: &str, new_message: GptMessage) {
    let mut map = CONVO_MAP.lock().unwrap();

    let messages = map.get_mut(user).unwrap();
    add_message(messages, new_message);
}

fn add_message(messages: &mut Vec<GptMessage>, new_message: GptMessage) {
    if messages.len() > 5 {
        messages.remove(1);
    }
    messages.push(new_message);
}
