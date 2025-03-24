//! Mock implementations for external dependencies
//! This module contains mock objects used for testing

use std::future::Future;
use std::boxed::Box;

use mockall::predicate::*;
use mockall::*;
use futures::future;

/// Mock HTTP client for testing external API calls
#[automock]
pub trait HttpClient {
    fn get(&self, url: &str) -> impl Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>>;
    fn post(&self, url: &str, body: &str) -> impl Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>>;
}

/// Creates a mock HTTP client for testing
pub fn create_mock_http_client() -> MockHttpClient {
    MockHttpClient::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_http_client() {
        let mut mock = create_mock_http_client();
        
        mock.expect_get()
            .with(eq("https://test.com"))
            .times(1)
            .returning(|_| Box::pin(future::ready(Ok("test response".to_string()))));

        let result = mock.get("https://test.com").await.unwrap();
        assert_eq!(result, "test response");
    }
} 