use reqwest::header::{self, HeaderMap};
use serde_json::Value;
use thiserror::Error;

use crate::openai_api_key;

#[derive(Error, Debug)]
pub enum OpenAiError {
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Unable to parse text from JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Refused to complete request: {0}")]
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

    fn build_api_auth_header() -> HeaderMap {
        let api_auth = ["Bearer ", &openai_api_key()].concat();

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
