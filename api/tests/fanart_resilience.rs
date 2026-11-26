//! Fanart resilience under a failing / stalling asset host (issue #80):
//! download failures are negative-cached for a short TTL, and a cached fallback
//! poster is served without waiting on fanart.
//!
//! Upstream metadata (ID resolution, fanart image lists, TMDB base poster) is
//! pre-seeded into the in-memory caches / disk so the only network traffic is
//! the fanart asset download, which points at a local misbehaving server.

mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openposterdb_api::cache;
use openposterdb_api::id::{MediaType, ResolvedId};
use openposterdb_api::image::serve;
use openposterdb_api::services::db::{ImageSize, ImageSource, RenderSettings};
use openposterdb_api::services::fanart::{FanartClient, FanartImages, FanartPoster};
use openposterdb_api::services::image_download::ImageDownloadLimits;
use openposterdb_api::AppState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ID: &str = "tt7777777";
const TMDB_ID: u64 = 777_777;

#[derive(Clone, Copy)]
enum Upstream {
    /// Send headers + a few body bytes, then stall forever.
    StallMidBody,
    /// Serve this JPEG.
    Jpeg(&'static [u8]),
    /// Respond with this status and an empty body.
    Status(u16),
}

/// Raw HTTP server standing in for assets.fanart.tv. Returns its URL and a
/// connection counter.
async fn fanart_asset_server(mode: Upstream) -> (String, Arc<AtomicU32>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let counter = Arc::new(AtomicU32::new(0));
    let counter2 = counter.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            counter2.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut req = Vec::new();
                let mut tmp = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => req.extend_from_slice(&tmp[..n]),
                    }
                }
                match mode {
                    Upstream::StallMidBody => {
                        let head = "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n";
                        let _ = sock.write_all(head.as_bytes()).await;
                        let _ = sock.write_all(&[0xFF, 0xD8, 0xFF]).await;
                        let _ = sock.flush().await;
                        tokio::time::sleep(Duration::from_secs(120)).await;
                    }
                    Upstream::Status(code) => {
                        let resp = format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        let _ = sock.write_all(resp.as_bytes()).await;
                        let _ = sock.flush().await;
                    }
                    Upstream::Jpeg(body) => {
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = sock.write_all(head.as_bytes()).await;
                        let _ = sock.write_all(body).await;
                        let _ = sock.flush().await;
                    }
                }
            });
        }
    });
    (format!("http://{addr}/fanart/poster.jpg"), counter)
}

fn jpeg(r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(200, 300, image::Rgb([r, g, b]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}

fn fanart_settings() -> RenderSettings {
    RenderSettings {
        image_source: ImageSource::Fanart,
        // No ratings → no ratings-provider calls; badges suffix is just "@".
        ratings_limit: 0,
        logo_ratings_limit: 0,
        ..RenderSettings::default()
    }
}

fn poster(url: &str, lang: &str) -> FanartPoster {
    FanartPoster { id: "fa999".into(), url: url.into(), lang: lang.into(), likes: "1".into() }
}

/// Build a test app whose fanart client downloads with short limits, with ID
/// resolution, fanart metadata and the TMDB base poster pre-seeded.
async fn setup(asset_url: &str, budget: Duration, read: Duration) -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (_app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        cache_dir_override: Some(dir.path().to_string_lossy().into_owned()),
        ..Default::default()
    })
    .await;
    let mut st = (*state).clone();
    st.fanart = Some(
        FanartClient::new("test".into(), reqwest::Client::new())
            .with_image_limits(ImageDownloadLimits::new(budget, read, 4)),
    );
    let state = Arc::new(st);

    state
        .id_cache
        .insert(
            format!("imdb/{ID}"),
            ResolvedId {
                imdb_id: Some(ID.into()),
                tmdb_id: TMDB_ID,
                tvdb_id: None,
                media_type: MediaType::Movie,
                poster_path: Some("/base.jpg".into()),
                // Old film → cache entries never go stale (no stale refreshes).
                release_date: Some("2000-01-01".into()),
                episode: None,
            },
        )
        .await;
    state
        .fanart_cache
        .insert(
            format!("movie:{TMDB_ID}"),
            Arc::new(FanartImages {
                posters: vec![poster(asset_url, "en")],
                logos: vec![poster(asset_url, "en")],
                backdrops: vec![],
            }),
        )
        .await;
    // TMDB fallback base poster already downloaded.
    let base = cache::base_poster_path(
        &state.config.cache_dir,
        "/base.jpg",
        ImageSize::Medium.tmdb_size(),
    )
    .unwrap();
    cache::write(&base, &jpeg(200, 30, 30)).await.unwrap();
    (state, dir)
}

