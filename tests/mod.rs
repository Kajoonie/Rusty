//! Test suite for the Rusty Discord bot
//! This module contains all test configurations and common utilities

pub mod common;
pub mod integration;
pub mod unit;

// Re-export commonly used testing utilities
pub use assert_matches::assert_matches;
pub use pretty_assertions::{assert_eq, assert_ne};
pub use rstest::*;
pub use test_case::test_case;
pub use wiremock::{Mock, MockServer, ResponseTemplate};

/// Common test setup and utilities
pub mod test_utils {
    use std::sync::Once;
    use tracing::Level;

    static INIT: Once = Once::new();

    /// Initialize test environment
    pub fn init() {
        INIT.call_once(|| {
            // Initialize tracing for tests
            tracing_subscriber::fmt()
                .with_max_level(Level::DEBUG)
                .with_test_writer()
                .init();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_setup() {
        test_utils::init();
        // This test verifies that the test environment can be initialized
        assert!(true);
    }
} 