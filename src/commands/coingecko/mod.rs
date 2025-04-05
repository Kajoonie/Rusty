use serde_json::Value;
use reqwest::Url;
use thiserror::Error;

pub(crate) mod coin;

const API: &str = "https://api.coingecko.com/api/v3/";

trait JsonParse {
    fn f64(&self) -> f64;
    fn string(&self) -> String;
}

impl JsonParse for Value {
    fn f64(&self) -> f64 {
        match &self {
            Value::Number(x) => x.as_f64().unwrap(),
            _ => 0.0,
        }
    }

    fn string(&self) -> String {
        match &self {
            Value::String(x) => x.to_owned(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug)]
struct MarketData {
    price_usd: f64,
    usd_change_24h: f64,
    perc_change_24h: f64,
}

impl MarketData {
    fn from_json(json: &Value) -> Option<Self> {
        let market_data = &json["market_data"];

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

#[derive(Debug)]
struct CoinInfo {
    name: String,
    icon: String,
    market_data: MarketData,
}

impl CoinInfo {
    fn from_json(json: &Value) -> Option<Self> {
        Some(Self {
            name: from_api_format(&json["id"].string()),
            icon: json["image"]["small"].string(),
            market_data: MarketData::from_json(json)?,
        })
    }
}

fn to_api_format(s: &str) -> String {
    s.as_bytes()
        .to_ascii_lowercase()
        .iter()
        .map(|x| char::from(*x))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
}

fn from_api_format(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => word.to_string(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[derive(Error, Debug)]
enum CoingeckoError {
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Unable to parse text from JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Invalid response received from CoinGecko")]
    Invalid,
}

async fn send_request(base_url: &str, path: &str, query: &[(&str, &str)]) -> Result<Value, CoingeckoError> {
    let client = reqwest::Client::new();
    // Parse the base URL and join the path correctly
    let base = Url::parse(base_url)
        .map_err(|e| CoingeckoError::BadRequest(format!("Invalid base URL '{}': {}", base_url, e)))?;
    let full_url = base.join(path)
        .map_err(|e| CoingeckoError::BadRequest(format!("Invalid path segment '{}' for base URL '{}': {}", path, base_url, e)))?;
    let builder = client.get(full_url).query(query); // Use the parsed Url object
    let response = builder
        .send()
        .await
        .map_err(CoingeckoError::Api)?
        .text()
        .await
        .map_err(CoingeckoError::Api)?;

    let val: serde_json::Value = serde_json::from_str(&response).map_err(CoingeckoError::Json)?;

    match &val["error"] {
        Value::String(x) => Err(CoingeckoError::BadRequest(x.to_string())),
        _ => Ok(val),
    }
}



#[cfg(test)]
mod tests {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };
    use tokio;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_to_api_format_simple() {
        assert_eq!(to_api_format("bitcoin"), "bitcoin");
    }

    #[test]
    fn test_to_api_format_with_space() {
        assert_eq!(to_api_format("ethereum classic"), "ethereum-classic");
    }

    #[test]
    fn test_to_api_format_with_uppercase() {
        assert_eq!(to_api_format("Bitcoin Cash"), "bitcoin-cash");
    }

    #[test]
    fn test_to_api_format_with_multiple_spaces() {
        assert_eq!(to_api_format("  usd coin  "), "usd-coin"); // Assumes leading/trailing spaces are trimmed by split_whitespace
    }

    #[test]
    fn test_to_api_format_empty() {
        assert_eq!(to_api_format(""), "");
    }

    #[test]
    fn test_to_api_format_single_space() {
        assert_eq!(to_api_format(" "), ""); // Single space results in empty string after split/join
    }

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