/// Files under `{cache}/posters/imdb/` whose name starts with `prefix`.
fn poster_files(state: &AppState, prefix: &str) -> Vec<String> {
    let dir = std::path::Path::new(&state.config.cache_dir).join("posters").join("imdb");
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect()
}

async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    cond()
}

async fn poster_request(state: &AppState) -> (bytes::Bytes, Duration) {
    let start = Instant::now();
    let (bytes, _) = serve::handle_inner(state, "imdb", &format!("{ID}.jpg"), fanart_settings(), None)
        .await
        .expect("poster should be served from the TMDB fallback");
    (bytes, start.elapsed())
}

#[tokio::test]
async fn stalled_fanart_download_is_bounded_and_negative_cached() {
    let (url, hits) = fanart_asset_server(Upstream::StallMidBody).await;
    let (state, _dir) = setup(&url, Duration::from_millis(800), Duration::from_millis(200)).await;

    // Cold cache: the fanart attempt stalls mid-body, is cut off by the 800ms
    // budget, and the request falls back to TMDB art instead of hanging.
    let (first, elapsed) = poster_request(&state).await;
    assert!(elapsed < Duration::from_secs(5), "cold request not bounded: {elapsed:?}");
    assert!(hits.load(Ordering::SeqCst) >= 1, "fanart should have been attempted");

    // Recorded as a short-lived *failure*, not as "fanart has no art".
    state.fanart_failed.run_pending_tasks().await;
    assert!(state.fanart_failed.get(&format!("imdb/{ID}_f_err")).await.is_some());
    assert!(state.fanart_negative.get(&format!("imdb/{ID}_f_en_neg")).await.is_none());
    // The fallback render is cached under the fanart-fallback variant.
    assert_eq!(poster_files(&state, &format!("{ID}_f_fallback")).len(), 1);

    // Within the failure TTL nothing contacts fanart again — even with the
    // memory cache dropped (served from disk).
    let attempts = hits.load(Ordering::SeqCst);
    state.image_mem_cache.invalidate_all();
    state.image_inflight.invalidate_all();
    let (second, elapsed) = poster_request(&state).await;
    assert_eq!(second, first);
    assert!(elapsed < Duration::from_millis(700), "warm request waited: {elapsed:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(hits.load(Ordering::SeqCst), attempts, "failure negative cache must suppress retries");
}

#[tokio::test]
async fn cached_fallback_is_served_without_waiting_on_fanart() {
    let (url, hits) = fanart_asset_server(Upstream::StallMidBody).await;
    // A foreground fanart attempt would take the full 2s budget.
    let (state, _dir) = setup(&url, Duration::from_secs(2), Duration::from_secs(2)).await;

    // Produce the fallback render while fanart is known-failing (no fanart call).
    state.fanart_failed.insert(format!("imdb/{ID}_f_err"), ()).await;
    let (fallback, _) = poster_request(&state).await;
    assert_eq!(hits.load(Ordering::SeqCst), 0, "blocked fanart must not be contacted");

    // Simulate a restart / failure-TTL expiry: in-memory state is gone, only the
    // disk cache remains, and fanart is still stalling.
    state.fanart_failed.invalidate_all();
    state.image_mem_cache.invalidate_all();
    state.image_inflight.invalidate_all();
    state.refresh_locks.invalidate_all();

    let (served, elapsed) = poster_request(&state).await;
    assert_eq!(served, fallback);
    assert!(elapsed < Duration::from_secs(1), "cached poster waited on fanart: {elapsed:?}");

    // Fanart is retried in the background instead, and its failure recorded.
    assert!(wait_until(Duration::from_secs(2), || hits.load(Ordering::SeqCst) >= 1).await);
    let key = format!("imdb/{ID}_f_err");
    let mut recorded = false;
    for _ in 0..200 {
        if state.fanart_failed.get(&key).await.is_some() {
            recorded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(recorded, "background attempt should record the failure");
}

#[tokio::test]
async fn background_retry_upgrades_fallback_to_fanart() {
    let fanart_jpeg: &'static [u8] = Box::leak(jpeg(30, 30, 200).into_boxed_slice());
    let (url, hits) = fanart_asset_server(Upstream::Jpeg(fanart_jpeg)).await;
    let (state, _dir) = setup(&url, Duration::from_secs(5), Duration::from_secs(2)).await;

    // Fallback produced while fanart was failing.
    state.fanart_failed.insert(format!("imdb/{ID}_f_err"), ()).await;
    let (fallback, _) = poster_request(&state).await;
    state.fanart_failed.invalidate_all();

    // Fanart recovered: the next request still gets the cached fallback at once…
    let (served, _) = poster_request(&state).await;
    assert_eq!(served, fallback);

    // …while the background attempt fetches the fanart art and caches its render.
    let upgraded = wait_until(Duration::from_secs(10), || {
        !poster_files(&state, &format!("{ID}_f_en")).is_empty()
    })
    .await;
    assert!(upgraded, "background retry should cache a fanart render");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Give the (non-atomic) file write and memory-cache insert a moment to finish.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (now, _) = poster_request(&state).await;
    assert_ne!(now, fallback, "subsequent requests get the fanart render");
}

#[tokio::test]
async fn logo_download_failure_is_not_cached_as_missing_art() {
    let (url, hits) = fanart_asset_server(Upstream::StallMidBody).await;
    let (state, _dir) = setup(&url, Duration::from_millis(600), Duration::from_millis(200)).await;
    // No TMDB logos either, so the request ends in a 404 after the fanart attempt.
    state
        .tmdb_images_cache
        .insert(
            format!("movie:{TMDB_ID}:en"),
            Arc::new(openposterdb_api::services::tmdb::TmdbImagesResponse {
                backdrops: vec![],
                logos: vec![],
                posters: vec![],
            }),
        )
        .await;

    let settings = fanart_settings();
    let start = Instant::now();
    let res = serve::handle_logo_backdrop_inner(
        &state, "imdb", &format!("{ID}.png"), &settings, serve::LogoBackdropKind::Logo, None,
    )
    .await;
    assert!(res.is_err());
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(hits.load(Ordering::SeqCst) >= 1);

    state.fanart_failed.run_pending_tasks().await;
    assert!(state.fanart_failed.get(&format!("imdb/{ID}_l_f_err")).await.is_some());
    assert!(
        state.fanart_negative.get(&format!("imdb/{ID}_l_f_en_neg")).await.is_none(),
        "a failed download must not be recorded as 'no fanart logo'"
    );

    // A repeat request within the failure TTL skips fanart entirely.
    let attempts = hits.load(Ordering::SeqCst);
    state.image_inflight.invalidate_all();
    let _ = serve::handle_logo_backdrop_inner(
        &state, "imdb", &format!("{ID}.png"), &settings, serve::LogoBackdropKind::Logo, None,
    )
    .await;
    assert_eq!(hits.load(Ordering::SeqCst), attempts);
}

// --- review follow-ups ---------------------------------------------------

fn leak_jpeg(r: u8, g: u8, b: u8) -> &'static [u8] {
    Box::leak(jpeg(r, g, b).into_boxed_slice())
}

/// The settings/ratings cache suffix, learned from the fallback file name.
fn fallback_suffix(state: &AppState) -> String {
    let files = poster_files(state, &format!("{ID}_f_fallback"));
    assert_eq!(files.len(), 1, "expected one fallback render: {files:?}");
    files[0]
        .strip_prefix(&format!("{ID}_f_fallback"))
        .unwrap()
        .strip_suffix(".jpg")
        .unwrap()
        .to_string()
}

fn poster_file_path(state: &AppState, name: &str) -> std::path::PathBuf {
    std::path::Path::new(&state.config.cache_dir).join("posters").join("imdb").join(name)
}

/// Produce the fallback render without contacting fanart (failure pre-recorded),
/// then clear in-memory state so later requests start from disk.
async fn seed_fallback(state: &AppState) {
    state.fanart_failed.insert(format!("imdb/{ID}_f_err"), ()).await;
    let _ = poster_request(state).await;
    state.fanart_failed.invalidate_all();
    state.image_mem_cache.invalidate_all();
    state.image_inflight.invalidate_all();
    state.refresh_locks.invalidate_all();
}

/// Write a cached `_f_en` render with known bytes and an mtime old enough to
/// be stale (no image_meta row → default 1-day staleness).
fn write_stale_fanart_render(state: &AppState, suffix: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = poster_file_path(state, &format!("{ID}_f_en{suffix}.jpg"));
    std::fs::write(&path, bytes).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(3 * 86400);
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
    path
}

#[tokio::test]
async fn variant_purge_within_inflight_ttl_rerenders() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let (url, _hits) = fanart_asset_server(Upstream::Jpeg(leak_jpeg(30, 30, 200))).await;
    let (state, _dir) = setup(&url, Duration::from_secs(5), Duration::from_secs(2)).await;
    let app = openposterdb_api::build_app(state.clone());
    let token = common::setup_admin(&app).await;

    // Cold request renders the fanart poster (step 3, coalesced via image_inflight).
    let _ = poster_request(&state).await;
    let files = poster_files(&state, &format!("{ID}_f_en"));
    assert_eq!(files.len(), 1);
    let cache_value = files[0].strip_suffix(".jpg").unwrap().to_string();

    // Purge just that variant, well inside image_inflight's 30s TTL.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/admin/posters/imdb/{}?scope=variant", cache_value.replace('@', "%40")))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(poster_files(&state, &format!("{ID}_f_en")).is_empty());

    // The next request must re-render and re-cache, not replay the purged bytes.
    let _ = poster_request(&state).await;
    assert_eq!(
        poster_files(&state, &format!("{ID}_f_en")).len(),
        1,
        "purged variant should be re-rendered and re-cached"
    );
    assert!(state.image_mem_cache.get(&format!("imdb/{cache_value}")).await.is_some());
}

#[tokio::test]
async fn permanent_asset_error_is_cached_as_no_art() {
    let (url, hits) = fanart_asset_server(Upstream::Status(404)).await;
    let (state, _dir) = setup(&url, Duration::from_secs(2), Duration::from_secs(1)).await;

    let _ = poster_request(&state).await;
    assert_eq!(hits.load(Ordering::SeqCst), 1, "404 is not retried");
    state.fanart_negative.run_pending_tasks().await;
    assert!(
        state.fanart_negative.get(&format!("imdb/{ID}_f_en_neg")).await.is_some(),
        "a dead asset URL is long-negative-cached as no art"
    );
    assert!(state.fanart_failed.get(&format!("imdb/{ID}_f_err")).await.is_none());
}

#[tokio::test]
async fn fanart_failure_recorded_when_tmdb_fallback_also_fails() {
    let (url, hits) = fanart_asset_server(Upstream::StallMidBody).await;
    let (state, _dir) = setup(&url, Duration::from_millis(600), Duration::from_millis(200)).await;
    // TMDB CDN fails too: no base poster on disk, and downloads cut off fast.
    let mut st = (*state).clone();
    st.tmdb = openposterdb_api::services::tmdb::TmdbClient::new("test".into(), reqwest::Client::new())
        .with_image_limits(ImageDownloadLimits::new(Duration::from_millis(300), Duration::from_millis(300), 4));
    let state = Arc::new(st);
    let other = "tt7777778";
    state
        .id_cache
        .insert(
            format!("imdb/{other}"),
            ResolvedId {
                imdb_id: Some(other.into()),
                tmdb_id: TMDB_ID,
                tvdb_id: None,
                media_type: MediaType::Movie,
                poster_path: Some("/opdb-test-does-not-exist-91c3.jpg".into()),
                release_date: Some("2000-01-01".into()),
                episode: None,
            },
        )
        .await;

    let other_jpg = format!("{other}.jpg");
    let req = || serve::handle_inner(&state, "imdb", &other_jpg, fanart_settings(), None);
    assert!(req().await.is_err(), "both sources failed");
    assert!(hits.load(Ordering::SeqCst) >= 1);
    state.fanart_failed.run_pending_tasks().await;
    assert!(
        state.fanart_failed.get(&format!("imdb/{other}_f_err")).await.is_some(),
        "fanart failure must be recorded even though the TMDB fallback failed"
    );

    let attempts = hits.load(Ordering::SeqCst);
    assert!(req().await.is_err());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(hits.load(Ordering::SeqCst), attempts, "second request must not contact fanart");
}

#[tokio::test]
async fn stale_fanart_render_kept_when_fanart_fails() {
    let (url, hits) = fanart_asset_server(Upstream::StallMidBody).await;
    let (state, _dir) = setup(&url, Duration::from_millis(600), Duration::from_millis(200)).await;
    seed_fallback(&state).await;
    let suffix = fallback_suffix(&state);
    let cached = jpeg(10, 200, 10);
    let path = write_stale_fanart_render(&state, &suffix, &cached);

    // Stale fanart render is served; the refresh runs in the background.
    let (served, elapsed) = poster_request(&state).await;
    assert_eq!(&served[..], &cached[..]);
    assert!(elapsed < Duration::from_millis(500), "stale hit waited: {elapsed:?}");

    let key = format!("imdb/{ID}_f_err");
    let mut recorded = false;
    for _ in 0..120 {
        if state.fanart_failed.get(&key).await.is_some() {
            recorded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(recorded, "refresh should record the fanart failure");
    assert!(hits.load(Ordering::SeqCst) >= 1);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(std::fs::read(&path).unwrap(), cached, "fanart render must not be overwritten with TMDB art");

    // While the failure is recorded, a stale hit doesn't contact fanart again.
    let attempts = hits.load(Ordering::SeqCst);
    state.image_mem_cache.invalidate_all();
    let (served, _) = poster_request(&state).await;
    assert_eq!(&served[..], &cached[..]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(hits.load(Ordering::SeqCst), attempts);
    assert_eq!(std::fs::read(&path).unwrap(), cached);
}

#[tokio::test]
async fn stale_fanart_render_refreshed_when_fanart_found() {
    let (url, hits) = fanart_asset_server(Upstream::Jpeg(leak_jpeg(30, 30, 200))).await;
    let (state, _dir) = setup(&url, Duration::from_secs(5), Duration::from_secs(2)).await;
    seed_fallback(&state).await;
    let suffix = fallback_suffix(&state);
    let cached = jpeg(10, 200, 10);
    let path = write_stale_fanart_render(&state, &suffix, &cached);

    let (served, _) = poster_request(&state).await;
    assert_eq!(&served[..], &cached[..]);

    let refreshed = wait_until(Duration::from_secs(10), || {
        std::fs::read(&path).map(|b| b != cached).unwrap_or(false)
    })
    .await;
    assert!(refreshed, "stale fanart render should be re-rendered from fanart");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(state.fanart_failed.get(&format!("imdb/{ID}_f_err")).await.is_none());
}
