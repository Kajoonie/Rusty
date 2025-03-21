use std::sync::Mutex;
use dashmap::DashMap;
use once_cell::sync::Lazy;

use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};

use super::*;

const MODEL: &str = "llama3.1:8b";

static CONVO_MAP: Lazy<DashMap<String, Mutex<Vec<ChatMessage>>>> = Lazy::new(DashMap::new);

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

    let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);

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
    CONVO_MAP.entry(user.to_string()).or_insert_with(|| {
        Mutex::new(vec![])
    });

    let user_convo = CONVO_MAP.get(user).unwrap();
    let messages = user_convo.lock().unwrap();
    messages.clone()
}