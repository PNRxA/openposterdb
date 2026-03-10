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

// --- Global settings endpoints ---

#[tokio::test]
async fn settings_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_settings_returns_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "tmdb");
    assert_eq!(json["fanart_lang"], "en");
    assert_eq!(json["fanart_textless"], false);
    assert_eq!(json["fanart_available"], true);
}

#[tokio::test]
async fn update_settings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Update
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "poster_source": "fanart",
                "fanart_lang": "de",
                "fanart_textless": true
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read back
    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "fanart");
    assert_eq!(json["fanart_lang"], "de");
    assert_eq!(json["fanart_textless"], true);
}

#[tokio::test]
async fn update_settings_rejects_invalid_source() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "poster_source": "invalid",
                "fanart_lang": "en"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_settings_rejects_invalid_lang() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "poster_source": "fanart",
                "fanart_lang": "../../etc"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
