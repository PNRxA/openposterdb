mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

async fn setup_cdn_app() -> (axum::Router, std::sync::Arc<openposterdb_api::AppState>) {
    common::setup_test_app_with_options(common::TestAppOptions {
        enable_cdn_redirects: true,
        ..Default::default()
    })
    .await
}

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

// --- Redirect behavior when CDN redirects are enabled ---

#[tokio::test]
async fn poster_returns_302_when_cdn_enabled() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-poster").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);

    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/c/"), "redirect should point to /c/ URL: {location}");
    assert!(location.contains("/imdb/poster-default/tt0111161.jpg"), "redirect should preserve path: {location}");
}

#[tokio::test]
async fn logo_returns_302_when_cdn_enabled() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-logo").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/logo-default/tt0111161.png"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);

    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/c/"));
    assert!(location.contains("/imdb/logo-default/tt0111161.png"));
}

#[tokio::test]
async fn backdrop_returns_302_when_cdn_enabled() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-backdrop").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/backdrop-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);

    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/c/"));
    assert!(location.contains("/imdb/backdrop-default/tt0111161.jpg"));
}

// --- Redirect cache headers ---

#[tokio::test]
async fn redirect_has_private_cache_control() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-cc").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);
    let cc = res.headers().get("cache-control").unwrap().to_str().unwrap();
    assert!(cc.contains("private"), "redirect cache-control should be private: {cc}");
    assert!(cc.contains("max-age=300"), "redirect cache-control should have max-age=300: {cc}");
}

// --- Query parameter forwarding ---

#[tokio::test]
async fn redirect_forwards_fallback_param() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-fallback").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("?fallback=true"), "redirect should forward ?fallback=true: {location}");
}

#[tokio::test]
async fn redirect_does_not_forward_lang_param() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-lang").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?lang=de"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(!location.contains("lang="), "redirect should NOT forward ?lang= (encoded in hash): {location}");
}

// --- No redirect when CDN disabled ---

#[tokio::test]
async fn poster_does_not_redirect_when_cdn_disabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "no-cdn").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Should NOT be 302 — should either serve or error, not redirect
    assert_ne!(res.status(), StatusCode::FOUND, "should not redirect when CDN disabled");
}

// --- Same settings produce same redirect target ---

#[tokio::test]
async fn same_settings_same_redirect_target() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let key1 = create_api_key(&app, &token, "cdn-same-1").await;
    let key2 = create_api_key(&app, &token, "cdn-same-2").await;

    let req1 = Request::builder()
        .uri(format!("/{key1}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();
    let res1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(res1.status(), StatusCode::FOUND);
    let loc1 = res1.headers().get("location").unwrap().to_str().unwrap().to_string();

    let req2 = Request::builder()
        .uri(format!("/{key2}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::FOUND);
    let loc2 = res2.headers().get("location").unwrap().to_str().unwrap().to_string();

    assert_eq!(loc1, loc2, "two keys with default settings should redirect to same URL");
}

// --- /c/ endpoint: unknown hash returns 404 ---

#[tokio::test]
async fn cdn_endpoint_unknown_hash_returns_404() {
    let (app, _state) = setup_cdn_app().await;

    let req = Request::builder()
        .uri("/c/deadbeef1234/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn cdn_logo_endpoint_unknown_hash_returns_404() {
    let (app, _state) = setup_cdn_app().await;

    let req = Request::builder()
        .uri("/c/deadbeef1234/imdb/logo-default/tt0111161.png")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cdn_backdrop_endpoint_unknown_hash_returns_404() {
    let (app, _state) = setup_cdn_app().await;

    let req = Request::builder()
        .uri("/c/deadbeef1234/imdb/backdrop-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// --- /c/ endpoint: registered hash serves with CDN cache headers ---

#[tokio::test]
async fn cdn_endpoint_registered_hash_has_cdn_cache_headers() {
    let (app, state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-headers").await;

    // First request: get the redirect to learn the hash
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("location").unwrap().to_str().unwrap().to_string();

    // Ensure the moka cache has flushed
    state.settings_hash_registry.run_pending_tasks().await;

    // Follow the redirect (with fallback so we get 200 even with fake TMDB key)
    let req = Request::builder()
        .uri(&location)
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // With fallback=true, should get 200 (placeholder) even if generation fails
    assert_eq!(res.status(), StatusCode::OK);

    let cc = res.headers().get("cache-control").unwrap().to_str().unwrap();
    assert!(cc.contains("public"), "CDN response should have public cache-control: {cc}");
    assert!(cc.contains("max-age=86400"), "CDN response should have max-age=86400: {cc}");
    assert!(cc.contains("stale-while-revalidate=604800"), "CDN response should have s-w-r: {cc}");
}

// --- Free API key also redirects when CDN enabled ---

#[tokio::test]
async fn free_key_redirects_when_cdn_enabled() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/c/"));
}

// --- isValid is not affected by CDN redirects ---

#[tokio::test]
async fn is_valid_not_affected_by_cdn_redirects() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-isvalid").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/isValid"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "isValid should return 200, not redirect");
}

// --- Lang override produces different hash ---

#[tokio::test]
async fn lang_override_produces_different_redirect_target() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-lang-diff").await;

    let req1 = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();
    let res1 = app.clone().oneshot(req1).await.unwrap();
    let loc1 = res1.headers().get("location").unwrap().to_str().unwrap().to_string();

    let req2 = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?lang=de"))
        .body(Body::empty())
        .unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    let loc2 = res2.headers().get("location").unwrap().to_str().unwrap().to_string();

    assert_ne!(loc1, loc2, "?lang= override should produce a different hash");
}

// --- Redirect hash is 12 hex chars ---

#[tokio::test]
async fn redirect_hash_is_12_hex_chars() {
    let (app, _state) = setup_cdn_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "cdn-hashlen").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    // /c/{32_hex_chars}/imdb/...
    let hash = location.strip_prefix("/c/").unwrap().split('/').next().unwrap();
    assert_eq!(hash.len(), 32, "hash should be 32 chars: {hash}");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "hash should be hex: {hash}");
}
