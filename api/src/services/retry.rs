use std::future::Future;
use std::time::Duration;

use rand::Rng;
use reqwest::Response;

use crate::error::AppError;

pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub service_name: &'static str,
}

pub const TMDB_API_RETRY: RetryConfig = RetryConfig {
    max_retries: 2,
    base_delay: Duration::from_millis(500),
    max_delay: Duration::from_secs(4),
    service_name: "tmdb",
};

pub const TMDB_CDN_RETRY: RetryConfig = RetryConfig {
    max_retries: 2,
    base_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(8),
    service_name: "tmdb-cdn",
};

pub const FANART_RETRY: RetryConfig = RetryConfig {
    max_retries: 3,
    base_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(8),
    service_name: "fanart",
};

pub const OMDB_RETRY: RetryConfig = RetryConfig {
    max_retries: 1,
    base_delay: Duration::from_secs(2),
    max_delay: Duration::from_secs(2),
    service_name: "omdb",
};

pub const MDBLIST_RETRY: RetryConfig = RetryConfig {
    max_retries: 1,
    base_delay: Duration::from_secs(2),
    max_delay: Duration::from_secs(2),
    service_name: "mdblist",
};

/// Send an HTTP request with retry logic for transient failures.
///
/// The closure must rebuild and send the request each time (since `RequestBuilder`
/// is not `Clone`). Returns the raw `Response` so callers keep their existing
/// status-handling logic.
pub async fn send_with_retry<F, Fut>(config: &RetryConfig, request_fn: F) -> Result<Response, AppError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Response, reqwest::Error>>,
{
    for attempt in 0..=config.max_retries {
        let result = request_fn().await;

        match result {
            Ok(resp) => {
                let status = resp.status();

                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    if attempt == config.max_retries {
                        tracing::warn!(
                            service = config.service_name,
                            "request failed after {} retries: 429 Too Many Requests",
                            config.max_retries,
                        );
                        return Ok(resp);
                    }

                    let delay = retry_after_delay(&resp, config, attempt);
                    tracing::warn!(
                        service = config.service_name,
                        "429 Too Many Requests, retrying (attempt {}/{}, delay {}ms)",
                        attempt + 1,
                        config.max_retries,
                        delay.as_millis(),
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }

                if status.is_server_error() {
                    if attempt == config.max_retries {
                        tracing::warn!(
                            service = config.service_name,
                            "request failed after {} retries: {status}",
                            config.max_retries,
                        );
                        return Ok(resp);
                    }

                    let delay = backoff_delay(config, attempt);
                    tracing::warn!(
                        service = config.service_name,
                        "{status}, retrying (attempt {}/{}, delay {}ms)",
                        attempt + 1,
                        config.max_retries,
                        delay.as_millis(),
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }

                // 2xx, 3xx, or non-retryable 4xx — return immediately
                return Ok(resp);
            }
            Err(e) => {
                if attempt == config.max_retries {
                    tracing::warn!(
                        service = config.service_name,
                        "request failed after {} retries: {e}",
                        config.max_retries,
                    );
                    return Err(AppError::Api(e));
                }

                let delay = backoff_delay(config, attempt);
                tracing::warn!(
                    service = config.service_name,
                    "connection error, retrying (attempt {}/{}, delay {}ms): {e}",
                    attempt + 1,
                    config.max_retries,
                    delay.as_millis(),
                );
                tokio::time::sleep(delay).await;
            }
        }
    }

    // All paths return inside the loop; this is unreachable but satisfies the compiler.
    unreachable!("retry loop always returns within max_retries iterations")
}

/// Parse `Retry-After` header as seconds, falling back to exponential backoff.
fn retry_after_delay(resp: &Response, config: &RetryConfig, attempt: u32) -> Duration {
    if let Some(val) = resp.headers().get(reqwest::header::RETRY_AFTER) {
        if let Ok(s) = val.to_str() {
            if let Ok(secs) = s.trim().parse::<u64>() {
                let capped = Duration::from_secs(secs).min(config.max_delay);
                return add_jitter(capped);
            }
        }
    }
    backoff_delay(config, attempt)
}

/// Exponential backoff: `min(base_delay * 2^attempt, max_delay)` + jitter.
fn backoff_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let delay = config
        .base_delay
        .saturating_mul(1 << attempt)
        .min(config.max_delay);
    add_jitter(delay)
}

