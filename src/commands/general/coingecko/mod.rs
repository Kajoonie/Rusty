use serde_json::Value;
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
        .into_iter()
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

async fn send_request(url: &str, query: &[(&str, &str)]) -> Result<Value, CoingeckoError> {
    let client = reqwest::Client::new();
    let builder = client.get(url).query(query);
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
