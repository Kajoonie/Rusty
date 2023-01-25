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
    // volume_24h: f64,
    usd_change_24h: f64,
    perc_change_24h: f64,
    // high_24h: f64,
    // low_24h: f64,
}

impl MarketData {
    fn from_json(json: &Value) -> Option<Self> {
        let market_data = &json["market_data"];

        let price_usd = market_data["current_price"]["usd"].f64();
        // let volume_24h = market_data["total_volume"]["usd"].f64();
        let usd_change_24h = market_data["price_change_24h_in_currency"]["usd"].f64();
        let perc_change_24h = market_data["price_change_percentage_24h_in_currency"]["usd"].f64();
        // let high_24h = market_data["high_24h"]["usd"].f64();
        // let low_24h = market_data["low_24h"]["usd"].f64();

        Some(Self {
            price_usd,
            // volume_24h,
            usd_change_24h,
            perc_change_24h,
            // high_24h,
            // low_24h,
        })
    }
}

#[derive(Debug)]
struct CoinInfo {
    name: String,
    // symbol: String,
    icon: String,
    market_data: MarketData,
}

impl CoinInfo {
    fn from_json(json: &Value) -> Option<Self> {
        Some(Self {
            name: upper_first(json["id"].string()),
            // symbol: json["symbol"].string(),
            icon: json["image"]["small"].string(),
            market_data: MarketData::from_json(json)?,
        })
    }
}

fn upper_first(s: String) -> String {
    let mut c = s.chars();
    match c.next() {
        None => s,
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[derive(Error, Debug)]
enum CoingeckoError {
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Unable to parse text from JSON: {0}")]
    Json(#[from] serde_json::Error),

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

    serde_json::from_str(&response).map_err(CoingeckoError::Json)
}