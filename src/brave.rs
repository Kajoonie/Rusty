#[cfg(feature = "brave_search")]
use reqwest::Client;
#[cfg(feature = "brave_search")]
use serde::Deserialize;
#[cfg(feature = "brave_search")]
use std::env;
#[cfg(feature = "brave_search")]
use thiserror::Error;

#[cfg(feature = "brave_search")]
#[derive(Error, Debug)]
pub enum BraveSearchError {
    #[error("API communication failure: {0}")]
    Api(#[from] reqwest::Error),

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Unable to parse response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("No search results found")]
    NoResults,
}

#[cfg(feature = "brave_search")]
#[derive(Debug, Deserialize)]
pub struct BraveSearchResponse {
    pub web: BraveWebResults,
}

#[cfg(feature = "brave_search")]
#[derive(Debug, Deserialize)]
pub struct BraveWebResults {
    pub results: Vec<BraveWebResult>,
}

#[cfg(feature = "brave_search")]
#[derive(Debug, Deserialize)]
pub struct BraveWebResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

#[cfg(not(feature = "brave_search"))]
#[derive(Debug)]
pub enum BraveSearchError {
    FeatureNotEnabled,
}

#[cfg(not(feature = "brave_search"))]
#[derive(Debug)]
pub struct BraveWebResult {}

#[cfg(feature = "brave_search")]
pub async fn search(query: &str) -> Result<Vec<BraveWebResult>, BraveSearchError> {
    let api_key = env::var("BRAVE_API_KEY").map_err(|_| {
        BraveSearchError::MissingApiKey(
            "BRAVE_API_KEY not found in environment variables".to_string(),
        )
    })?;

    let client = Client::new();
    let response = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .query(&[("q", query)])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await?;

    let search_response: BraveSearchResponse = response.json().await?;

    if search_response.web.results.is_empty() {
        return Err(BraveSearchError::NoResults);
    }

    Ok(search_response.web.results)
}

#[cfg(not(feature = "brave_search"))]
pub async fn search(_query: &str) -> Result<Vec<BraveWebResult>, BraveSearchError> {
    Err(BraveSearchError::FeatureNotEnabled)
}

#[cfg(feature = "brave_search")]
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

#[cfg(not(feature = "brave_search"))]
pub fn format_search_results(_results: &[BraveWebResult], _query: &str) -> String {
    "The Brave Search feature is not enabled.".to_string()
}