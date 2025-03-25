use dashmap::DashMap;
use ollama_rs::models::LocalModel;
use ollama_rs::Ollama;
use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};
use serenity::all::User;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::database;

pub struct OllamaClient {
    client: Ollama,
    convo_map: DashMap<User, Mutex<Vec<ChatMessage>>>
}

pub static OLLAMA_CLIENT: Lazy<Arc<OllamaClient>> = Lazy::new(|| Arc::new(OllamaClient::default())); 

impl OllamaClient {
    pub fn default() -> Self {
        let client = Ollama::default();
        let convo_map = DashMap::new();
        Self { client, convo_map }
    }

    fn get_conversation_history(&self, user: &User) -> Vec<ChatMessage> {
        self.convo_map.entry(user.clone()).or_insert_with(|| {
            Mutex::new(vec![])
        });
    
        let user_convo = self.convo_map.get(user).unwrap();
        let messages = user_convo.lock().unwrap();
        messages.clone()
    }

    pub async fn chat(&self, user: &User, message: &str) -> Result<ChatMessageResponse, OllamaError> {
        let model = database::get_user_model(user);
        let mut chat_history = self.get_conversation_history(user);
        
        self.client.clone().send_chat_messages_with_history(
            &mut chat_history,
            ChatMessageRequest::new(
                model.to_string(),
                vec![ChatMessage::user(message.to_string())],
            )
        )
        .await
    }

    pub async fn list_models(&self) -> Result<Vec<LocalModel>, OllamaError> {
        self.client.list_local_models().await
    }
}