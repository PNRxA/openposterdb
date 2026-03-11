mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, EncodingKey, Header as JwtHeader};
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

/// Helper: set up admin, create an API key, return the raw key string.
async fn create_api_key(app: &axum::Router) -> String {
    let token = common::setup_admin(app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "self-test-key"})))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["key"].as_str().unwrap().to_string()
}

/// Helper: create an API key and log in with it, returning the session JWT.
async fn create_api_key_and_login(app: &axum::Router) -> (String, String) {
    let raw_key = create_api_key(app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": raw_key})))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_token = json["token"].as_str().unwrap().to_string();
    (raw_key, session_token)
}

// --- key-login endpoint ---

#[tokio::test]
async fn key_login_valid_key_returns_token_and_info() {
    let (app, _state) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": api_key})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "self-test-key");
    assert!(json["key_prefix"].is_string());
    // Should contain a JWT session token
    assert!(json["token"].is_string());
}

#[tokio::test]
async fn key_login_invalid_key_returns_401() {
    let (app, _state) = common::setup_test_app().await;
    // setup admin so the app is initialized, but use a bogus key
    common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": "bogus-invalid-key"})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- self-service endpoints require API key auth ---

#[tokio::test]
async fn self_endpoints_without_auth_return_401() {
    let (app, _state) = common::setup_test_app().await;

    for (method, uri) in [
        ("GET", "/api/key/me"),
        ("GET", "/api/key/me/settings"),
        ("PUT", "/api/key/me/settings"),
        ("DELETE", "/api/key/me/settings"),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} should require auth"
        );
    }
}

#[tokio::test]
async fn self_endpoints_reject_admin_jwt_token() {
    let (app, _state) = common::setup_test_app().await;
    let jwt_token = common::setup_admin(&app).await;

    // Admin JWT tokens should NOT work on API-key-auth endpoints
    // (they don't contain key_id claims)
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {jwt_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn self_endpoints_reject_raw_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let raw_key = create_api_key(&app).await;

    // Raw API keys should NOT work — must use session JWT from key-login
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {raw_key}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- GET /api/key/me ---

#[tokio::test]
async fn get_own_key_info_returns_name_and_prefix() {
    let (app, _state) = common::setup_test_app().await;
    let (raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "self-test-key");
    assert_eq!(json["key_prefix"], &raw_key[..8]);
}

// --- GET /api/key/me/settings ---

#[tokio::test]
async fn get_own_settings_returns_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "t");
    assert_eq!(json["fanart_lang"], "en");
    assert_eq!(json["fanart_textless"], false);
    assert_eq!(json["is_default"], true);
    assert_eq!(json["fanart_available"], true);
    assert_eq!(json["ratings_limit"], 3);
    assert_eq!(json["ratings_order"], "mal,imdb,lb,rt,mc,rta,tmdb,trakt");
}

// --- PUT /api/key/me/settings ---

#[tokio::test]
async fn update_own_settings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    // Update
    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "ja",
            "fanart_textless": true
        })))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read back
    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "f");
    assert_eq!(json["fanart_lang"], "ja");
    assert_eq!(json["fanart_textless"], true);
    assert_eq!(json["is_default"], false);
}

#[tokio::test]
async fn update_own_settings_rejects_invalid_source() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "invalid"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_own_settings_rejects_invalid_lang() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "x"
        })))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- DELETE /api/key/me/settings ---

#[tokio::test]
async fn reset_own_settings_restores_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    // Set custom settings
    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "de",
            "fanart_textless": true
        })))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    // Reset
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Should be back to defaults
    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "t");
    assert_eq!(json["is_default"], true);
}

// --- Security: API key cannot access admin routes ---

#[tokio::test]
async fn api_key_session_cannot_access_admin_routes() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    for (method, uri) in [
        ("GET", "/api/keys"),
        ("GET", "/api/admin/stats"),
        ("GET", "/api/admin/settings"),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {session_token}"))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "API key session should not access {method} {uri}"
        );
    }
}

