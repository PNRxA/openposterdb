mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

/// Helper: create an API key and return the raw key string.
async fn create_api_key(app: &axum::Router, token: &str, name: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": name})))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["key"].as_str().unwrap().to_string()
}

/// Helper: enable the free API key setting.
async fn set_free_api_key_enabled(app: &axum::Router, token: &str, enabled: bool) {
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "free_api_key_enabled": enabled
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- Logo endpoint auth ---

#[tokio::test]
async fn logo_endpoint_rejects_invalid_api_key() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/bogus-key/imdb/logo-default/tt0000001.png")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logo_endpoint_accepts_valid_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-test").await;

    // The fanart call will fail (fake key), but should NOT be 401
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0000001.png"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "valid API key should not return 401");
}

// --- Backdrop endpoint auth ---

#[tokio::test]
async fn backdrop_endpoint_rejects_invalid_api_key() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/bogus-key/imdb/backdrop-default/tt0000001.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn backdrop_endpoint_accepts_valid_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-test").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "valid API key should not return 401");
}

// --- Free API key on logo/backdrop ---

#[tokio::test]
async fn logo_free_key_rejected_when_disabled() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/logo-default/tt0111161.png")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logo_free_key_accepted_when_enabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/logo-default/tt0111161.png")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "free key should not return 401 when enabled");
}

#[tokio::test]
async fn backdrop_free_key_rejected_when_disabled() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/backdrop-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn backdrop_free_key_accepted_when_enabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/backdrop-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "free key should not return 401 when enabled");
}

// --- Free key does not track last_used for logo/backdrop ---

#[tokio::test]
async fn logo_free_key_does_not_track_last_used() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/logo-default/tt0111161.png")
        .body(Body::empty())
        .unwrap();
    let _res = app.oneshot(req).await.unwrap();

    assert!(
        state.pending_last_used.is_empty(),
        "free API key should not track last_used for logo"
    );
}

#[tokio::test]
async fn backdrop_free_key_does_not_track_last_used() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/backdrop-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let _res = app.oneshot(req).await.unwrap();

    assert!(
        state.pending_last_used.is_empty(),
        "free API key should not track last_used for backdrop"
    );
}

// --- Logo/backdrop track last_used for regular keys ---

#[tokio::test]
async fn logo_tracks_last_used_for_regular_key() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-track").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0000001.png"))
        .body(Body::empty())
        .unwrap();
    let _res = app.oneshot(req).await.unwrap();

    assert!(
        !state.pending_last_used.is_empty(),
        "regular API key should track last_used for logo"
    );
}

#[tokio::test]
async fn backdrop_tracks_last_used_for_regular_key() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-track").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();
    let _res = app.oneshot(req).await.unwrap();

    assert!(
        !state.pending_last_used.is_empty(),
        "regular API key should track last_used for backdrop"
    );
}

// --- Fallback returns placeholder images ---

#[tokio::test]
async fn logo_fallback_returns_png_placeholder() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-fallback").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0000001.png?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/png"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(!body.is_empty());
    // PNG magic bytes
    assert_eq!(&body[..4], &[0x89, b'P', b'N', b'G']);
}

#[tokio::test]
async fn backdrop_fallback_returns_jpeg_placeholder() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-fallback").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/jpeg"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(!body.is_empty());
    // JPEG magic bytes
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

// --- Without fallback, errors return non-200 ---

#[tokio::test]
async fn logo_no_fallback_returns_error() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-nofallback").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0000001.png"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Without fallback, should get an error status (not 200, not 401)
    assert_ne!(res.status(), StatusCode::OK);
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn backdrop_no_fallback_returns_error() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-nofallback").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::OK);
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Negative cache prevents repeated lookups ---

#[tokio::test]
async fn logo_negative_cache_short_circuits_request() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-neg").await;

    // Pre-populate the negative cache for this ID's logo
    // Key format: "{id_type}/{id_value}{kind_prefix}_f_{lang}_neg"
    state.fanart_negative.insert("imdb/tt9999999_l_f_en_neg".to_string(), ()).await;
    state.fanart_negative.run_pending_tasks().await;

    // Request should fail immediately via the negative cache check.
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt9999999.png"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn backdrop_negative_cache_short_circuits_request() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-neg").await;

    // Key format: "{id_type}/{id_value}{kind_prefix}_f_{lang}_neg" (backdrop lang is empty)
    state.fanart_negative.insert("imdb/tt9999999_b_f__neg".to_string(), ()).await;
    state.fanart_negative.run_pending_tasks().await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt9999999.jpg"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn logo_negative_cache_with_fallback_returns_placeholder() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-neg-fb").await;

    state.fanart_negative.insert("imdb/tt9999999_l_f_en_neg".to_string(), ()).await;
    state.fanart_negative.run_pending_tasks().await;

    // With fallback=true, negative cache miss should still return placeholder PNG
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt9999999.png?fallback=true"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
}

// --- Language override on logo/backdrop ---

#[tokio::test]
async fn logo_with_valid_lang_param_accepted() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-lang").await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?lang=de"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Should not be 400 (bad request) — the lang param is valid.
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logo_with_invalid_lang_param_rejected() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-lang-bad").await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?lang=toolongvalue"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_with_valid_lang_param_accepted() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-lang").await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?lang=de"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn backdrop_with_invalid_lang_param_rejected() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-lang-bad").await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?lang=toolongvalue"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Invalid id_type returns 400 ---

#[tokio::test]
async fn logo_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-badtype").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/invalid_type/logo-default/tt0000001.png"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-badtype").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/invalid_type/backdrop-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Cache-Control header is set ---

#[tokio::test]
async fn logo_fallback_has_cache_control_header() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "logo-cc").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0000001.png?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cc = res.headers().get("cache-control").unwrap().to_str().unwrap();
    assert!(cc.contains("public"), "should have public cache-control");
    assert!(cc.contains("max-age="), "should have max-age");
}

#[tokio::test]
async fn backdrop_fallback_has_cache_control_header() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "backdrop-cc").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cc = res.headers().get("cache-control").unwrap().to_str().unwrap();
    assert!(cc.contains("public"), "should have public cache-control");
    assert!(cc.contains("max-age="), "should have max-age");
}
