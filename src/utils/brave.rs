//! Utilities for interacting with the Brave Search API.
//! Requires the `brave_search` feature flag and `BRAVE_API_KEY` environment variable.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during Brave Search API interactions.
#[derive(Error, Debug)]
pub enum BraveSearchError {
    /// Error during HTTP request communication.
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    /// Error parsing the JSON response from the API.
    #[error("Unable to parse response: {0}")]
    Json(#[from] serde_json::Error),

    /// The API returned a successful response, but it contained no search results.
    #[error("No search results found")]
    NoResults,
}

/// Represents the overall structure of the Brave Search API response.
#[derive(Debug, Serialize, Deserialize)]
pub struct BraveSearchResponse {
    /// Contains the web search results.
    pub web: BraveWebResults,
}

/// Represents the web search results section of the response.
#[derive(Debug, Serialize, Deserialize)]
pub struct BraveWebResults {
    /// A list of individual web search results.
    pub results: Vec<BraveWebResult>,
}

/// Represents a single web search result item.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BraveWebResult {
    /// The title of the search result page.
    pub title: String,
    /// The URL of the search result page.
    pub url: String,
    /// A snippet or description of the search result page.
    pub description: String,
}

/// Performs a web search using the Brave Search API.
///
/// # Arguments
///
/// * `query` - The search query string.
/// * `base_url` - The base URL of the Brave Search API endpoint.
/// * `api_key` - The Brave Search API subscription token.
///
/// # Returns
///
/// A `Result` containing a `Vec<BraveWebResult>` on success, or a `BraveSearchError` on failure.
pub async fn search(
    query: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<BraveWebResult>, BraveSearchError> {
    // Construct the full API endpoint URL.
    let url = format!("{}/res/v1/web/search", base_url);

    // Create a new reqwest client.
    let client = Client::new();
    // Build and send the GET request with query parameters and necessary headers.
    let response = client
        .get(&url) 
        .query(&[("q", query)])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key) 
        .send()
        .await?;

    // Check for non-2xx status codes.
    if !response.status().is_success() {
        // Convert HTTP error status into our custom API error.
        return Err(BraveSearchError::Api(
            response.error_for_status().unwrap_err(),
        ));
    }

    // Parse the successful JSON response.
    let search_response: BraveSearchResponse = response.json().await?;

    // Check if the results list is empty.
    if search_response.web.results.is_empty() {
        return Err(BraveSearchError::NoResults);
    }

    // Return the vector of results.
    Ok(search_response.web.results)
}

/// Formats a slice of `BraveWebResult` into a human-readable string, limited to the first 5 results.
///
/// # Arguments
///
/// * `results` - A slice of search results.
/// * `query` - The original search query string.
///
/// # Returns
///
/// A formatted string suitable for display (e.g., in a Discord message).
pub fn format_search_results(results: &[BraveWebResult], query: &str) -> String {
    // Start with a header including the original query.
    let mut formatted = format!("Search results for: \"{}\"\n\n", query);

    // Iterate through the first 5 results (or fewer if less than 5).
    for (i, result) in results.iter().take(5).enumerate() {
        // Append formatted result (index, title, URL, description).
        formatted.push_str(&format!(
            "{}. {}\n   URL: {}\n   {}\n\n",
            i + 1,
            result.title,
            result.url,
            result.description
        ));
    }

    formatted
}

