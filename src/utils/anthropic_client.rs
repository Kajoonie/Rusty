//! A small client for the Claude Messages API (Anthropic), used by `/ask`.
//!
//! There is no official Rust SDK, so this sends raw HTTP with the bot's shared `reqwest`
//! client. Every request is a single stateless question: one system prompt, one user
//! message, one answer. The API key is only ever sent in the `x-api-key` header and is
//! never logged or put into an error message.

use std::env;
use std::sync::LazyLock;
use std::time::Duration;

use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Environment variable holding the Anthropic API key. `/ask` is unavailable without it.
pub const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Environment variable overriding the model.
pub const MODEL_ENV: &str = "ANTHROPIC_MODEL";

/// Where the Claude API lives. Tests point the client at a mock server instead.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// The model used when `ANTHROPIC_MODEL` is not set.
///
/// The request below is written for this model: it relies on `output_config.effort`,
/// always-on adaptive thinking (no `thinking` field is sent), server-side fallbacks and a
/// 512-token minimum for prompt caching. Another model may reject or ignore some of that.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// Value of the `anthropic-version` header.
const API_VERSION: &str = "2026-10-21";
/// Beta that enables the `fallbacks` request field.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// `fallbacks` mode: on a safety-classifier refusal the API re-runs the request on the
/// model Anthropic recommends for that kind of refusal, inside the same call.
const FALLBACKS: &str = "default";

/// Upper bound on generated tokens. Thinking counts toward it, so it is far above the
/// length of an answer; the answer's length is controlled by the system prompt.
const MAX_TOKENS: u32 = 8_000;
/// How hard the model thinks before answering (`low`, `medium`, `high`, ...).
///
/// `medium` is this model's own default. Thinking is adaptive, so a plain question still
/// gets little of it, while composing a song in DECtalk's phoneme notation (melody as
/// note numbers, lyrics as phonemes, durations that fit the limits) gets the reasoning it
/// needs. `low` would answer faster but made-up notes cannot be fixed once spoken.
const EFFORT: &str = "medium";
/// Longest a request may take, including reading the response.
///
/// A long song with thinking takes a while to generate; Discord keeps a deferred reply
/// open for 15 minutes, so this is a limit on patience, not a protocol one.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How much of an API error message is kept for the log.
const ERROR_LOG_LIMIT: usize = 300;

/// Represents the ways a question to Claude can fail.
///
/// The messages are shown to Discord users, so they contain no key, URL or response
/// body. Details are logged where the error is created.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ClaudeError {
    /// `ANTHROPIC_API_KEY` is not configured.
    #[error("`/ask` is unavailable: `ANTHROPIC_API_KEY` is not set on the bot's server.")]
    MissingKey,

    /// The API rejected the key (HTTP 401), or the key cannot be sent at all.
    #[error("Claude did not accept the bot's API key. The bot's owner needs to check it.")]
    Unauthorized,

    /// Billing or permission problem on the account (HTTP 402 or 403).
    #[error("The bot's Anthropic account cannot make that request (billing or permissions).")]
    AccountProblem,

    /// The model does not exist or is not available to the account (HTTP 404).
    #[error(
        "The configured Claude model is not available. The bot's owner needs to check `ANTHROPIC_MODEL`."
    )]
    ModelNotFound,

    /// Too many requests (HTTP 429).
    #[error("Claude is rate limited right now. Try again in a moment.")]
    RateLimited,

    /// The API is overloaded or failing (HTTP 5xx, including 529).
    #[error("Claude is overloaded or having problems right now. Try again in a moment.")]
    Unavailable,

    /// Any other error status, such as a 400 for a request the model does not accept.
    #[error("Claude rejected the request (HTTP {0}).")]
    Rejected(u16),

    /// No complete response within the time limit.
    #[error("Claude took longer than {} seconds to answer.", .0.as_secs())]
    Timeout(Duration),

    /// The API could not be reached.
    #[error("I could not reach Claude.")]
    Network,

    /// The response was not the JSON that was expected.
    #[error("Claude sent a response I could not read.")]
    InvalidResponse,

    /// The model, a safety classifier, or every fallback model declined to answer.
    #[error("Claude declined to answer that.")]
    Refused,

    /// The answer hit the token limit before it was finished.
    #[error("Claude's answer was cut off before it was finished. Try a simpler question.")]
    Truncated,

    /// The response contained no text.
    #[error("Claude gave an empty answer.")]
    EmptyAnswer,
}

/// A specialized `Result` type for Claude API operations.
pub type ClaudeResult<T> = Result<T, ClaudeError>;

