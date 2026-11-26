//! Upstream image byte downloads (TMDB CDN, fanart.tv assets).
//!
//! Image downloads differ from JSON API calls in two ways that matter under
//! burst load: they are larger (so a connection can stall *mid-body*), and they
//! sit directly on the request path of a poster render. This module therefore
//! wraps them with:
//!
//! - a **total time budget** per download ([`ImageDownloadLimits::timeout`])
//!   covering every attempt, backoff sleep and permit wait, so one stalled
//!   upstream can't hold a request hostage for the shared client's 30s timeout
//!   (or longer, once retries multiply it);
//! - a **stall timeout** ([`ImageDownloadLimits::read_timeout`]): an attempt that
//!   makes no progress (no response headers, or no next body chunk) for that long
//!   is abandoned and retried;
//! - **retries on body/read errors** (truncated or stalled bodies), not just on
//!   connect errors and 429/5xx statuses, reusing the per-service
//!   [`RetryConfig`] backoff;
//! - a **concurrency limit** per upstream host, acquired per attempt (never held
//!   across a backoff sleep) and bounded by the same time budget.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{RequestBuilder, Response, StatusCode};
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::error::AppError;
use crate::services::retry::{backoff_delay, retry_after_delay, RetryConfig};

/// Default total budget for one image download, retries included.
pub const DEFAULT_IMAGE_DOWNLOAD_TIMEOUT_SECS: u64 = 15;
/// Default max time an attempt may go without progress before it is retried.
pub const DEFAULT_IMAGE_DOWNLOAD_READ_TIMEOUT_SECS: u64 = 5;
/// Default max concurrent downloads per upstream image host.
pub const DEFAULT_IMAGE_DOWNLOAD_CONCURRENCY: usize = 32;

/// Timeouts and concurrency limit for one upstream image host.
///
/// Each client (TMDB, fanart) gets its own instance — and therefore its own
/// semaphore — so a stalled fanart.tv can't starve the TMDB CDN downloads used
/// as its fallback.
#[derive(Clone, Debug)]
pub struct ImageDownloadLimits {
    /// Total budget for one download: all attempts, backoff and permit waits.
    pub timeout: Duration,
    /// Max time without progress (response headers / next body chunk) per attempt.
    pub read_timeout: Duration,
    permits: Arc<Semaphore>,
}

impl ImageDownloadLimits {
    pub fn new(timeout: Duration, read_timeout: Duration, concurrency: usize) -> Self {
        Self {
            timeout,
            read_timeout: read_timeout.min(timeout),
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
        }
    }

    /// Build limits from config. Call once per upstream host: every call
    /// creates a fresh semaphore.
    pub fn from_config(config: &Config) -> Self {
        Self::new(
            Duration::from_secs(config.image_download_timeout_secs),
            Duration::from_secs(config.image_download_read_timeout_secs),
            config.image_download_concurrency,
        )
    }
}

impl Default for ImageDownloadLimits {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(DEFAULT_IMAGE_DOWNLOAD_TIMEOUT_SECS),
            Duration::from_secs(DEFAULT_IMAGE_DOWNLOAD_READ_TIMEOUT_SECS),
            DEFAULT_IMAGE_DOWNLOAD_CONCURRENCY,
        )
    }
}

/// Result of a successful download.
#[derive(Debug, PartialEq, Eq)]
pub enum Downloaded {
    Body(Vec<u8>),
    /// 304 Not Modified (only possible for conditional requests).
    NotModified,
}

