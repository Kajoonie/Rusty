//! Common test utilities, fixtures, and mocks
//! This module contains shared functionality used across different test categories

pub mod fixtures;
pub mod mocks;

use std::path::PathBuf;
use tokio::runtime::Runtime;

/// Creates a new tokio runtime for testing async functions
pub fn test_runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create test runtime")
}

/// Get the path to test fixtures directory
pub fn fixtures_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("common");
    path.push("fixtures");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let rt = test_runtime();
        assert!(rt.block_on(async { true }));
    }

    #[test]
    fn test_fixtures_path() {
        let path = fixtures_path();
        assert!(path.ends_with("tests/common/fixtures"));
    }
} 