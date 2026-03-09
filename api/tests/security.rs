mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sea_orm::ConnectionTrait;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

// --- CORS origin enforcement (via preflight OPTIONS requests) ---

#[tokio::test]
async fn cors_preflight_without_origin_config_rejected() {
    // No CORS origin configured — CorsLayer::new() rejects all cross-origin
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/setup")
        .header("origin", "https://example.com")
        .header("access-control-request-method", "POST")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // No Access-Control-Allow-Origin header should be present
    assert!(res.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn cors_preflight_with_correct_origin_allowed() {
    let (app, _state) =
        common::setup_test_app_with_cors(Some("https://example.com".into())).await;

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/setup")
        .header("origin", "https://example.com")
        .header("access-control-request-method", "POST")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    let acao = res
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_eq!(acao, Some("https://example.com"));
}

#[tokio::test]
async fn cors_preflight_with_wrong_origin_rejected() {
    let (app, _state) =
        common::setup_test_app_with_cors(Some("https://example.com".into())).await;

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/setup")
        .header("origin", "https://evil.com")
        .header("access-control-request-method", "POST")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // CorsLayer echoes the configured origin, not the request origin.
    // The browser compares and rejects when they don't match.
    let acao = res
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_ne!(acao, Some("https://evil.com"), "should not allow wrong origin");
}

#[tokio::test]
async fn cors_get_allowed_even_with_cors_origin_set() {
    let (app, _state) =
        common::setup_test_app_with_cors(Some("https://example.com".into())).await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn cors_delete_without_origin_no_cors_headers() {
    let (app, _state) =
        common::setup_test_app_with_cors(Some("https://example.com".into())).await;

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/keys/1")
        .header("authorization", "Bearer fake")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Without matching origin, browser will block the response
    let acao = res
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_ne!(acao, Some("https://evil.com"));
}

// --- Refresh token expiry ---

#[tokio::test]
async fn refresh_with_expired_token_returns_401() {
    let (app, state) = common::setup_test_app().await;

    // Setup admin
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin",
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let set_cookie = common::extract_set_cookie(res.headers()).unwrap();
    let refresh_token = common::extract_refresh_token(&set_cookie).unwrap();

    // Manually expire all refresh tokens in the DB
    state
        .db
        .execute_unprepared(
            "UPDATE refresh_tokens SET expires_at = '2020-01-01 00:00:00'",
        )
        .await
        .unwrap();

    // Try to refresh with the expired token
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("cookie", format!("refresh_token={refresh_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Concurrent setup race condition ---

#[tokio::test]
async fn concurrent_setup_only_one_succeeds() {
    let (app, _state) = common::setup_test_app().await;

    let app1 = app.clone();
    let app2 = app.clone();

    let req1 = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin1",
            "password": "testpassword123"
        })))
        .unwrap();

    let req2 = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin2",
            "password": "testpassword123"
        })))
        .unwrap();

    let (res1, res2) = tokio::join!(app1.oneshot(req1), app2.oneshot(req2));
    let status1 = res1.unwrap().status();
    let status2 = res2.unwrap().status();

    // Exactly one should succeed, one should fail
    let successes = [status1, status2]
        .iter()
        .filter(|s| **s == StatusCode::OK)
        .count();
    let failures = [status1, status2]
        .iter()
        .filter(|s| **s == StatusCode::FORBIDDEN)
        .count();

    assert_eq!(successes, 1, "exactly one setup should succeed (got {status1}, {status2})");
    assert_eq!(failures, 1, "exactly one setup should be forbidden (got {status1}, {status2})");
}

// --- API key negative caching ---

#[tokio::test]
async fn invalid_api_key_cached_returns_401_consistently() {
    let (app, _state) = common::setup_test_app().await;

    // First request with invalid key
    let req = Request::builder()
        .uri("/bogus-key/imdb/poster-default/tt0000001.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Second request with same invalid key should also fail (from cache)
    let req = Request::builder()
        .uri("/bogus-key/imdb/poster-default/tt0000001.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Deleted API key is rejected after cache invalidation ---

#[tokio::test]
async fn deleted_api_key_rejected_after_cache_invalidation() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create an API key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "temp-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let api_key = json["key"].as_str().unwrap().to_string();
    let key_id = json["id"].as_i64().unwrap();

    // Use the API key (populates cache)
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

    // Delete the API key via the endpoint (which calls invalidate_all)
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/keys/{key_id}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Ensure cache eviction completes
    state.api_key_cache.run_pending_tasks().await;

    // Now the API key should be rejected
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Input validation on auth endpoints ---

#[tokio::test]
async fn setup_rejects_short_password() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin",
            "password": "short"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn setup_rejects_empty_username() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "",
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn setup_rejects_username_with_whitespace() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin user",
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Expired JWT token ---

#[tokio::test]
async fn expired_jwt_rejected() {
    let (app, state) = common::setup_test_app().await;

    let claims = serde_json::json!({
        "sub": "admin",
        "exp": 1000000000 // Way in the past (2001)
    });

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&state.jwt_secret),
    )
    .unwrap();

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- JWT signed with wrong secret ---

#[tokio::test]
async fn jwt_with_wrong_secret_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let wrong_secret = vec![0xCD; 32];
    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;

    let claims = serde_json::json!({
        "sub": "admin",
        "exp": exp
    });

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&wrong_secret),
    )
    .unwrap();

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Poster endpoint with invalid id type ---

#[tokio::test]
async fn poster_endpoint_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a valid API key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let api_key = json["key"].as_str().unwrap();

    // Use invalid id type
    let req = Request::builder()
        .uri(format!("/{api_key}/invalid_type/poster-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Refresh with fabricated token (not in DB) ---

#[tokio::test]
async fn refresh_with_fabricated_token_returns_401() {
    let (app, _state) = common::setup_test_app().await;
    common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("cookie", "refresh_token=aabbccdd00112233fabricatedtoken")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Delete nonexistent API key ---

#[tokio::test]
async fn delete_nonexistent_api_key_succeeds() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/keys/99999")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- Path traversal prevention ---

#[tokio::test]
async fn poster_endpoint_rejects_path_traversal() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a valid API key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let api_key = json["key"].as_str().unwrap();

    // Attempt path traversal in id_value
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/..%2F..%2Fetc%2Fpasswd.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Error redaction (integration) ---

#[tokio::test]
async fn internal_error_response_is_redacted() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a valid API key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let api_key = json["key"].as_str().unwrap();

    // Request a poster that will fail internally (fake TMDB key)
    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt0000001.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error_msg = json["error"].as_str().unwrap();
    // Must not leak internal details like URLs, connection errors, etc.
    assert_eq!(error_msg, "Internal server error");
}

// --- HSTS header ---

#[tokio::test]
async fn hsts_header_present_when_secure_cookies() {
    let (app, _state) = common::setup_test_app_with_options(common::TestAppOptions {
        secure_cookies: true,
        ..Default::default()
    })
    .await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let hsts = res
        .headers()
        .get("strict-transport-security")
        .and_then(|v| v.to_str().ok());
    assert_eq!(hsts, Some("max-age=63072000; includeSubDomains"));
}

#[tokio::test]
async fn hsts_header_absent_when_not_secure() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("strict-transport-security").is_none());
}

// --- Password max length (integration) ---

#[tokio::test]
async fn setup_rejects_too_long_password() {
    let (app, _state) = common::setup_test_app().await;
    let long_pw = "a".repeat(257);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin",
            "password": long_pw
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
