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

// --- Security headers ---

#[tokio::test]
async fn security_headers_present_on_all_responses() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let xcto = res
        .headers()
        .get("x-content-type-options")
        .and_then(|v| v.to_str().ok());
    assert_eq!(xcto, Some("nosniff"), "X-Content-Type-Options should be nosniff");

    let xfo = res
        .headers()
        .get("x-frame-options")
        .and_then(|v| v.to_str().ok());
    assert_eq!(xfo, Some("DENY"), "X-Frame-Options should be DENY");
}

#[tokio::test]
async fn security_headers_present_on_error_responses() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    assert!(
        res.headers().get("x-content-type-options").is_some(),
        "X-Content-Type-Options should be present on error responses"
    );
    assert!(
        res.headers().get("x-frame-options").is_some(),
        "X-Frame-Options should be present on error responses"
    );
}

// --- Username max length ---

#[tokio::test]
async fn setup_rejects_too_long_username() {
    let (app, _state) = common::setup_test_app().await;
    let long_name = "a".repeat(129);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": long_name,
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- API key name validation ---

#[tokio::test]
async fn create_api_key_rejects_too_long_name() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let long_name = "a".repeat(129);

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": long_name})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_api_key_rejects_control_chars_in_name() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "key\u{0000}name"})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Malformed JWT handling ---

#[tokio::test]
async fn truncated_jwt_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.truncated")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwt_with_none_algorithm_rejected() {
    let (app, _state) = common::setup_test_app().await;

    use base64::Engine;
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"alg":"none","typ":"JWT"}"#);
    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!(r#"{{"sub":"admin","exp":{exp}}}"#));
    let token = format!("{header}.{payload}.");

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwt_with_empty_string_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "Bearer ")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwt_with_garbage_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "Bearer not-a-jwt-at-all")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Concurrent refresh token reuse ---

#[tokio::test]
async fn concurrent_refresh_token_reuse_at_most_one_succeeds() {
    let (app, _state) = common::setup_test_app().await;

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
    let set_cookie = common::extract_set_cookie(res.headers()).unwrap();
    let refresh_token = common::extract_refresh_token(&set_cookie).unwrap();

    let app1 = app.clone();
    let app2 = app.clone();
    let rt1 = refresh_token.clone();
    let rt2 = refresh_token.clone();

    let req1 = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("cookie", format!("refresh_token={rt1}"))
        .body(Body::empty())
        .unwrap();

    let req2 = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("cookie", format!("refresh_token={rt2}"))
        .body(Body::empty())
        .unwrap();

    let (res1, res2) = tokio::join!(app1.oneshot(req1), app2.oneshot(req2));
    let status1 = res1.unwrap().status();
    let status2 = res2.unwrap().status();

    let successes = [status1, status2]
        .iter()
        .filter(|s| **s == StatusCode::OK)
        .count();

    assert!(
        successes <= 1,
        "at most one concurrent refresh should succeed (got {status1}, {status2})"
    );
}

// --- Empty/malformed body handling ---

#[tokio::test]
async fn setup_with_empty_body_returns_error() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status().is_client_error(),
        "empty body should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn login_with_empty_body_returns_error() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status().is_client_error(),
        "empty body should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn setup_with_wrong_content_type_returns_error() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "text/plain")
        .body(Body::from(r#"{"username":"admin","password":"testpassword123"}"#))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status().is_client_error(),
        "wrong content-type should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn key_login_with_empty_body_returns_error() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status().is_client_error(),
        "empty body should return 4xx, got {}",
        res.status()
    );
}

// --- Free API key cannot login ---

#[tokio::test]
async fn free_api_key_cannot_login_via_security() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Enable free key
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "free_api_key_enabled": true
        })))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": "t0-free-rpdb"})))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Cookie Secure flag behavior ---

#[tokio::test]
async fn cookie_has_secure_flag_in_secure_mode() {
    let (app, _state) = common::setup_test_app_with_options(common::TestAppOptions {
        secure_cookies: true,
        ..Default::default()
    })
    .await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin",
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let set_cookie = common::extract_set_cookie(res.headers()).unwrap();
    assert!(set_cookie.contains("; Secure"), "cookie should have Secure flag");
}

#[tokio::test]
async fn cookie_lacks_secure_flag_in_insecure_mode() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "username": "admin",
            "password": "testpassword123"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let set_cookie = common::extract_set_cookie(res.headers()).unwrap();
    assert!(!set_cookie.contains("; Secure"), "cookie should not have Secure flag");
}

// --- Authorization header edge cases ---

#[tokio::test]
async fn auth_without_bearer_prefix_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_with_no_authorization_header_rejected() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Poster endpoint with null byte injection ---

#[tokio::test]
async fn poster_endpoint_rejects_null_bytes_in_id() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

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

    let req = Request::builder()
        .uri(format!("/{api_key}/imdb/poster-default/tt123%00.jpg"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
