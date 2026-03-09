mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn stats_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/stats")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stats_returns_counts() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/stats")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["total_posters"], 0);
    assert_eq!(json["total_api_keys"], 0);
    assert!(json["mem_cache_entries"].is_number());
    assert!(json["id_cache_entries"].is_number());
    assert!(json["ratings_cache_entries"].is_number());
    assert!(json["poster_mem_cache_mb"].is_number());
}

#[tokio::test]
async fn list_posters_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/posters")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_posters_returns_empty() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/posters?page=1&page_size=10")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["items"].as_array().unwrap().len(), 0);
    assert_eq!(json["total"], 0);
    assert_eq!(json["page"], 1);
    assert_eq!(json["page_size"], 10);
}

#[tokio::test]
async fn list_posters_default_pagination() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/posters")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 1);
    assert_eq!(json["page_size"], 50);
}

#[tokio::test]
async fn stats_reflects_api_key_count() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create an API key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({"name": "test-key"}).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Check stats
    let req = Request::builder()
        .uri("/api/admin/stats")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["total_api_keys"], 1);
}

#[tokio::test]
async fn poster_image_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/posters/imdb/tt0111161/image")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
