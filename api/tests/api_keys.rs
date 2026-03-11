mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

#[tokio::test]
async fn create_key_authenticated() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["key"].is_string());
    assert!(json["key_prefix"].is_string());
    assert_eq!(json["name"], "test-key");
}

#[tokio::test]
async fn list_keys_authenticated() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key first
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "my-key"})))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    // List keys
    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let keys = json.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"], "my-key");
}

#[tokio::test]
async fn delete_key_authenticated() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "to-delete"})))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    // Delete the key
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/keys/{id}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify it's gone
    let req = Request::builder()
        .uri("/api/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn endpoints_without_auth_return_401() {
    let (app, _state) = common::setup_test_app().await;

    // GET /api/keys without auth
    let req = Request::builder()
        .uri("/api/keys")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // POST /api/keys without auth
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"name": "test"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // DELETE /api/keys/1 without auth
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/keys/1")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_key_with_empty_name() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": ""})))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Per-key settings endpoints ---

#[tokio::test]
async fn key_settings_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/keys/1/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .method("PUT")
        .uri("/api/keys/1/settings")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"poster_source": "t"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/keys/1/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_key_settings_returns_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    // Get settings
    let req = Request::builder()
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
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
    assert_eq!(json["ratings_limit"], 3);
    assert_eq!(json["ratings_order"], "mal,imdb,lb,rt,mc,rta,tmdb,trakt");
}

#[tokio::test]
async fn update_key_settings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    // Update settings
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
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
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
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
async fn delete_key_settings_resets_to_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    // Set custom settings
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "de",
            "fanart_textless": true
        })))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    // Delete settings
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Should be back to defaults
    let req = Request::builder()
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_source"], "t");
    assert_eq!(json["is_default"], true);
}

#[tokio::test]
async fn update_key_settings_rejects_invalid_source() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "invalid"
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_key_settings_rejects_invalid_lang() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create a key
    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "f",
            "fanart_lang": "a"
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_key_settings_with_ratings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_limit": 4,
            "ratings_order": "trakt,imdb,mal,lb"
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ratings_limit"], 4);
    assert_eq!(json["ratings_order"], "trakt,imdb,mal,lb");
    assert_eq!(json["is_default"], false);
}

#[tokio::test]
async fn update_key_settings_rejects_invalid_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_limit": -1
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_key_settings_rejects_invalid_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "ratings_order": "invalid_source"
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_key_settings_returns_new_field_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_position"], "bc");
    assert_eq!(json["logo_ratings_limit"], 5);
    assert_eq!(json["backdrop_ratings_limit"], 5);
    assert_eq!(json["poster_badge_style"], "h");
    assert_eq!(json["logo_badge_style"], "v");
    assert_eq!(json["backdrop_badge_style"], "v");
}

#[tokio::test]
async fn update_key_settings_with_poster_position_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "poster_position": "r",
            "logo_ratings_limit": 1,
            "backdrop_ratings_limit": 0
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .uri(format!("/api/keys/{id}/settings"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_position"], "r");
    assert_eq!(json["logo_ratings_limit"], 1);
    assert_eq!(json["backdrop_ratings_limit"], 0);
    assert_eq!(json["is_default"], false);
}

#[tokio::test]
async fn update_key_settings_rejects_invalid_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({"name": "test-key"})))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/keys/{id}/settings"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "poster_source": "t",
            "poster_position": "center-bottom"
        })))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
