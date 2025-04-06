use reqwest::Client;
use serde::{Deserialize, Serialize};
// Removed unused std::env
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BraveSearchError {
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Unable to parse response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("No search results found")]
    NoResults,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BraveSearchResponse {
    pub web: BraveWebResults,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BraveWebResults {
    pub results: Vec<BraveWebResult>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BraveWebResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

pub async fn search(
    query: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<BraveWebResult>, BraveSearchError> {
    let url = format!("{}/res/v1/web/search", base_url);

    let client = Client::new();
    let response = client
        .get(&url) 
        .query(&[("q", query)])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key) 
        .send()
        .await?;

    // Check for non-success status codes before attempting to parse JSON
    if !response.status().is_success() {
        // Propagate the error status from the response
        return Err(BraveSearchError::Api(
            response.error_for_status().unwrap_err(),
        ));
    }

    let search_response: BraveSearchResponse = response.json().await?;

    if search_response.web.results.is_empty() {
        return Err(BraveSearchError::NoResults);
    }

    Ok(search_response.web.results)
}

pub fn format_search_results(results: &[BraveWebResult], query: &str) -> String {
    let mut formatted = format!("Search results for: \"{}\"\n\n", query);

    for (i, result) in results.iter().take(5).enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
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

    #[test]
    fn test_format_search_results_empty() {
        let results: Vec<BraveWebResult> = vec![];
        let query = "empty query";

        let expected_output = "Search results for: \"empty query\"\n\n";

        let actual_output = format_search_results(&results, query);

        assert_eq!(actual_output, expected_output);
    }

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

    // Helper function to create BraveWebResult, assuming Clone is needed or useful
    fn create_result(title: &str, url: &str, description: &str) -> BraveWebResult {
        BraveWebResult {
            title: title.to_string(),
            url: url.to_string(),
            description: description.to_string(),
        }
    }

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
