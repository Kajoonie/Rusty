use std::env;

use reqwest::header::{self, HeaderMap};
use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpenAiError {
    #[error("API communication failure")]
    Api(#[from] reqwest::Error),

    #[error("Unable to parse text from JSON")]
    Json(#[from] serde_json::Error),

    #[error("Refused to complete request")]
    Refusal(String),

    #[error("Unknown response from OpenAI API")]
    Unknown,
}

pub struct OpenAiRequest {
    valid: fn(&Value) -> &Value,
    error: fn(&Value) -> &Value,
}

impl OpenAiRequest {
    pub fn new(valid: fn(&Value) -> &Value, error: fn(&Value) -> &Value) -> Self {
        Self { valid, error }
    }

    fn openai_api_key() -> String {
        env::var("OPENAI_API_KEY").expect("OpenAI API Key not specified")
    }

    fn build_api_auth_header() -> HeaderMap {
        let api_auth = ["Bearer ", Self::openai_api_key().as_str()].concat();

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, api_auth.parse().unwrap());

        headers
    }

    pub async fn send_request(&self, endpoint: &str, body: Value) -> Result<String, OpenAiError> {
        let client = reqwest::Client::new();
        let request_builder = client
            .post(endpoint)
            .headers(Self::build_api_auth_header())
            .json(&body);

        let response = request_builder.send().await.map_err(OpenAiError::Api)?;
        let text = response.text().await.map_err(OpenAiError::Api)?;
        let result: Value = serde_json::from_str(&text).map_err(OpenAiError::Json)?;

        match (self.valid)(&result) {
            Value::String(str_val) => Ok(str_val.to_owned()),
            _ => match (self.error)(&result) {
                Value::String(err_val) => Err(OpenAiError::Refusal(err_val.to_owned())),
                _ => Err(OpenAiError::Unknown),
            },
        }
    }
}
