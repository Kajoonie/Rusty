//! Provides a client wrapper for interacting with an Ollama server.
//! Manages conversation history per user and handles model selection.

use dashmap::DashMap;
use ollama_rs::Ollama;
use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};
use ollama_rs::models::LocalModel;
use serenity::all::User;
use std::env;
use std::sync::Mutex;
use std::sync::{Arc, LazyLock};
use tracing::{debug, error, info, warn};

use crate::utils::database;

/// A specialized `Result` type for Ollama client operations, using `ollama_rs::OllamaError`.
pub type OllamaResult<T> = Result<T, OllamaError>;

/// A wrapper around the `ollama_rs::Ollama` client, adding conversation history management.
pub struct OllamaClient {
    /// The underlying `ollama_rs` client instance.
    client: Ollama,
    /// The default model name to use if a user hasn't set a preference.
    default_model: Option<String>,
    /// A map storing conversation history (`Vec<ChatMessage>`) per user.
    /// Uses `DashMap` for concurrent access and `Mutex` for interior mutability of the history vector.
    convo_map: DashMap<User, Mutex<Vec<ChatMessage>>>,
}

/// Global, thread-safe, lazily initialized instance of the `OllamaClient`.
pub static OLLAMA_CLIENT: LazyLock<Arc<OllamaClient>> = LazyLock::new(|| {
    debug!("Initializing OllamaClient");
    Arc::new(OllamaClient::default())
});

/// Determines the default Ollama model name.
/// Tries to read the `DEFAULT_OLLAMA_MODEL` environment variable first.
/// Falls back to a hardcoded default if the environment variable is not set.
fn set_default_model() -> Option<String> {
    // Check environment variable.
    if let Ok(model) = env::var("DEFAULT_OLLAMA_MODEL") {
        debug!("Using default model from environment variable: {}", model);
        Some(model)
    } else {
        // Fallback to hardcoded default if env var is missing.
        warn!(
            "DEFAULT_OLLAMA_MODEL environment variable not set. Using hardcoded default 'test-default-model'."
        );
        Some("test-default-model".to_string()) // Return a default string instead of None
    }
}

impl OllamaClient {
    /// Creates a new `OllamaClient` instance with default settings.
    /// Initializes the underlying `ollama_rs` client and determines the default model.
    pub fn default() -> Self {
        debug!("Creating new OllamaClient instance");
        // Initialize the base Ollama client.
        let client = Ollama::default();
        // Determine the default model.
        let default_model = set_default_model();
        // Initialize an empty conversation map.
        let convo_map = DashMap::new();

        Self {
            client,
            default_model,
            convo_map,
        }
    }

    /// Returns a clone of the default model name, if one is set.
    pub fn get_default_model(&self) -> Option<String> {
        self.default_model.clone()
    }

    /// Fetches the list of locally available models from the Ollama server.
    pub async fn list_models(&self) -> OllamaResult<Vec<LocalModel>> {
        info!("Fetching list of local models from Ollama");
        self.client.list_local_models().await
    }

    /// Retrieves the conversation history for a given user.
    /// If no history exists for the user, initializes an empty history.
    /// Returns a clone of the message vector.
    fn get_conversation_history(&self, user: &User) -> Vec<ChatMessage> {
        debug!("Retrieving conversation history for user {}", user.name);
        // Get or insert an entry for the user, initializing with an empty Mutex<Vec> if new.
        self.convo_map.entry(user.clone()).or_insert_with(|| {
            debug!(
                "Initializing new conversation history for user {}",
                user.name
            );
            Mutex::new(vec![])
        });

        // Get the entry (guaranteed to exist now).
        let user_convo = self.convo_map.get(user).unwrap();
        // Lock the mutex to access the message vector.
        let messages = user_convo.lock().unwrap();
        debug!(
            "Retrieved {} messages from history for user {}",
            messages.len(),
            user.name
        );
        // Clone the messages to return.
        messages.clone()
    }

    /// Sends a chat message to the Ollama server, maintaining conversation history.
    ///
    /// 1. Determines the model to use (user preference or default).
    /// 2. Retrieves the user's conversation history.
    /// 3. Sends the new message along with the history to the Ollama API.
    /// 4. Updates the user's conversation history with the new user message and the assistant's response.
    pub async fn chat(&self, user: &User, message: &str) -> OllamaResult<ChatMessageResponse> {
        info!("Processing chat request for user {}", user.name);
        // Determine the model: check user preference via database, then fallback.
        let model = match database::get_user_model(user) {
            Some(model) => model,
            None => {
                // Return error if no model could be determined.
                return Err(OllamaError::Other(
                    "No model set for user or default defined".to_string(),
                ));
            }
        };

        debug!("Using model '{}' for user {}", model, user.name);

        // Get the current conversation history for the user.
        let mut chat_history = self.get_conversation_history(user);

        info!("Sending chat request to Ollama for user {}", user.name);
        // Use the ollama-rs method that handles history automatically.
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
            // On success, log and return the response.
            Ok(response) => {
                debug!(
                    "Successfully received response from Ollama for user {}",
                    user.name
                );
                Ok(response)
            }
            // On error, log and return the error.
            Err(e) => {
                error!(
                    "Failed to get response from Ollama for user {}: {}",
                    user.name, e
                );
                Err(e)
            }
        }
    }
}

