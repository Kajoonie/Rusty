pub(crate) mod chat;
pub(crate) mod imgen;
pub(crate) mod question;
pub(crate) mod list_models;

use crate::CommandResult;
use crate::Context;
use reqwest::header::{self, HeaderMap};
use serde_json::Value;
use thiserror::Error;

use crate::openai_api_key;

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
