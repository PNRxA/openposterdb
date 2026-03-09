mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn auth_middleware_missing_header() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_middleware_invalid_token() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "Bearer invalid.jwt.token")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_middleware_missing_bearer_prefix() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "NotBearer sometoken")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_middleware_valid_token_passes() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn poster_endpoint_rejects_invalid_api_key() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/bogus-key/imdb/poster-default/tt0000001.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn poster_endpoint_accepts_valid_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create an API key via the admin endpoint
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(r#"{"name":"poster-test"}"#))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let api_key = json["key"].as_str().unwrap();

    // Use the API key on the poster endpoint — the TMDB call will fail since
    // we use a fake API key, but the response should NOT be 401 (key is valid).
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "valid API key should not return 401");
}

#[tokio::test]
async fn logout_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