/// Add small random jitter (0–25% of delay).
fn add_jitter(delay: Duration) -> Duration {
    let jitter_ms = rand::rng().random_range(0..=delay.as_millis() as u64 / 4);
    delay + Duration::from_millis(jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use axum::response::IntoResponse;

    /// Tiny test server that returns a sequence of status codes, then 200.
    /// Returns the server URL and a request counter.
    async fn mock_server(statuses: Vec<u16>) -> (String, Arc<AtomicU32>) {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let statuses = Arc::new(statuses);

        let app = axum::Router::new().route(
            "/",
            axum::routing::get(move || {
                let counter = counter_clone.clone();
                let statuses = statuses.clone();
                async move {
                    let n = counter.fetch_add(1, Ordering::SeqCst) as usize;
                    let code = statuses.get(n).copied().unwrap_or(200);
                    axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::OK)
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app).into_future());
        (format!("http://{addr}"), counter)
    }

    /// Server that returns a specific header along with the status code sequence.
    async fn mock_server_with_headers(
        statuses: Vec<(u16, Vec<(&'static str, &'static str)>)>,
    ) -> (String, Arc<AtomicU32>) {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let statuses: Arc<Vec<(u16, Vec<(&'static str, &'static str)>)>> = Arc::new(statuses);

        let app = axum::Router::new().route(
            "/",
            axum::routing::get(move || {
                let counter = counter_clone.clone();
                let statuses = statuses.clone();
                async move {
                    let n = counter.fetch_add(1, Ordering::SeqCst) as usize;
                    let (code, headers) = statuses
                        .get(n)
                        .cloned()
                        .unwrap_or((200, vec![]));
                    let status = axum::http::StatusCode::from_u16(code)
                        .unwrap_or(axum::http::StatusCode::OK);
                    let mut resp = status.into_response();
                    for (k, v) in headers {
                        resp.headers_mut().insert(
                            axum::http::HeaderName::from_static(k),
                            axum::http::HeaderValue::from_static(v),
                        );
                    }
                    resp
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app).into_future());
        (format!("http://{addr}"), counter)
    }

    const FAST: RetryConfig = RetryConfig {
        max_retries: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        service_name: "test",
    };

    #[test]
    fn backoff_respects_max_delay() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(4),
            service_name: "test",
        };
        let delay = backoff_delay(&config, 3);
        assert!(delay <= Duration::from_secs(5));
        assert!(delay >= Duration::from_secs(4));
    }

    #[test]
    fn backoff_first_attempt() {
        let config = RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(4),
            service_name: "test",
        };
        let delay = backoff_delay(&config, 0);
        assert!(delay >= Duration::from_millis(500));
        assert!(delay <= Duration::from_millis(625));
    }

    #[tokio::test]
    async fn success_on_first_try() {
        let (url, counter) = mock_server(vec![200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_429_then_success() {
        let (url, counter) = mock_server(vec![429, 200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_429_exhausted_returns_429() {
        let (url, counter) = mock_server(vec![429, 429, 429]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 429);
        assert_eq!(counter.load(Ordering::SeqCst), 3); // initial + 2 retries
    }

    #[tokio::test]
    async fn retry_500_then_success() {
        let (url, counter) = mock_server(vec![500, 200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_502_then_success() {
        let (url, counter) = mock_server(vec![502, 200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_503_then_success() {
        let (url, counter) = mock_server(vec![503, 200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn no_retry_on_404() {
        let (url, counter) = mock_server(vec![404]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 404);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_retry_on_401() {
        let (url, counter) = mock_server(vec![401]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 401);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_retry_on_403() {
        let (url, counter) = mock_server(vec![403]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 403);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_retry_on_400() {
        let (url, counter) = mock_server(vec![400]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 400);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn connection_error_retries() {
        // Point at a port nothing is listening on
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        let config = RetryConfig {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            service_name: "test",
        };
        let result =
            send_with_retry(&config, || client.get("http://127.0.0.1:1").send()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn retry_after_header_respected() {
        let (url, counter) = mock_server_with_headers(vec![
            (429, vec![("retry-after", "1")]),
            (200, vec![]),
        ])
        .await;
        let client = reqwest::Client::new();
        let config = RetryConfig {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_secs(5),
            service_name: "test",
        };
        let start = tokio::time::Instant::now();
        let resp = send_with_retry(&config, || client.get(&url).send()).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        // Should have waited ~1s (the Retry-After value), not the 1ms base_delay
        assert!(elapsed >= Duration::from_millis(900));
    }

    #[tokio::test]
    async fn retry_after_capped_to_max_delay() {
        let (url, counter) = mock_server_with_headers(vec![
            (429, vec![("retry-after", "60")]),
            (200, vec![]),
        ])
        .await;
        let client = reqwest::Client::new();
        let config = RetryConfig {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(50),
            service_name: "test",
        };
        let start = tokio::time::Instant::now();
        let resp = send_with_retry(&config, || client.get(&url).send()).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        // 60s Retry-After capped to 50ms max_delay + up to 25% jitter
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn mixed_5xx_then_429_then_success() {
        let (url, counter) = mock_server(vec![503, 429, 200]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn server_error_exhausted_returns_last_status() {
        let (url, counter) = mock_server(vec![503, 502, 500]).await;
        let client = reqwest::Client::new();
        let resp = send_with_retry(&FAST, || client.get(&url).send()).await.unwrap();
        assert_eq!(resp.status(), 500);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
