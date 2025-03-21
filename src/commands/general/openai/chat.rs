use std::{collections::HashMap, sync::Mutex};
use once_cell::sync::Lazy;

use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};

use super::*;

const MODEL: &str = "llama3.1:8b";

static CONVO_MAP: Lazy<Mutex<HashMap<String, Vec<ChatMessage>>>> =
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
    let author_str = format!("{}{}", author.name, author.id);

    let chat_history = get_conversation_history(&author_str);

    let response = send_request(message.clone(), chat_history).await?;

    let content = response.message.content;

    let full_message = format!("**{}**: {message}\n\n**GPT**: {content}", author.name);

    chunk_response(ctx, full_message).await
}

async fn send_request(user_message: String, mut chat_history: Vec<ChatMessage>) -> Result<ChatMessageResponse, OllamaError> {
    let mut ollama = Ollama::default();

    ollama.send_chat_messages_with_history(
            &mut chat_history,
            ChatMessageRequest::new(
                MODEL.to_string(),
                vec![ChatMessage::user(user_message)],
            )
        )
        .await
}

fn get_conversation_history(user: &str) -> Vec<ChatMessage> {
    let mut map = CONVO_MAP.lock().unwrap();

    if map.get(user).is_none() {
        map.insert(user.to_string(), vec![]);
    }

    map.get(user).unwrap().to_owned()
}