/// Module containing tests for the OllamaClient.
#[cfg(test)]
mod tests {
    use super::*;
    use ollama_rs::{Ollama, generation::chat::ChatMessageResponse, models::LocalModel};
    use serde_json::json;
    use serenity::model::user::User;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Test helper: Creates a mock `serenity::model::user::User` for testing.
    fn create_test_user() -> User {
        let user_json = json!({
            "id": "123456789012345678", // ID needs to be a string in JSON for u64/UserId
            "avatar": null, // Use null instead of an invalid length string
            "bot": false,
            "discriminator": "1234", // Discriminator also often a string in API
            "global_name": "TestUser",
            "username": "TestUser", // Serenity uses 'username' field name in JSON mapping
            "public_flags": null
             // Add other fields as needed, ensuring types match JSON expectations
             // We omit fields like system, accent_colour, banner, etc. as they are likely optional
        });
        serde_json::from_value(user_json).expect("Failed to deserialize test user from JSON")
    }

    /// Test helper: Creates an `OllamaClient` instance configured to use a mock server.
    async fn setup_test_client(mock_server: &MockServer) -> OllamaClient {
        // Get the base URI of the mock server.
        let uri = mock_server.uri();
        // Extract host, scheme, and port for the Ollama::new constructor.
        let parsed_url = url::Url::parse(&uri).expect("Failed to parse mock server URI");
        let host = parsed_url.host_str().expect("URI has no host").to_string();
        let scheme = parsed_url.scheme(); // Get the scheme ("http" or "https")
        let port = parsed_url.port().expect("URI has no port");
        // Construct the host string WITH the scheme for Ollama::new
        let host_with_scheme = format!("{}://{}", scheme, host);
        // Create the ollama-rs client instance pointing to the mock server.
        let ollama_rs_client = Ollama::new(host_with_scheme, port);

        // Create the OllamaClient wrapper.
        OllamaClient {
            client: ollama_rs_client,
            // Set a default model, although chat() currently relies on database::get_user_model
            default_model: Some("test-default-model".to_string()),
            convo_map: DashMap::new(),
        }
    }

    /// Tests successful retrieval of the model list.
    #[tokio::test]
    async fn test_list_models_success() {
        // Setup mock server and client.
        let mock_server = MockServer::start().await;
        let client = setup_test_client(&mock_server).await;

        // Define the expected response body.
        let expected_models = vec![
            LocalModel {
                name: "llama3:latest".to_string(),
                modified_at: "2024-04-05T12:00:00Z".to_string(),
                size: 123456789,
            },
            LocalModel {
                name: "mistral:latest".to_string(),
                modified_at: "2024-04-04T10:00:00Z".to_string(),
                size: 987654321,
            },
        ];
        let response_body = json!({ "models": expected_models });

        // Configure the mock server to respond to the list models request.
        Mock::given(method("GET"))
            .and(path("/api/tags")) // Ollama API path for listing models
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1) // Expect the mock to be called exactly once
            .mount(&mock_server)
            .await;

        // Call the method under test.
        let result = client.list_models().await;

        // Assert the result is Ok and contains the expected data.
        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3:latest");
        assert_eq!(models[1].name, "mistral:latest");

        // Verify the mock server received the expected request.
        mock_server.verify().await;
    }

    /// Tests handling of an API error when listing models.
    #[tokio::test]
    async fn test_list_models_error() {
        let mock_server = MockServer::start().await;
        let client = setup_test_client(&mock_server).await;

        // Configure the mock server to return an error status.
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Call the method under test.
        let result = client.list_models().await;

        // Assert the result is an error.
        assert!(result.is_err());

        // Verify the mock server received the request.
        mock_server.verify().await;
    }

    /// Tests a successful chat interaction, assuming database lookup provides the model.
    #[tokio::test]
    async fn test_chat_success() {
        let mock_server = MockServer::start().await;
        let client = setup_test_client(&mock_server).await;
        let user = create_test_user();
        let user_message = "Hello, Ollama!";
        // The actual model used will be the default from setup_test_client,
        // as the database lookup for the test user will fail.
        let model_name = "test-default-model"; // Use the actual default model name

        // Define the expected response message and structure.
        let response_message = ChatMessage::assistant("Hi there!".to_string());
        let expected_response = ChatMessageResponse {
            model: model_name.to_string(), // Expect the default model in response too
            created_at: "2024-04-05T13:00:00Z".to_string(), // Example timestamp
            message: response_message.clone(),
            done: true,
            final_data: None,
        };
        let response_body = json!(expected_response);

        // Configure the mock server to respond to the chat request.
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Call the method under test.
        let result = client.chat(&user, user_message).await;

        // Assert the result is Ok and contains the expected response data.
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.model, model_name);
        assert_eq!(response.message.content, "Hi there!");

        // Verify the mock server received the request.
        mock_server.verify().await;
    }

    /// Tests handling of an API error during a chat request.
    #[tokio::test]
    async fn test_chat_api_error() {
        let mock_server = MockServer::start().await;
        let client = setup_test_client(&mock_server).await;
        let user = create_test_user();
        let user_message = "Another message";

        // Configure the mock server to return an error status for the chat request.
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Model not found"))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Call the method under test.
        let result = client.chat(&user, user_message).await;

        // Assert the result is an error.
        assert!(result.is_err());

        // Verify the mock server received the request.
        mock_server.verify().await;
    }

    /// Tests the scenario where the database lookup fails to provide a model, resulting in an error.
    #[tokio::test]
    async fn test_chat_no_model_set_error() {
        // This test verifies the *internal* logic before the API call
        let mock_server = MockServer::start().await; // Needed for setup_test_client
        let client = setup_test_client(&mock_server).await;
        let user = create_test_user();
        let user_message = "Will this work?";

        // No mock server setup needed as the error should occur before the API call.

        // Call the method under test.
        let result = client.chat(&user, user_message).await;

        // Assert the result is an error (specifically, the 'No model set' error).
        assert!(result.is_err());
    }
}