// --- Security: expired API key session JWT ---

#[tokio::test]
async fn expired_api_key_session_jwt_rejected() {
    let (app, state) = common::setup_test_app().await;
    create_api_key(&app).await;

    // Craft an expired ApiKeyClaims JWT
    let claims = serde_json::json!({
        "key_id": 1,
        "exp": 1000000000 // Way in the past (2001)
    });
    let token = encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(&state.jwt_secret),
    )
    .unwrap();

    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Security: API key session JWT with wrong secret ---

#[tokio::test]
async fn api_key_session_jwt_wrong_secret_rejected() {
    let (app, _state) = common::setup_test_app().await;
    create_api_key(&app).await;

    let wrong_secret = vec![0xCD; 32];
    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;

    let claims = serde_json::json!({
        "key_id": 1,
        "exp": exp
    });
    let token = encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(&wrong_secret),
    )
    .unwrap();

    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Security: fabricated JWT with non-existent key_id ---

#[tokio::test]
async fn fabricated_key_id_in_jwt_rejected() {
    let (app, state) = common::setup_test_app().await;
    common::setup_admin(&app).await;

    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;
    let claims = serde_json::json!({
        "key_id": 999999,
        "exp": exp
    });
    let token = encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(&state.jwt_secret),
    )
    .unwrap();

    // The JWT is valid but key_id doesn't exist — handler should return error
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Security: deleted key's session JWT is rejected ---

#[tokio::test]
async fn deleted_key_session_jwt_rejected() {
    let (app, _state) = common::setup_test_app().await;
    let admin_token = common::setup_admin(&app).await;

    // Create a key and get a session JWT
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {admin_token}"))
        .body(json_body(serde_json::json!({"name": "doomed-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let raw_key = json["key"].as_str().unwrap().to_string();
    let key_id = json["id"].as_i64().unwrap();

    // Login with the key to get session JWT
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": raw_key})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_token = json["token"].as_str().unwrap().to_string();

    // Verify it works before deletion
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Delete the key via admin endpoint
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/keys/{key_id}"))
        .header("authorization", format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Session JWT should now fail (key no longer exists)
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Security: cross-key isolation ---

#[tokio::test]
async fn key_session_only_sees_own_settings() {
    let (app, _state) = common::setup_test_app().await;
    let admin_token = common::setup_admin(&app).await;

    // Create two API keys
    let mut keys = Vec::new();
    for name in ["key-a", "key-b"] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/keys")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {admin_token}"))
            .body(json_body(serde_json::json!({"name": name})))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        keys.push(json["key"].as_str().unwrap().to_string());
    }

    // Login with key A and customize settings
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": keys[0]})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token_a = json["token"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token_a}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "ja",
            "fanart_textless": true
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Login with key B — should still see defaults
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": keys[1]})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token_b = json["token"].as_str().unwrap().to_string();

    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "t", "key B should not see key A's settings");
    assert_eq!(json["is_default"], true);

    // Key A's info endpoint returns key A's name
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {token_a}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "key-a");

    // Key B's info endpoint returns key B's name
    let req = Request::builder()
        .uri("/api/key/me")
        .header("authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "key-b");
}

// --- Rating settings via self-service ---

#[tokio::test]
async fn update_own_settings_with_ratings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_limit": 5,
            "ratings_order": "mal,imdb,trakt,rt,rta"
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {session_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ratings_limit"], 5);
    assert_eq!(json["ratings_order"], "mal,imdb,trakt,rt,rta");
    assert_eq!(json["is_default"], false);
}

#[tokio::test]
async fn update_own_settings_rejects_invalid_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_limit": 99
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_own_settings_rejects_invalid_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let (_raw_key, session_token) = create_api_key_and_login(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session_token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_order": "imdb,nope"
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