/// Download an image, retrying transient failures within the time budget.
///
/// `request_fn` builds a fresh GET for every attempt (`RequestBuilder` is not
/// `Clone`). Retried: connect/send errors, missing response headers within
/// `read_timeout`, 429 (honouring `Retry-After`, capped by the retry config),
/// 5xx, and errors or stalls while reading the body. Not retried: other non-2xx
/// statuses (e.g. 404), which are permanent for an image URL.
pub async fn download<F>(
    retry: &RetryConfig,
    limits: &ImageDownloadLimits,
    request_fn: F,
) -> Result<Downloaded, AppError>
where
    F: Fn() -> RequestBuilder,
{
    let service = retry.service_name;
    let attempts = async {
        let mut attempt: u32 = 0;
        loop {
            let mut retry_after = None;
            let err = {
                // Acquire per attempt so a permit is never held across a backoff
                // sleep; the outer budget bounds how long we can wait for one.
                let _permit = limits
                    .permits
                    .acquire()
                    .await
                    .map_err(|_| AppError::Other("image download queue closed".into()))?;
                // The per-request timeout overrides the shared client's total
                // timeout so the budget here is the only one that applies.
                let send = request_fn().timeout(limits.timeout).send();
                match tokio::time::timeout(limits.read_timeout, send).await {
                    Err(_) => AppError::Other(format!(
                        "{service}: no response within {}ms",
                        limits.read_timeout.as_millis()
                    )),
                    Ok(Err(e)) => AppError::from(e),
                    Ok(Ok(resp)) => {
                        let status = resp.status();
                        if status == StatusCode::NOT_MODIFIED {
                            return Ok(Downloaded::NotModified);
                        }
                        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                            if status == StatusCode::TOO_MANY_REQUESTS {
                                retry_after = Some(retry_after_delay(&resp, retry, attempt));
                            }
                            AppError::Other(format!("{service} returned {status}"))
                        } else if !status.is_success() {
                            // Permanent for this URL — surface it without retrying.
                            return Err(match resp.error_for_status() {
                                Err(e) => AppError::from(e),
                                Ok(_) => AppError::Other(format!("{service} returned unexpected {status}")),
                            });
                        } else {
                            match read_body(resp, limits.read_timeout).await {
                                Ok(bytes) => return Ok(Downloaded::Body(bytes)),
                                Err(e) => e,
                            }
                        }
                    }
                }
            };

            if attempt >= retry.max_retries {
                tracing::warn!(
                    service,
                    "image download failed after {} retries: {err}",
                    retry.max_retries,
                );
                return Err(err);
            }
            let delay = retry_after.unwrap_or_else(|| backoff_delay(retry, attempt));
            tracing::warn!(
                service,
                "image download failed, retrying (attempt {}/{}, delay {}ms): {err}",
                attempt + 1,
                retry.max_retries,
                delay.as_millis(),
            );
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    };

    match tokio::time::timeout(limits.timeout, attempts).await {
        Ok(result) => result,
        Err(_) => {
            let timeout_ms = limits.timeout.as_millis() as u64;
            tracing::warn!(service, timeout_ms, "image download exceeded its time budget");
            Err(AppError::Other(format!(
                "{service} image download timed out after {timeout_ms}ms"
            )))
        }
    }
}

/// True when a [`download`] error is a permanent answer for that URL — a 4xx
/// other than 408/429 (e.g. 404 Not Found, 410 Gone, 403 Forbidden) — rather
/// than a transient failure (timeouts, connect/body errors, 5xx, 429).
pub fn is_permanent_error(err: &AppError) -> bool {
    match err {
        AppError::Api(e) => e.status().is_some_and(|s| {
            s.is_client_error()
                && s != StatusCode::REQUEST_TIMEOUT
                && s != StatusCode::TOO_MANY_REQUESTS
        }),
        _ => false,
    }
}

