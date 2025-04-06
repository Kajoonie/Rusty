//! This module aggregates various utility submodules used throughout the application.

/// Utilities for interacting with the Brave Search API.
pub(crate) mod brave;
/// Utilities for interacting with the application's SQLite database.
pub(crate) mod database;
/// Utilities for interacting with an Ollama client/server.
pub(crate) mod ollama_client;
