pub(crate) mod chat;
pub(crate) mod list_models;
pub(crate) mod set_model;
pub(crate) mod search;
pub(crate) mod get_model;

use crate::CommandResult;
use crate::Context;
use crate::brave;
use crate::database::{self, UserPreference};

use std::sync::Mutex;
use std::sync::Arc;
use dashmap::DashMap;
use once_cell::sync::Lazy;

use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};

static OLLAMA_INSTANCE: Lazy<Arc<Ollama>> = Lazy::new(|| Arc::new(Ollama::default()));

fn get_ollama() -> Arc<Ollama> {
    OLLAMA_INSTANCE.clone()
}

const MAX_MESSAGE_LENGTH: usize = 2000;

pub async fn chunk_response<S: AsRef<str>>(ctx: Context<'_>, response: S) -> CommandResult {
    let response = response.as_ref();
    let mut iter = response.chars();
    let mut pos = 0;
    while pos < response.len() {
        let mut len = 0;
        for ch in iter.by_ref().take(MAX_MESSAGE_LENGTH) {
            len += ch.len_utf8();
        }
        ctx.say(&response[pos..pos + len]).await?;
        pos += len;
    }

    Ok(())
}

static CONVO_MAP: Lazy<DashMap<String, Mutex<Vec<ChatMessage>>>> = Lazy::new(DashMap::new);

fn get_conversation_history(user: &str) -> Vec<ChatMessage> {
    CONVO_MAP.entry(user.to_string()).or_insert_with(|| {
        Mutex::new(vec![])
    });

    let user_convo = CONVO_MAP.get(user).unwrap();
    let messages = user_convo.lock().unwrap();
    messages.clone()
}

async fn send_request_with_model(user_message: String, mut chat_history: Vec<ChatMessage>, model: &str) -> Result<ChatMessageResponse, OllamaError> {
    let mut ollama = (*get_ollama()).clone();

    ollama.send_chat_messages_with_history(
            &mut chat_history,
            ChatMessageRequest::new(
                model.to_string(),
                vec![ChatMessage::user(user_message)],
            )
        )
        .await
}