/// A client for single-question requests to the Claude Messages API.
///
/// Deliberately not `Debug`, so the key cannot end up in a log by accident.
pub struct AnthropicClient {
    /// The HTTP client requests are sent with.
    http: reqwest::Client,
    /// Scheme and host of the API, without a trailing slash.
    base_url: String,
    /// The API key, if one is configured.
    api_key: Option<String>,
    /// The model that is asked.
    model: String,
    /// Time limit for one request.
    timeout: Duration,
}

/// Global client, configured from the environment on first use.
static CLIENT: LazyLock<AnthropicClient> = LazyLock::new(AnthropicClient::from_env);

/// Returns the client configured from `ANTHROPIC_API_KEY` and `ANTHROPIC_MODEL`.
pub fn client() -> &'static AnthropicClient {
    &CLIENT
}

/// Logs whether `/ask` can reach Claude.
///
/// Like the DECtalk check this never panics: the bot starts without a key and `/ask`
/// replies with an error until one is set.
pub fn check_api_key() {
    let client = client();

    if client.has_api_key() {
        info!(
            "{} is set; /ask will use the model {}",
            API_KEY_ENV,
            client.model()
        );
    } else {
        warn!("{} is not set; /ask will reply with an error", API_KEY_ENV);
    }
}

/// Returns a configured value, treating an unset, empty or blank one as missing.
fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// A successful response from the Messages API. Fields that are not needed are ignored.
#[derive(Deserialize)]
struct MessageResponse {
    /// The model that produced the message; differs from the request after a fallback.
    #[serde(default)]
    model: Option<String>,
    /// Why generation stopped. Checked before `content` is read.
    #[serde(default)]
    stop_reason: Option<String>,
    /// The content blocks: thinking, text, and others.
    #[serde(default)]
    content: Vec<ContentBlock>,
    /// Token counts, for the log.
    #[serde(default)]
    usage: Option<Usage>,
}

/// One block of a response. Only `text` blocks carry the answer.
#[derive(Deserialize)]
struct ContentBlock {
    /// The block type, e.g. `thinking`, `text` or `fallback`.
    #[serde(rename = "type")]
    kind: String,
    /// The text of a `text` block.
    #[serde(default)]
    text: Option<String>,
}

/// Token counts of a response.
#[derive(Deserialize, Default)]
struct Usage {
    /// Input tokens that were neither read from nor written to the cache.
    #[serde(default)]
    input_tokens: u64,
    /// Input tokens written to the prompt cache.
    #[serde(default)]
    cache_creation_input_tokens: u64,
    /// Input tokens served from the prompt cache.
    #[serde(default)]
    cache_read_input_tokens: u64,
    /// Generated tokens, thinking included.
    #[serde(default)]
    output_tokens: u64,
}

/// The body of an error response.
#[derive(Deserialize, Default)]
struct ErrorResponse {
    /// What went wrong.
    #[serde(default)]
    error: Option<ErrorDetail>,
    /// Identifies the request to Anthropic's support.
    #[serde(default)]
    request_id: Option<String>,
}

/// The `error` object of an error response.
#[derive(Deserialize, Default)]
struct ErrorDetail {
    /// The error type, e.g. `rate_limit_error`.
    #[serde(rename = "type", default)]
    kind: Option<String>,
    /// A description of the problem.
    #[serde(default)]
    message: Option<String>,
}

/// Maps an error status to what the user is told.
fn error_for_status(status: u16) -> ClaudeError {
    match status {
        401 => ClaudeError::Unauthorized,
        402 | 403 => ClaudeError::AccountProblem,
        404 => ClaudeError::ModelNotFound,
        429 => ClaudeError::RateLimited,
        500..=599 => ClaudeError::Unavailable,
        other => ClaudeError::Rejected(other),
    }
}

/// Joins the `text` blocks of a response, skipping thinking and any other block type.
fn answer_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text.as_deref())
        .collect()
}

