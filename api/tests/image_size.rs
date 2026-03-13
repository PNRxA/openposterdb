mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Helper: create an admin and API key, return the raw key.
async fn create_api_key(app: &axum::Router) -> String {
    let token = common::setup_admin(app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(r#"{"name":"size-test"}"#))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["key"].as_str().unwrap().to_string()
}

// --- Poster imageSize validation ---

#[tokio::test]
async fn poster_valid_image_size_medium_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?imageSize=medium"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Should not be 400 — the imageSize is valid.
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_valid_image_size_large_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?imageSize=large"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_valid_image_size_very_large_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?imageSize=very-large"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_small_image_size_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?imageSize=small"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_image_size_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?imageSize=huge"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_no_image_size_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    // Without imageSize param — should default to medium, no 400
    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Logo imageSize validation ---

#[tokio::test]
async fn logo_small_image_size_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?imageSize=small"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logo_valid_image_size_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?imageSize=large"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Backdrop imageSize validation ---

#[tokio::test]
async fn backdrop_small_image_size_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?imageSize=small"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // small is valid for backdrops, so no 400
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_invalid_image_size_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?imageSize=gigantic"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
