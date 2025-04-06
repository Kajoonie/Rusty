//! Module providing functionality to interact with the CoinGecko API.
//! Includes command definitions, data structures, API request logic, and error handling.

use serde_json::Value;
use reqwest::Url;
use thiserror::Error;

/// Submodule defining the actual `/coin` command and its subcommands (e.g., `/coin price`).
pub(crate) mod coin;

/// Base URL for the CoinGecko API v3.
const API: &str = "https://api.coingecko.com/api/v3/";

/// Trait providing helper methods for parsing `serde_json::Value`.
trait JsonParse {
    /// Safely parses the value as an f64, returning 0.0 on failure or wrong type.
    fn f64(&self) -> f64;
    /// Safely parses the value as a String, returning a default string representation on failure.
    fn string(&self) -> String;
}

impl JsonParse for Value {
    /// Implementation of `f64` for `serde_json::Value`.
    fn f64(&self) -> f64 {
        match &self {
            Value::Number(x) => x.as_f64().unwrap(),
            _ => 0.0,
        }
    }

    /// Implementation of `string` for `serde_json::Value`.
    fn string(&self) -> String {
        match &self {
            Value::String(x) => x.to_owned(),
            _ => self.to_string(),
        }
    }
}

/// Represents market data for a cryptocurrency.
#[derive(Debug)]
struct MarketData {
    /// Current price in USD.
    price_usd: f64,
    /// Price change in USD over the last 24 hours.
    usd_change_24h: f64,
    /// Price change percentage over the last 24 hours.
    perc_change_24h: f64,
}

impl MarketData {
    /// Creates a `MarketData` instance from a `serde_json::Value`.
    /// Assumes the input JSON has the expected structure from the CoinGecko API.
    fn from_json(json: &Value) -> Option<Self> {
        // Access the nested 'market_data' object.
        let market_data = &json["market_data"];

        // Extract price and change values using the JsonParse trait helpers.
        let price_usd = market_data["current_price"]["usd"].f64();
        let usd_change_24h = market_data["price_change_24h_in_currency"]["usd"].f64();
        let perc_change_24h = market_data["price_change_percentage_24h_in_currency"]["usd"].f64();

        Some(Self {
            price_usd,
            usd_change_24h,
            perc_change_24h,
        })
    }
}

/// Represents overall information for a cryptocurrency.
#[derive(Debug)]
struct CoinInfo {
    /// Formatted name of the coin.
    name: String,
    /// URL to the coin's icon.
    icon: String,
    /// Market data associated with the coin.
    market_data: MarketData,
}

impl CoinInfo {
    /// Creates a `CoinInfo` instance from a `serde_json::Value`.
    /// Assumes the input JSON has the expected structure from the CoinGecko API.
    fn from_json(json: &Value) -> Option<Self> {
        Some(Self {
            // Format the coin ID from the API into a display name.
            name: from_api_format(&json["id"].string()),
            // Extract the small image URL.
            icon: json["image"]["small"].string(),
            // Parse the nested market data, propagating potential failure (using `?`).
            market_data: MarketData::from_json(json)?,
        })
    }
}

/// Converts a user-provided string (like a coin name) into the format expected by the CoinGecko API (lowercase, space-separated words joined by hyphens).
fn to_api_format(s: &str) -> String {
    // Convert to lowercase byte slice.
    s.as_bytes()
        .to_ascii_lowercase()
        // Convert back to char iterator.
        .iter()
        .map(|x| char::from(*x))
        // Collect into a String.
        .collect::<String>()
        // Split by whitespace to handle multi-word names.
        .split_whitespace()
        .collect::<Vec<&str>>()
        // Join words with hyphens.
        .join("-")
}

