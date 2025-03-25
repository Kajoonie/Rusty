use dashmap::DashMap;
use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};
use ollama_rs::models::LocalModel;
use ollama_rs::Ollama;
use once_cell::sync::Lazy;
use serenity::all::User;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{debug, error, info};

use crate::database;

pub struct OllamaClient {
    client: Ollama,
    convo_map: DashMap<User, Mutex<Vec<ChatMessage>>>,
}

pub static OLLAMA_CLIENT: Lazy<Arc<OllamaClient>> = Lazy::new(|| {
    debug!("Initializing OllamaClient");
    Arc::new(OllamaClient::default())
});

impl OllamaClient {
    pub fn default() -> Self {
        debug!("Creating new OllamaClient instance");
        let client = Ollama::default();
        let convo_map = DashMap::new();
        Self { client, convo_map }
    }

    fn get_conversation_history(&self, user: &User) -> Vec<ChatMessage> {
        debug!("Retrieving conversation history for user {}", user.name);
        self.convo_map.entry(user.clone()).or_insert_with(|| {
            debug!(
                "Initializing new conversation history for user {}",
                user.name
            );
            Mutex::new(vec![])
        });

        let user_convo = self.convo_map.get(user).unwrap();
        let messages = user_convo.lock().unwrap();
        debug!(
            "Retrieved {} messages from history for user {}",
            messages.len(),
            user.name
        );
        messages.clone()
    }

    pub async fn chat(
        &self,
        user: &User,
        message: &str,
    ) -> Result<ChatMessageResponse, OllamaError> {
        info!("Processing chat request for user {}", user.name);
        let model = database::get_user_model(user);
        debug!("Using model '{}' for user {}", model, user.name);

        let mut chat_history = self.get_conversation_history(user);

        info!("Sending chat request to Ollama for user {}", user.name);
        match self
            .client
            .clone()
            .send_chat_messages_with_history(
                &mut chat_history,
                ChatMessageRequest::new(
                    model.to_string(),
                    vec![ChatMessage::user(message.to_string())],
                ),
            )
            .await
        {
            Ok(response) => {
                debug!(
                    "Successfully received response from Ollama for user {}",
                    user.name
                );
                Ok(response)
            }
            Err(e) => {
                error!(
                    "Failed to get response from Ollama for user {}: {}",
                    user.name, e
                );
                Err(e)
            }
        }
    }

    pub async fn list_models(&self) -> Result<Vec<LocalModel>, OllamaError> {
        info!("Fetching list of local models from Ollama");
        match self.client.list_local_models().await {
            Ok(models) => {
                debug!("Successfully retrieved {} local models", models.len());
                Ok(models)
            }
            Err(e) => {
                error!("Failed to fetch local models: {}", e);
                Err(e)
            }
        }
    }
}
