//! Test fixtures for the Rusty Discord bot
//! This module contains sample data and configurations used in tests

/// Sample Discord message content for testing
pub const SAMPLE_MESSAGE_CONTENT: &str = "!help";

/// Sample user ID for testing
pub const SAMPLE_USER_ID: u64 = 123456789;

/// Sample channel ID for testing
pub const SAMPLE_CHANNEL_ID: u64 = 987654321;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_data() {
        assert!(!SAMPLE_MESSAGE_CONTENT.is_empty());
        assert!(SAMPLE_USER_ID > 0);
        assert!(SAMPLE_CHANNEL_ID > 0);
    }
} 