impl AnthropicClient {
    /// Creates a client.
    ///
    /// * `base_url` - scheme and host of the API, e.g. `DEFAULT_BASE_URL` or a mock server.
    /// * `api_key` - the key, or `None` if `/ask` is not configured.
    /// * `model` - the model ID to ask.
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: non_empty(api_key),
            model: model.into(),
            timeout: REQUEST_TIMEOUT,
        }
    }

    /// Creates the client from `ANTHROPIC_API_KEY` and `ANTHROPIC_MODEL`.
    pub fn from_env() -> Self {
        Self::new(
            crate::HTTP_CLIENT.clone(),
            DEFAULT_BASE_URL,
            env::var(API_KEY_ENV).ok(),
            non_empty(env::var(MODEL_ENV).ok()).unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        )
    }

    /// Whether an API key is configured.
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// The model that is asked.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Builds the request body for one question.
    ///
    /// The system prompt is the same on every call, so it is marked for prompt caching;
    /// the question follows it and is never cached. Nothing here depends on the model:
    /// no `thinking`, sampling parameters, prefill or tools are sent.
    fn request_body(&self, system: &str, user_message: &str) -> Value {
        json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": [{
                "type": "text",
                "text": system,
                "cache_control": { "type": "ephemeral" },
            }],
            "output_config": { "effort": EFFORT },
            "fallbacks": FALLBACKS,
            "messages": [{ "role": "user", "content": user_message }],
        })
    }

    /// Sends one question and returns the text of the answer.
    ///
    /// * `system` - the system prompt.
    /// * `user_message` - the question.
    pub async fn ask(&self, system: &str, user_message: &str) -> ClaudeResult<String> {
        let api_key = self.api_key.as_deref().ok_or(ClaudeError::MissingKey)?;

        let mut key_header = HeaderValue::from_str(api_key).map_err(|_| {
            warn!(
                "{} contains characters that cannot be sent in a header",
                API_KEY_ENV
            );
            ClaudeError::Unauthorized
        })?;
        key_header.set_sensitive(true);

        let response = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", key_header)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .timeout(self.timeout)
            .json(&self.request_body(system, user_message))
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| self.transport_error(e))?;

        if !status.is_success() {
            let error: ErrorResponse = serde_json::from_slice(&body).unwrap_or_default();
            let detail = error.error.unwrap_or_default();
            let message: String = detail
                .message
                .unwrap_or_default()
                .chars()
                .take(ERROR_LOG_LIMIT)
                .collect();
            warn!(
                "Claude API error: status {}, type {}, request {}: {}",
                status.as_u16(),
                detail.kind.as_deref().unwrap_or("unknown"),
                error.request_id.as_deref().unwrap_or("unknown"),
                message
            );

            return Err(error_for_status(status.as_u16()));
        }

        let message: MessageResponse = serde_json::from_slice(&body).map_err(|e| {
            warn!("Claude API response could not be parsed: {}", e);
            ClaudeError::InvalidResponse
        })?;

        self.read_answer(message)
    }

    /// Turns a parsed response into the answer, checking why generation stopped first.
    fn read_answer(&self, message: MessageResponse) -> ClaudeResult<String> {
        let stop_reason = message.stop_reason.as_deref().unwrap_or("none");
        let usage = message.usage.unwrap_or_default();
        debug!(
            "Claude answered: stop reason {}, input tokens {} (cache read {}, cache write {}), output tokens {}",
            stop_reason,
            usage.input_tokens,
            usage.cache_read_input_tokens,
            usage.cache_creation_input_tokens,
            usage.output_tokens
        );

        if let Some(served_by) = message.model.as_deref().filter(|m| *m != self.model) {
            info!(
                "Claude request for {} was answered by {}",
                self.model, served_by
            );
        }

        match stop_reason {
            // A refusal carries no answer, or a partial one that must not be used.
            "refusal" => {
                info!("Claude declined a question (stop reason refusal)");
                return Err(ClaudeError::Refused);
            }
            "max_tokens" | "model_context_window_exceeded" => {
                warn!(
                    "Claude's answer was cut off (stop reason {}, {} output tokens)",
                    stop_reason, usage.output_tokens
                );
                return Err(ClaudeError::Truncated);
            }
            "end_turn" | "stop_sequence" => {}
            other => warn!("Unexpected Claude stop reason: {}", other),
        }

        let answer = answer_text(&message.content);
        if answer.trim().is_empty() {
            warn!("Claude returned no text (stop reason {})", stop_reason);
            return Err(ClaudeError::EmptyAnswer);
        }

        Ok(answer)
    }

    /// Maps a failure to send the request or read the response, and logs it.
    fn transport_error(&self, error: reqwest::Error) -> ClaudeError {
        if error.is_timeout() {
            warn!("Claude API request timed out after {:?}", self.timeout);
            ClaudeError::Timeout(self.timeout)
        } else {
            // `without_url` keeps the log line short; the key is never part of the error.
            warn!(
                "Claude API request failed (connect error: {}): {}",
                error.is_connect(),
                error.without_url()
            );
            ClaudeError::Network
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_KEY: &str = "test-key-123";

    /// An HTTP client that talks to the mock server directly, whatever proxy the
    /// environment configures.
    fn test_http() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    /// A client pointed at the mock server, with a key.
    fn test_client(server: &MockServer) -> AnthropicClient {
        AnthropicClient::new(
            test_http(),
            server.uri(),
            Some(TEST_KEY.to_string()),
            DEFAULT_MODEL,
        )
    }

    /// Mounts a response for `POST /v1/messages` that must be requested exactly once.
    async fn mount(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(response)
            .expect(1)
            .mount(server)
            .await;
    }

    /// A successful response body with the given stop reason and content blocks.
    fn message(stop_reason: &str, content: Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": DEFAULT_MODEL,
            "content": content,
            "stop_reason": stop_reason,
            "usage": { "input_tokens": 10, "output_tokens": 20 },
        }))
    }

    /// An error response body in the API's format.
    fn api_error(status: u16, kind: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_json(json!({
            "type": "error",
            "error": { "type": kind, "message": "details that stay in the log" },
            "request_id": "req_test",
        }))
    }

    #[tokio::test]
    async fn test_request_has_expected_headers_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", TEST_KEY))
            .and(header("anthropic-version", "2026-10-21"))
            .and(header("anthropic-beta", "server-side-fallback-2026-07-01"))
            .and(header("content-type", "application/json"))
            .respond_with(message(
                "end_turn",
                json!([{ "type": "text", "text": "hi" }]),
            ))
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client(&server);
        let answer = client.ask("the system prompt", "the question").await;
        assert_eq!(answer, Ok("hi".to_string()));

        // The body is exactly this: no `thinking`, sampling parameters, tools or prefill.
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body,
            json!({
                "model": "claude-opus-5-5",
                "max_tokens": MAX_TOKENS,
                "system": [{
                    "type": "text",
                    "text": "the system prompt",
                    "cache_control": { "type": "ephemeral" },
                }],
                "output_config": { "effort": EFFORT },
                "fallbacks": "default",
                "messages": [{ "role": "user", "content": "the question" }],
            })
        );
        assert!(["low", "medium"].contains(&EFFORT));
        assert!((4_000..=16_000).contains(&MAX_TOKENS));
    }

    #[tokio::test]
    async fn test_request_uses_the_configured_model() {
        let server = MockServer::start().await;
        mount(
            &server,
            message("end_turn", json!([{ "type": "text", "text": "hi" }])),
        )
        .await;

        let client = AnthropicClient::new(
            test_http(),
            // A trailing slash on the base URL is tolerated.
            format!("{}/", server.uri()),
            Some(TEST_KEY.to_string()),
            "claude-other-model",
        );
        assert_eq!(client.model(), "claude-other-model");
        assert_eq!(client.ask("s", "q").await, Ok("hi".to_string()));

        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["model"], "claude-other-model");
    }

    #[tokio::test]
    async fn test_answer_joins_text_blocks_and_skips_thinking() {
        let server = MockServer::start().await;
        mount(
            &server,
            message(
                "end_turn",
                json!([
                    { "type": "thinking", "thinking": "", "signature": "abc" },
                    { "type": "text", "text": "The sky is blue " },
                    { "type": "fallback", "from": { "model": "a" }, "to": { "model": "b" } },
                    { "type": "redacted_thinking", "data": "xyz" },
                    { "type": "text", "text": "because of scattering." },
                ]),
            ),
        )
        .await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Ok("The sky is blue because of scattering.".to_string())
        );
    }

    #[tokio::test]
    async fn test_refusal_is_an_error_even_with_partial_text() {
        let server = MockServer::start().await;
        mount(
            &server,
            ResponseTemplate::new(200).set_body_json(json!({
                "model": DEFAULT_MODEL,
                "content": [{ "type": "text", "text": "partial output to discard" }],
                "stop_reason": "refusal",
                "stop_details": { "category": "cyber" },
            })),
        )
        .await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Err(ClaudeError::Refused)
        );
    }

    #[tokio::test]
    async fn test_refusal_with_empty_content_is_an_error() {
        let server = MockServer::start().await;
        mount(&server, message("refusal", json!([]))).await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Err(ClaudeError::Refused)
        );
    }

    #[tokio::test]
    async fn test_max_tokens_stop_is_an_error() {
        let server = MockServer::start().await;
        mount(
            &server,
            message(
                "max_tokens",
                json!([{ "type": "text", "text": "an answer that stops mid" }]),
            ),
        )
        .await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Err(ClaudeError::Truncated)
        );
    }

    #[tokio::test]
    async fn test_empty_content_is_an_error() {
        for content in [
            json!([]),
            json!([{ "type": "thinking", "thinking": "", "signature": "abc" }]),
            json!([{ "type": "text", "text": "  \n " }]),
        ] {
            let server = MockServer::start().await;
            mount(&server, message("end_turn", content)).await;

            assert_eq!(
                test_client(&server).ask("s", "q").await,
                Err(ClaudeError::EmptyAnswer)
            );
        }
    }

    #[tokio::test]
    async fn test_error_statuses_map_to_short_messages() {
        for (status, kind, expected) in [
            (401, "authentication_error", ClaudeError::Unauthorized),
            (402, "billing_error", ClaudeError::AccountProblem),
            (403, "permission_error", ClaudeError::AccountProblem),
            (404, "not_found_error", ClaudeError::ModelNotFound),
            (429, "rate_limit_error", ClaudeError::RateLimited),
            (500, "api_error", ClaudeError::Unavailable),
            (529, "overloaded_error", ClaudeError::Unavailable),
            (400, "invalid_request_error", ClaudeError::Rejected(400)),
            (413, "request_too_large", ClaudeError::Rejected(413)),
        ] {
            let server = MockServer::start().await;
            mount(&server, api_error(status, kind)).await;

            let error = test_client(&server).ask("s", "q").await.unwrap_err();
            assert_eq!(error, expected, "status {status}");

            // Nothing from the request or the response body reaches the user.
            let shown = error.to_string();
            assert!(!shown.contains(TEST_KEY), "{shown}");
            assert!(!shown.contains("details that stay in the log"), "{shown}");
            assert!(!shown.contains("req_test"), "{shown}");
        }
    }

    #[tokio::test]
    async fn test_error_status_without_json_body_is_handled() {
        let server = MockServer::start().await;
        mount(
            &server,
            ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"),
        )
        .await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Err(ClaudeError::Unavailable)
        );
    }

    #[tokio::test]
    async fn test_unreadable_success_body_is_an_error() {
        let server = MockServer::start().await;
        mount(
            &server,
            ResponseTemplate::new(200).set_body_string("not json"),
        )
        .await;

        assert_eq!(
            test_client(&server).ask("s", "q").await,
            Err(ClaudeError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn test_slow_response_times_out() {
        let server = MockServer::start().await;
        mount(
            &server,
            message("end_turn", json!([{ "type": "text", "text": "late" }]))
                .set_delay(Duration::from_secs(30)),
        )
        .await;

        let mut client = test_client(&server);
        client.timeout = Duration::from_millis(100);

        let error = client.ask("s", "q").await.unwrap_err();
        assert_eq!(error, ClaudeError::Timeout(Duration::from_millis(100)));
        assert_eq!(
            ClaudeError::Timeout(REQUEST_TIMEOUT).to_string(),
            format!(
                "Claude took longer than {} seconds to answer.",
                REQUEST_TIMEOUT.as_secs()
            )
        );
    }

    #[tokio::test]
    async fn test_unreachable_server_is_a_network_error() {
        // Nothing listens on a port that was just released.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let uri = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let client =
            AnthropicClient::new(test_http(), uri, Some(TEST_KEY.to_string()), DEFAULT_MODEL);
        assert_eq!(client.ask("s", "q").await, Err(ClaudeError::Network));
    }

    #[tokio::test]
    async fn test_missing_key_fails_without_a_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        for key in [None, Some(String::new()), Some("   ".to_string())] {
            let client = AnthropicClient::new(test_http(), server.uri(), key, DEFAULT_MODEL);
            assert!(!client.has_api_key());

            let error = client.ask("s", "q").await.unwrap_err();
            assert_eq!(error, ClaudeError::MissingKey);
            assert!(error.to_string().contains("ANTHROPIC_API_KEY"));
        }
    }

    #[tokio::test]
    async fn test_key_that_cannot_be_a_header_fails_without_a_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let client = AnthropicClient::new(
            test_http(),
            server.uri(),
            Some("bad\nkey".to_string()),
            DEFAULT_MODEL,
        );
        assert_eq!(client.ask("s", "q").await, Err(ClaudeError::Unauthorized));
    }

    #[test]
    fn test_non_empty_treats_blank_values_as_missing() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(Some("  ".to_string())), None);
        assert_eq!(non_empty(Some(" x ".to_string())), Some("x".to_string()));
    }
}
