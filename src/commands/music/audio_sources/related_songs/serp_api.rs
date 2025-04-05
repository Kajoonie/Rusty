use crate::commands::music::audio_sources::track_metadata::TrackMetadata;
use crate::commands::music::utils::music_manager::MusicError;
use serenity::async_trait;
use serpapi_search_rust::serp_api_search::SerpApiSearch;
use std::collections::HashMap;
use std::time::Duration;

use super::{RelatedSongsFetcher, RelatedSongsResult};

#[async_trait]
pub trait SerpApiSearcher: Send + Sync {
    async fn search_youtube_video(&self, video_id: &str) -> Result<serde_json::Value, String>;
}

/// Concrete implementation using the serpapi-search-rust crate
pub struct RealSerpApiSearcher {
    api_key: String,
}

impl RealSerpApiSearcher {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl SerpApiSearcher for RealSerpApiSearcher {
    async fn search_youtube_video(&self, video_id: &str) -> Result<serde_json::Value, String> {
        let mut params: HashMap<String, String> = HashMap::new();
        params.insert("v".to_string(), video_id.to_string());

        let search = SerpApiSearch::new("youtube_video".to_string(), params, self.api_key.clone());

        search
            .json()
            .await
            .map_err(|e| format!("SerpAPI request failed: {}", e))
    }
}

use std::sync::Arc; // Add Arc for shared ownership

/// SerpAPI implementation for fetching related songs, generic over the searcher
pub struct SerpApiFetcher<S: SerpApiSearcher> {
    searcher: Arc<S>, // Store the searcher instance
}

impl<S: SerpApiSearcher> SerpApiFetcher<S> {
    // Accept an Arc<S> to allow shared ownership if needed
    pub fn new(searcher: Arc<S>) -> Self {
        Self { searcher }
    }

