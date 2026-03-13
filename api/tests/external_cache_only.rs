mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

async fn setup_external_cache_app() -> (axum::Router, std::sync::Arc<openposterdb_api::AppState>) {
    common::setup_test_app_with_options(common::TestAppOptions {
        external_cache_only: true,
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

/// Poster request with fallback=true should succeed even with external_cache_only
/// (placeholder image returned since we have a fake TMDB key).
#[tokio::test]
async fn poster_with_fallback_succeeds() {
    let (app, _state) = setup_external_cache_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "ext-cache-poster").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("image/"), "should return an image: {ct}");
}

/// No files should be written to the cache directory when external_cache_only is enabled.
#[tokio::test]
async fn no_files_written_to_cache_dir() {
    // Use a unique path that doesn't exist yet
    let cache_dir = format!("/tmp/openposterdb-test-ext-cache-{}", std::process::id());
    // Ensure it doesn't exist from a previous run
    let _ = tokio::fs::remove_dir_all(&cache_dir).await;

    let (app, _state) = common::setup_test_app_with_options(common::TestAppOptions {
        external_cache_only: true,
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;

    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "ext-cache-no-files").await;

    // Make a poster request (with fallback so it doesn't error)
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0111161.jpg?fallback=true"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Cache directory should not have been created
    assert!(
        !std::path::Path::new(&cache_dir).exists(),
        "cache directory should not exist when external_cache_only is enabled"
    );
}

/// isValid should still work with external_cache_only.
#[tokio::test]
async fn is_valid_works_with_external_cache_only() {
    let (app, _state) = setup_external_cache_app().await;
    let token = common::setup_admin(&app).await;
    let api_key = create_api_key(&app, &token, "ext-cache-valid").await;

    let req = Request::builder()
        .uri(format!("/{api_key}/isValid"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

/// Auth and API key management should work normally with external_cache_only.
#[tokio::test]
async fn admin_operations_work_with_external_cache_only() {
    let (app, _state) = setup_external_cache_app().await;
    let token = common::setup_admin(&app).await;

    // Create and list API keys
    let _api_key = create_api_key(&app, &token, "ext-cache-admin").await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().len() >= 1);
}