/// Read the full body chunk by chunk, failing if the stream stalls for longer
/// than `read_timeout` between chunks or ends early (truncated body).
async fn read_body(mut resp: Response, read_timeout: Duration) -> Result<Vec<u8>, AppError> {
    // Cap the pre-allocation so a bogus Content-Length can't reserve huge memory.
    let capacity = resp.content_length().unwrap_or(0).min(8 * 1024 * 1024) as usize;
    let mut buf = Vec::with_capacity(capacity);
    loop {
        match tokio::time::timeout(read_timeout, resp.chunk()).await {
            Err(_) => {
                return Err(AppError::Other(format!(
                    "image body stalled for {}ms after {} bytes",
                    read_timeout.as_millis(),
                    buf.len()
                )));
            }
            Ok(Err(e)) => return Err(AppError::from(e)),
            Ok(Ok(None)) => return Ok(buf),
            Ok(Ok(Some(chunk))) => buf.extend_from_slice(&chunk),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// What the raw test server does on the n-th connection.
    #[derive(Clone)]
    enum Behaviour {
        /// 200 with the full body.
        Full(&'static [u8]),
        /// 200 advertising the full length, but closes after `n` bytes.
        Truncated(&'static [u8], usize),
        /// 200 advertising the full length, sends `n` bytes, then stalls.
        StallMidBody(&'static [u8], usize),
        /// Accepts the request but never sends response headers.
        StallHeaders,
        /// A bare status with an empty body.
        Status(u16),
    }

    /// Raw TCP HTTP/1.1 server so tests can misbehave at the byte level
    /// (truncate / stall mid-body), which a framework server can't do.
    /// Connection `n` gets `behaviours[n]`, or the last entry once exhausted.
    async fn raw_server(behaviours: Vec<Behaviour>) -> (String, Arc<AtomicU32>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        let counter2 = counter.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let n = counter2.fetch_add(1, Ordering::SeqCst) as usize;
                let b = behaviours
                    .get(n)
                    .or(behaviours.last())
                    .cloned()
                    .unwrap_or(Behaviour::Status(500));
                tokio::spawn(async move {
                    // Read the request head.
                    let mut req = Vec::new();
                    let mut tmp = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        match sock.read(&mut tmp).await {
                            Ok(0) | Err(_) => return,
                            Ok(k) => req.extend_from_slice(&tmp[..k]),
                        }
                    }
                    let head = |len: usize| {
                        format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n")
                    };
                    match b {
                        Behaviour::Full(body) => {
                            let _ = sock.write_all(head(body.len()).as_bytes()).await;
                            let _ = sock.write_all(body).await;
                        }
                        Behaviour::Truncated(body, n) => {
                            let _ = sock.write_all(head(body.len()).as_bytes()).await;
                            let _ = sock.write_all(&body[..n]).await;
                            // drop → connection closed before Content-Length bytes
                        }
                        Behaviour::StallMidBody(body, n) => {
                            let _ = sock.write_all(head(body.len()).as_bytes()).await;
                            let _ = sock.write_all(&body[..n]).await;
                            let _ = sock.flush().await;
                            tokio::time::sleep(Duration::from_secs(60)).await;
                        }
                        Behaviour::StallHeaders => {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                        }
                        Behaviour::Status(code) => {
                            let resp = format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                            let _ = sock.write_all(resp.as_bytes()).await;
                        }
                    }
                    let _ = sock.flush().await;
                });
            }
        });
        (format!("http://{addr}/img.jpg"), counter)
    }

    const FAST: RetryConfig = RetryConfig {
        max_retries: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        service_name: "test",
    };

    const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    fn limits(timeout_ms: u64, read_ms: u64, concurrency: usize) -> ImageDownloadLimits {
        ImageDownloadLimits::new(
            Duration::from_millis(timeout_ms),
            Duration::from_millis(read_ms),
            concurrency,
        )
    }

    #[tokio::test]
    async fn full_body_first_try() {
        let (url, counter) = raw_server(vec![Behaviour::Full(BODY)]).await;
        let client = reqwest::Client::new();
        let got = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::Body(BODY.to_vec()));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn truncated_body_is_retried() {
        let (url, counter) =
            raw_server(vec![Behaviour::Truncated(BODY, 5), Behaviour::Full(BODY)]).await;
        let client = reqwest::Client::new();
        let got = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::Body(BODY.to_vec()));
        assert_eq!(counter.load(Ordering::SeqCst), 2, "truncated body must trigger a retry");
    }

    #[tokio::test]
    async fn stalled_body_is_abandoned_and_retried() {
        let (url, counter) =
            raw_server(vec![Behaviour::StallMidBody(BODY, 5), Behaviour::Full(BODY)]).await;
        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let got = download(&FAST, &limits(5000, 200, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::Body(BODY.to_vec()));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        // One stall (~200ms read timeout) + a fast retry — nowhere near 60s.
        assert!(start.elapsed() < Duration::from_secs(2), "took {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn stalled_headers_are_abandoned_and_retried() {
        let (url, counter) =
            raw_server(vec![Behaviour::StallHeaders, Behaviour::Full(BODY)]).await;
        let client = reqwest::Client::new();
        let got = download(&FAST, &limits(5000, 200, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::Body(BODY.to_vec()));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn persistent_stall_is_bounded_by_total_budget() {
        let (url, _counter) = raw_server(vec![Behaviour::StallMidBody(BODY, 5)]).await;
        let client = reqwest::Client::new();
        // Plenty of retries, but the 600ms total budget must cut them off.
        let many = RetryConfig { max_retries: 50, ..FAST };
        let start = std::time::Instant::now();
        let res = download(&many, &limits(600, 200, 4), || client.get(&url)).await;
        let elapsed = start.elapsed();
        assert!(res.is_err());
        assert!(elapsed >= Duration::from_millis(550), "returned early: {elapsed:?}");
        assert!(elapsed < Duration::from_millis(1500), "budget not enforced: {elapsed:?}");
    }

    #[tokio::test]
    async fn retries_exhausted_returns_error() {
        let (url, counter) = raw_server(vec![Behaviour::Truncated(BODY, 3)]).await;
        let client = reqwest::Client::new();
        let res = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await;
        assert!(res.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3, "initial + 2 retries");
    }

    #[tokio::test]
    async fn server_error_then_success() {
        let (url, counter) =
            raw_server(vec![Behaviour::Status(503), Behaviour::Full(BODY)]).await;
        let client = reqwest::Client::new();
        let got = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::Body(BODY.to_vec()));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn not_found_is_not_retried() {
        let (url, counter) = raw_server(vec![Behaviour::Status(404)]).await;
        let client = reqwest::Client::new();
        let res = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await;
        assert!(res.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn error_classification_permanent_vs_transient() {
        let client = reqwest::Client::new();
        let l = limits(2000, 300, 4);
        for code in [404u16, 410, 403] {
            let (url, _) = raw_server(vec![Behaviour::Status(code)]).await;
            let err = download(&FAST, &l, || client.get(&url)).await.unwrap_err();
            assert!(is_permanent_error(&err), "{code} should be permanent: {err}");
        }
        for b in [
            Behaviour::Status(503),
            Behaviour::Status(429),
            Behaviour::Status(408),
            Behaviour::Truncated(BODY, 3),
            Behaviour::StallMidBody(BODY, 3),
        ] {
            let (url, _) = raw_server(vec![b]).await;
            let err = download(&FAST, &l, || client.get(&url)).await.unwrap_err();
            assert!(!is_permanent_error(&err), "should be transient: {err}");
        }
        // Budget overrun is transient too.
        let (url, _) = raw_server(vec![Behaviour::StallHeaders]).await;
        let err = download(&FAST, &limits(200, 200, 4), || client.get(&url)).await.unwrap_err();
        assert!(!is_permanent_error(&err));
    }

    #[tokio::test]
    async fn not_modified_is_reported() {
        let (url, _counter) = raw_server(vec![Behaviour::Status(304)]).await;
        let client = reqwest::Client::new();
        let got = download(&FAST, &limits(5000, 1000, 4), || client.get(&url)).await.unwrap();
        assert_eq!(got, Downloaded::NotModified);
    }

    #[tokio::test]
    async fn permit_wait_is_bounded_by_budget() {
        // One permit, held by a download stuck on a stalled body. A second
        // download must give up when its budget expires instead of queueing
        // behind the first indefinitely.
        let (url, _counter) = raw_server(vec![Behaviour::StallMidBody(BODY, 5)]).await;
        let client = reqwest::Client::new();
        let one = ImageDownloadLimits::new(Duration::from_secs(5), Duration::from_secs(5), 1);
        let holder = {
            let one = one.clone();
            let client = client.clone();
            let url = url.clone();
            tokio::spawn(async move { download(&FAST, &one, || client.get(&url)).await })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        let waiter = ImageDownloadLimits { timeout: Duration::from_millis(300), ..one.clone() };
        let start = std::time::Instant::now();
        let res = download(&FAST, &waiter, || client.get(&url)).await;
        assert!(res.is_err());
        assert!(start.elapsed() < Duration::from_secs(1), "permit wait unbounded: {:?}", start.elapsed());
        holder.abort();
    }

    #[test]
    fn read_timeout_is_clamped_to_total() {
        let l = ImageDownloadLimits::new(Duration::from_secs(3), Duration::from_secs(10), 0);
        assert_eq!(l.read_timeout, Duration::from_secs(3));
        // concurrency 0 is coerced to 1 rather than deadlocking every download
        assert_eq!(l.permits.available_permits(), 1);
    }
}
