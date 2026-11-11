//! This module aggregates various utility submodules used throughout the application.

/// Client for the Claude API, used by `/ask` (requires the `tts` feature).
#[cfg(feature = "tts")]
pub(crate) mod anthropic_client;
/// Utilities for interacting with the Brave Search API.
pub(crate) mod brave;
/// Utilities for interacting with the application's SQLite database.
pub(crate) mod database;
/// Utilities for interacting with an Ollama client/server.
pub(crate) mod ollama_client;