/// Converts a string from the CoinGecko API format (lowercase, hyphen-separated) back into a user-friendly format (capitalized words, space-separated).
fn from_api_format(s: &str) -> String {
    // Split the API string by hyphens.
    s.split('-')
        // Capitalize the first letter of each word.
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => word.to_string(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        // Collect capitalized words into a Vec<String>.
        .collect::<Vec<String>>()
        // Join words with spaces.
        .join(" ")
}

/// Custom error type for CoinGecko API interactions.
#[derive(Error, Debug)]
enum CoingeckoError {
    /// Error during HTTP request communication.
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    /// Error during JSON parsing.
    #[error("Unable to parse text from JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Error indicating a bad request, often due to invalid input or API-level errors.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Error indicating an unexpected or unparseable response from the API.
    #[error("Invalid response received from CoinGecko")]
    Invalid,
}

/// Sends an asynchronous GET request to a specified CoinGecko API endpoint.
///
/// # Arguments
///
/// * `base_url` - The base URL of the API (e.g., `API` constant).
/// * `path` - The specific API endpoint path (e.g., "coins/list").
/// * `query` - A slice of key-value pairs for URL query parameters.
///
/// # Returns
///
/// A `Result` containing the parsed `serde_json::Value` on success, or a `CoingeckoError` on failure.
async fn send_request(base_url: &str, path: &str, query: &[(&str, &str)]) -> Result<Value, CoingeckoError> {
    // Create a new reqwest client for each request (consider reusing if performance is critical).
    let client = reqwest::Client::new();
    // Safely parse the base URL.
    let base = Url::parse(base_url)
        .map_err(|e| CoingeckoError::BadRequest(format!("Invalid base URL '{}': {}", base_url, e)))?;
    // Safely join the path segment to the base URL.
    let full_url = base.join(path)
        .map_err(|e| CoingeckoError::BadRequest(format!("Invalid path segment '{}' for base URL '{}': {}", path, base_url, e)))?;
    // Build the request with query parameters.
    let builder = client.get(full_url).query(query);
    // Send the request and await the response text.
    let response = builder
        .send()
        .await
        .map_err(CoingeckoError::Api)?
        .text()
        .await
        .map_err(CoingeckoError::Api)?;

    // Attempt to parse the response text as JSON.
    let val: serde_json::Value = serde_json::from_str(&response).map_err(CoingeckoError::Json)?;

    // Check if the JSON response contains an 'error' field, indicating an API-level error.
    match &val["error"] {
        Value::String(x) => Err(CoingeckoError::BadRequest(x.to_string())),
        _ => Ok(val),
    }
}



/// Tests for the CoinGecko utility functions and API interactions.
#[cfg(test)]
mod tests {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };
    
    use serde_json::json;

    use super::*;

    /// Tests `to_api_format` with a simple lowercase string.
    #[test]
    fn test_to_api_format_simple() {
        assert_eq!(to_api_format("bitcoin"), "bitcoin");
    }

    /// Tests `to_api_format` with a string containing a space.
    #[test]
    fn test_to_api_format_with_space() {
        assert_eq!(to_api_format("ethereum classic"), "ethereum-classic");
    }

    /// Tests `to_api_format` with uppercase letters and a space.
    #[test]
    fn test_to_api_format_with_uppercase() {
        assert_eq!(to_api_format("Bitcoin Cash"), "bitcoin-cash");
    }

    /// Tests `to_api_format` with leading/trailing spaces.
    #[test]
    fn test_to_api_format_with_multiple_spaces() {
        assert_eq!(to_api_format("  usd coin  "), "usd-coin"); // Assumes leading/trailing spaces are trimmed by split_whitespace
    }

    /// Tests `to_api_format` with an empty string.
    #[test]
    fn test_to_api_format_empty() {
        assert_eq!(to_api_format(""), "");
    }

    /// Tests `to_api_format` with only a single space.
    #[test]
    fn test_to_api_format_single_space() {
        assert_eq!(to_api_format(" "), ""); // Single space results in empty string after split/join
    }

    /// Tests `send_request` with a successful API response using a mock server.
    #[tokio::test]
    async fn test_send_request_success() {
        // Arrange
        let server = MockServer::start().await;
        let mock_response = json!({
            "id": "bitcoin",
            "symbol": "btc",
            "name": "Bitcoin",
            "image": {
                "thumb": "https://assets.coingecko.com/coins/images/1/thumb/bitcoin.png?1547033579",
                "small": "https://assets.coingecko.com/coins/images/1/small/bitcoin.png?1547033579",
                "large": "https://assets.coingecko.com/coins/images/1/large/bitcoin.png?1547033579"
            },
            "market_data": {
                "current_price": {
                    "usd": 60000.0
                },
                "price_change_24h_in_currency": {
                    "usd": 1234.56
                },
                "price_change_percentage_24h_in_currency": {
                    "usd": 2.1
                }
            }
        });

        Mock::given(method("GET"))
            .and(path("/coins/bitcoin"))
            .and(query_param("localization", "false"))
            .and(query_param("tickers", "false"))
            .and(query_param("market_data", "true"))
            .and(query_param("community_data", "false"))
            .and(query_param("developer_data", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response.clone()))
            .expect(1)
            .mount(&server)
            .await;

        let query = vec![
            ("localization", "false"),
            ("tickers", "false"),
            ("market_data", "true"),
            ("community_data", "false"),
            ("developer_data", "false"),
        ];

        // Act
        let result = send_request(&server.uri(), "coins/bitcoin", &query).await;

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), mock_response);
        server.verify().await; // Verifies expectations
    }

    /// Tests `send_request` handling of an API-level error (e.g., invalid coin ID) returned in the JSON body.
    #[tokio::test]
    async fn test_send_request_api_error() {
        // Arrange
        let server = MockServer::start().await;
        let error_message = "Could not find coin with the given id";
        let mock_response = json!({
            "error": error_message
        });

        Mock::given(method("GET"))
            .and(path("/coins/invalid-coin"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .expect(1)
            .mount(&server)
            .await;

        let query = vec![("localization", "false")]; // Example query

        // Act
        let result = send_request(&server.uri(), "coins/invalid-coin", &query).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            CoingeckoError::BadRequest(msg) => assert_eq!(msg, error_message),
            e => panic!("Expected BadRequest error, got {:?}", e),
        }
        server.verify().await;
    }

    /// Tests `send_request` handling of an HTTP error (e.g., 404 Not Found) where the body is not valid JSON.
    #[tokio::test]
    async fn test_send_request_http_error_404() {
        // Arrange
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/coins/nonexistent"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found")) // Non-JSON body
            .expect(1)
            .mount(&server)
            .await;

        let query = vec![("localization", "false")];

        // Act
        let result = send_request(&server.uri(), "coins/nonexistent", &query).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            // Expecting JSON parsing error because the 404 body is not JSON
            CoingeckoError::Json(_) => { /* Expected */ }
            e => panic!("Expected Json error due to non-JSON 404 body, got {:?}", e),
        }
        server.verify().await;
    }

}
