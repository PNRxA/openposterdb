mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json;
use tower::ServiceExt;

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn preview_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/preview/poster")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn preview_returns_jpeg_with_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=60"
    );

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100, "JPEG should have substantial content");
    // JPEG magic bytes
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_respects_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Request with limit=1 — should produce a smaller image (fewer badges)
    let res_small = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1", &token)).await.unwrap();
    assert_eq!(res_small.status(), StatusCode::OK);
    let body_small = res_small.into_body().collect().await.unwrap().to_bytes();

    // Request with limit=0 (show all 8 badges)
    let res_all = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=0", &token)).await.unwrap();
    assert_eq!(res_all.status(), StatusCode::OK);
    let body_all = res_all.into_body().collect().await.unwrap().to_bytes();

    // Both should be valid JPEGs
    assert_eq!(body_small[0], 0xFF);
    assert_eq!(body_all[0], 0xFF);

    // Both are valid and non-empty
    assert!(body_small.len() > 100);
    assert!(body_all.len() > 100);
}

#[tokio::test]
async fn preview_respects_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Two different badge selections should produce different images
    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,tmdb", &token)).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=rt,mc", &token)).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Both valid JPEGs
    assert_eq!(body1[0], 0xFF);
    assert_eq!(body2[0], 0xFF);

    // Different badge selections should produce different image bytes
    assert_ne!(body1, body2);
}

#[tokio::test]
async fn preview_with_empty_order_still_works() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_order=&ratings_limit=3", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_cache_returns_identical_bytes_for_same_params() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let uri = "/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,tmdb";

    let res1 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Second request should return identical bytes from cache
    assert_eq!(body1, body2);
}

#[tokio::test]
async fn preview_cache_differs_for_different_params() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1&ratings_order=imdb", &token)).await.unwrap();
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1&ratings_order=rt", &token)).await.unwrap();
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Different rating params should produce different images (different cache keys)
    assert_ne!(body1, body2);
}

#[tokio::test]
async fn preview_cache_populates_entry_count() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    assert_eq!(state.preview_cache.entry_count(), 0);

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Force pending tasks to run so moka registers the insert
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1);
}

#[tokio::test]
async fn preview_cache_survives_settings_update() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Warm the preview cache
    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    state.preview_cache.run_pending_tasks().await;
    assert!(state.preview_cache.entry_count() > 0, "cache should be populated");

    // Update settings — cache keys encode the config, so no invalidation needed
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "poster_source": "tmdb",
                "ratings_limit": 5,
                "ratings_order": "imdb,rt,mc,tmdb,trakt,mal,lb,rta"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Cache should still have the original entry — different settings = different key
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1, "existing cache entry should survive settings update");
}

#[tokio::test]
async fn preview_serves_from_filesystem_after_memory_eviction() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let uri = "/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt";

    // First request — renders and writes to both memory + filesystem
    let res = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body1 = res.into_body().collect().await.unwrap().to_bytes();

    // Evict from memory cache to simulate TTL expiry
    state.preview_cache.invalidate_all();
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 0);

    // Second request — should serve from filesystem, not re-render
    let res = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body2 = res.into_body().collect().await.unwrap().to_bytes();

    // Should return identical bytes from filesystem
    assert_eq!(body1, body2);

    // Memory cache should be re-populated from filesystem
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1);
}

#[tokio::test]
async fn preview_accessible_via_self_serve_auth() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/poster")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_respects_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?poster_position=l", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100);
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_cache_differs_for_different_positions() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt&poster_position=bc", &token)).await.unwrap();
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt&poster_position=l", &token)).await.unwrap();
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body1[0], 0xFF);
    assert_eq!(body2[0], 0xFF);
    assert_ne!(body1, body2);
}

#[tokio::test]
async fn preview_rejects_invalid_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?poster_position=invalid", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