/// Module containing tests for the Brave Search utility functions.
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    /// Tests basic formatting of a couple of search results.
    #[test]
    fn test_format_search_results_basic() {
        let results = vec![
            BraveWebResult {
                title: "Result 1".to_string(),
                url: "https://example.com/1".to_string(),
                description: "Description for result 1.".to_string(),
            },
            BraveWebResult {
                title: "Result 2".to_string(),
                url: "https://example.com/2".to_string(),
                description: "Description for result 2.".to_string(),
            },
        ];
        let query = "test query";

        // Note: The expected string needs double backslashes for escapes within the JSON string literal
        let expected_output = "Search results for: \"test query\"\n\n1. Result 1\n   URL: https://example.com/1\n   Description for result 1.\n\n2. Result 2\n   URL: https://example.com/2\n   Description for result 2.\n\n";

        let actual_output = format_search_results(&results, query);

        assert_eq!(actual_output, expected_output);
    }

    /// Tests formatting when the input result list is empty.
    #[test]
    fn test_format_search_results_empty() {
        let results: Vec<BraveWebResult> = vec![];
        let query = "empty query";

        let expected_output = "Search results for: \"empty query\"\n\n";

        let actual_output = format_search_results(&results, query);

        assert_eq!(actual_output, expected_output);
    }

    /// Tests that formatting correctly limits the output to the first 5 results.
    #[test]
    fn test_format_search_results_limit_5() {
        let results = (0..7)
            .map(|i| BraveWebResult {
                title: format!("Result {}", i + 1),
                url: format!("https://example.com/{}", i + 1),
                description: format!("Description for result {}.", i + 1),
            })
            .collect::<Vec<_>>();
        let query = "limit query";

        let expected_output = "Search results for: \"limit query\"\n\n1. Result 1\n   URL: https://example.com/1\n   Description for result 1.\n\n2. Result 2\n   URL: https://example.com/2\n   Description for result 2.\n\n3. Result 3\n   URL: https://example.com/3\n   Description for result 3.\n\n4. Result 4\n   URL: https://example.com/4\n   Description for result 4.\n\n5. Result 5\n   URL: https://example.com/5\n   Description for result 5.\n\n";

        let actual_output = format_search_results(&results, query);

        assert_eq!(actual_output, expected_output);
    }

    /// Helper function to simplify creating `BraveWebResult` instances in tests.
    fn create_result(title: &str, url: &str, description: &str) -> BraveWebResult {
        BraveWebResult {
            title: title.to_string(),
            url: url.to_string(),
            description: description.to_string(),
        }
    }

    /// Tests the `search` function with a successful mock API response.
    #[tokio::test]
    async fn test_search_success() {
        // Arrange
        let server = MockServer::start().await;
        let base_url = server.uri();
        let api_key = "test-api-key-success";
        let query = "rust programming";

        let mock_response_results = vec![
            create_result(
                "The Rust Programming Language",
                "https://doc.rust-lang.org/book/",
                "Official Rust book.",
            ),
            create_result(
                "Rust Programming Language",
                "https://www.rust-lang.org/",
                "Official Rust website.",
            ),
        ];
        // Clone results for assertion comparison if needed
        let expected_results = mock_response_results
            .iter()
            .map(|r| create_result(&r.title, &r.url, &r.description))
            .collect::<Vec<_>>();

        let mock_response = BraveSearchResponse {
            web: BraveWebResults {
                results: mock_response_results,
            },
        };
        let response_body = serde_json::to_string(&mock_response).unwrap();

        Mock::given(method("GET"))
            .and(path("/res/v1/web/search"))
            .and(query_param("q", query))
            .and(header("Accept", "application/json"))
            .and(header("X-Subscription-Token", api_key))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        // Act
        let result = search(query, &base_url, api_key).await; // Pass api_key

        // Assert
        assert!(result.is_ok(), "Search failed: {:?}", result.err());
        let actual_results = result.unwrap();

        // Manual comparison (assuming PartialEq is not derived on BraveWebResult)
        assert_eq!(
            actual_results.len(),
            expected_results.len(),
            "Number of results mismatch"
        );
        for (i, (actual, expected)) in actual_results
            .iter()
            .zip(expected_results.iter())
            .enumerate()
        {
            assert_eq!(
                actual.title, expected.title,
                "Title mismatch at index {}",
                i
            );
            assert_eq!(actual.url, expected.url, "URL mismatch at index {}", i);
            assert_eq!(
                actual.description, expected.description,
                "Description mismatch at index {}",
                i
            );
        }
    }

    /// Tests the `search` function handling a non-2xx HTTP status code from the mock API.
    #[tokio::test]
    async fn test_search_api_error() {
        // Arrange
        let server = MockServer::start().await;
        let base_url = server.uri();
        let api_key = "test-api-key-error";
        let query = "error query";

        Mock::given(method("GET"))
            .and(path("/res/v1/web/search"))
            .and(query_param("q", query))
            .and(header("X-Subscription-Token", api_key))
            .respond_with(ResponseTemplate::new(500)) // Simulate server error
            .mount(&server)
            .await;

        // Act
        let result = search(query, &base_url, api_key).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            BraveSearchError::Api(e) => {
                assert!(e.is_status());
                assert_eq!(
                    e.status().unwrap(),
                    reqwest::StatusCode::INTERNAL_SERVER_ERROR
                );
            }
            e => panic!("Expected Api error, got {:?}", e),
        }
    }

    /// Tests the `search` function handling a successful API response that contains no results.
    #[tokio::test]
    async fn test_search_no_results() {
        // Arrange
        let server = MockServer::start().await;
        let base_url = server.uri();
        let api_key = "test-api-key-no-results";
        let query = "no results query";

        let mock_response = BraveSearchResponse {
            web: BraveWebResults { results: vec![] }, // Empty results
        };
        let response_body = serde_json::to_string(&mock_response).unwrap();

        Mock::given(method("GET"))
            .and(path("/res/v1/web/search"))
            .and(query_param("q", query))
            .and(header("Accept", "application/json"))
            .and(header("X-Subscription-Token", api_key))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        // Act
        let result = search(query, &base_url, api_key).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            BraveSearchError::NoResults => {} // Expected error
            e => panic!("Expected NoResults error, got {:?}", e),
        }
    }
}