    /// Helper function to parse YouTube duration strings (e.g. "5:32" or "1:23:45")
    fn parse_duration_string(duration_str: &str) -> Option<Duration> {
        let parts: Vec<&str> = duration_str.split(':').collect();

        match parts.len() {
            // MM:SS format
            2 => {
                let minutes = parts[0].parse::<u64>().ok()?;
                let seconds = parts[1].parse::<u64>().ok()?;
                Some(Duration::from_secs(minutes * 60 + seconds))
            }
            // HH:MM:SS format
            3 => {
                let hours = parts[0].parse::<u64>().ok()?;
                let minutes = parts[1].parse::<u64>().ok()?;
                let seconds = parts[2].parse::<u64>().ok()?;
                Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
            }
            _ => None,
        }
    }
}

#[async_trait]
// Add generic parameter and Send + Sync bounds for async trait safety
impl<S: SerpApiSearcher + Send + Sync> RelatedSongsFetcher for SerpApiFetcher<S> {
    async fn fetch_related_songs(&self, video_id: &str) -> RelatedSongsResult {
        // Use the injected searcher
        let results = self
            .searcher
            .search_youtube_video(video_id)
            .await
            .map_err(|e| MusicError::AudioSourceError(e))?;

        let mut related_songs = Vec::new();

        // Check if the response contains related videos
        if let Some(related_videos) = results.get("related_videos").and_then(|v| v.as_array()) {
            for video in related_videos {
                if let (Some(title), Some(link)) = (
                    video.get("title").and_then(|t| t.as_str()),
                    video.get("link").and_then(|l| l.as_str()),
                ) {
                    // Extract duration if available
                    // Note: The parse_duration_string is associated with SerpApiFetcher,
                    // so we need Self:: or SerpApiFetcher::<S>::
                    let duration = video
                        .get("length")
                        .and_then(|d| d.as_str())
                        .and_then(SerpApiFetcher::<S>::parse_duration_string);

                    // Extract thumbnail if available
                    let thumbnail = video
                        .get("thumbnail")
                        .and_then(|t| t.get("static"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());

                    related_songs.push(TrackMetadata {
                        title: title.to_string(),
                        url: Some(link.to_string()),
                        duration,
                        thumbnail,
                        requested_by: Some("Autoplay".into()),
                    });

                    // Limit to 5 related videos
                    if related_songs.len() >= 5 {
                        break;
                    }
                }
            }
        }

        Ok(related_songs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::music::utils::music_manager::MusicError;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    // Mock Searcher for testing
    struct MockSerpApiSearcher {
        expected_result: Result<serde_json::Value, String>,
    }

    #[async_trait]
    impl SerpApiSearcher for MockSerpApiSearcher {
        async fn search_youtube_video(&self, _video_id: &str) -> Result<serde_json::Value, String> {
            // Clone the result to return ownership
            self.expected_result.clone()
        }
    }

    // Helper to create TrackMetadata for assertions
    fn create_metadata(
        title: &str,
        url: Option<&str>,
        duration_secs: Option<u64>,
        thumbnail: Option<&str>,
    ) -> TrackMetadata {
        TrackMetadata {
            title: title.to_string(),
            url: url.map(String::from),
            duration: duration_secs.map(Duration::from_secs),
            thumbnail: thumbnail.map(String::from),
            requested_by: Some("Autoplay".into()),
        }
    }

    #[tokio::test]
    async fn test_fetch_related_songs_success() {
        // Arrange
        let video_id = "test_video_id";
        let mock_response_json = json!({
            "search_metadata": { "status": "Success" },
            "related_videos": [
                {
                    "title": "Related Song 1",
                    "link": "https://youtube.com/watch?v=related1",
                    "length": "3:45", // 225 seconds
                    "thumbnail": { "static": "https://thumb.nail/1.jpg" }
                },
                {
                    "title": "Related Song 2",
                    "link": "https://youtube.com/watch?v=related2",
                    "length": "0:55", // 55 seconds
                    "thumbnail": { "static": "https://thumb.nail/2.jpg" }
                },
                { // Missing length
                    "title": "Related Song 3",
                    "link": "https://youtube.com/watch?v=related3",
                    "thumbnail": { "static": "https://thumb.nail/3.jpg" }
                },
                { // Missing thumbnail
                    "title": "Related Song 4",
                    "link": "https://youtube.com/watch?v=related4",
                    "length": "1:02:03" // 3723 seconds
                },
                 { // Missing link - should be skipped
                    "title": "Related Song 5 - No Link",
                    "length": "4:00"
                },
                { // Missing title - should be skipped
                    "link": "https://youtube.com/watch?v=related6",
                    "length": "4:00"
                },
                {
                    "title": "Related Song 7",
                    "link": "https://youtube.com/watch?v=related7",
                    "length": "4:00",
                    "thumbnail": { "static": "https://thumb.nail/7.jpg" }
                }
            ]
        });

        let mock_searcher = Arc::new(MockSerpApiSearcher {
            expected_result: Ok(mock_response_json),
        });
        let fetcher: SerpApiFetcher<MockSerpApiSearcher> = SerpApiFetcher::new(mock_searcher);

        // Act
        let result = fetcher.fetch_related_songs(video_id).await;

        // Assert
        assert!(
            result.is_ok(),
            "fetch_related_songs failed: {:?}",
            result.err()
        );
        let songs = result.unwrap();

        let expected_songs = vec![
            create_metadata(
                "Related Song 1",
                Some("https://youtube.com/watch?v=related1"),
                Some(225),
                Some("https://thumb.nail/1.jpg"),
            ),
            create_metadata(
                "Related Song 2",
                Some("https://youtube.com/watch?v=related2"),
                Some(55),
                Some("https://thumb.nail/2.jpg"),
            ),
            create_metadata(
                "Related Song 3",
                Some("https://youtube.com/watch?v=related3"),
                None,
                Some("https://thumb.nail/3.jpg"),
            ),
            create_metadata(
                "Related Song 4",
                Some("https://youtube.com/watch?v=related4"),
                Some(3723),
                None,
            ),
            // Song 5 & 6 skipped due to missing fields
            // Song 7 is the 5th valid song, added before break
            create_metadata(
                "Related Song 7",
                Some("https://youtube.com/watch?v=related7"),
                Some(240), // "4:00"
                Some("https://thumb.nail/7.jpg"),
            ),
        ];

        assert_eq!(songs.len(), 5); // Expect 5 songs due to limit check placement
        assert_eq!(songs, expected_songs);
    }

    #[tokio::test]
    async fn test_fetch_related_songs_api_error() {
        // Arrange
        let video_id = "test_video_id_error";
        let error_message = "Simulated API error".to_string();

        let mock_searcher = Arc::new(MockSerpApiSearcher {
            expected_result: Err(error_message.clone()),
        });
        let fetcher: SerpApiFetcher<MockSerpApiSearcher> = SerpApiFetcher::new(mock_searcher);

        // Act
        let result = fetcher.fetch_related_songs(video_id).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            MusicError::AudioSourceError(msg) => {
                // The error message from the mock searcher should be passed through
                assert_eq!(msg, error_message);
            }
            e => panic!("Expected MusicError::AudioSourceError, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_fetch_related_songs_empty_results() {
        // Arrange
        let video_id = "test_video_id_empty";
        let mock_response_json = json!({
            "search_metadata": { "status": "Success" },
            "related_videos": [] // Empty array
        });

        let mock_searcher = Arc::new(MockSerpApiSearcher {
            expected_result: Ok(mock_response_json),
        });
        let fetcher: SerpApiFetcher<MockSerpApiSearcher> = SerpApiFetcher::new(mock_searcher);

        // Act
        let result = fetcher.fetch_related_songs(video_id).await;

        // Assert
        assert!(
            result.is_ok(),
            "fetch_related_songs failed: {:?}",
            result.err()
        );
        let songs = result.unwrap();
        assert!(songs.is_empty(), "Expected empty results, got {:?}", songs);
    }

    #[tokio::test]
    async fn test_fetch_related_songs_no_related_videos_key() {
        // Arrange
        let video_id = "test_video_id_no_key";
        let mock_response_json = json!({ // Missing 'related_videos' key entirely
            "search_metadata": { "status": "Success" }
        });

        let mock_searcher = Arc::new(MockSerpApiSearcher {
            expected_result: Ok(mock_response_json),
        });
        let fetcher: SerpApiFetcher<MockSerpApiSearcher> = SerpApiFetcher::new(mock_searcher);

        // Act
        let result = fetcher.fetch_related_songs(video_id).await;

        // Assert
        assert!(
            result.is_ok(),
            "fetch_related_songs failed: {:?}",
            result.err()
        );
        let songs = result.unwrap();
        assert!(
            songs.is_empty(),
            "Expected empty results when key is missing, got {:?}",
            songs
        );
    }

    #[tokio::test]
    async fn test_fetch_related_songs_malformed_json() {
        // Arrange
        let video_id = "test_video_id_malformed";
        // Simulate the error that would occur if the underlying crate failed to parse JSON
        let error_message = "Simulated JSON parsing error".to_string();

        let mock_searcher = Arc::new(MockSerpApiSearcher {
            expected_result: Err(error_message.clone()),
        });
        let fetcher: SerpApiFetcher<MockSerpApiSearcher> = SerpApiFetcher::new(mock_searcher);

        // Act
        let result = fetcher.fetch_related_songs(video_id).await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            MusicError::AudioSourceError(msg) => {
                // The error message from the mock searcher should be passed through
                assert_eq!(msg, error_message);
            }
            e => panic!("Expected MusicError::AudioSourceError, got {:?}", e),
        }
    }

    #[test]
    fn test_parse_duration_string() {
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("5:32"),
            Some(Duration::from_secs(332))
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("1:23:45"),
            Some(Duration::from_secs(5025))
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("0:59"),
            Some(Duration::from_secs(59))
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("10:00"),
            Some(Duration::from_secs(600))
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("1:00:00"),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string(""),
            None
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("abc"),
            None
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("5:"),
            None
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string(":32"),
            None
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("1:2:3:4"),
            None
        );
        assert_eq!(
            SerpApiFetcher::<MockSerpApiSearcher>::parse_duration_string("1:aa:30"),
            None
        );
    }
}
