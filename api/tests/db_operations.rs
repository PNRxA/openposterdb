mod common;

use openposterdb_api::services::db::{self, UpsertApiKeySettings};

// --- Batch last_used_at updates ---

#[tokio::test]
async fn batch_update_last_used_updates_timestamps() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    // Create two API keys
    let mut key_ids = Vec::new();
    for name in ["key-1", "key-2"] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/keys")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                serde_json::json!({"name": name}).to_string(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        key_ids.push(json["id"].as_i64().unwrap() as i32);
    }

    // Verify last_used_at is initially null
    let keys = db::list_api_keys(&state.db).await.unwrap();
    for key in &keys {
        assert!(key.last_used_at.is_none(), "last_used_at should start null");
    }

    // Run batch update
    db::batch_update_last_used(&state.db, &key_ids).await.unwrap();

    // Verify last_used_at is now set
    let keys = db::list_api_keys(&state.db).await.unwrap();
    for key in &keys {
        assert!(
            key.last_used_at.is_some(),
            "last_used_at should be set after batch update"
        );
    }
}

#[tokio::test]
async fn batch_update_last_used_empty_ids_is_noop() {
    let (_app, state) = common::setup_test_app().await;

    // Should succeed without error
    db::batch_update_last_used(&state.db, &[]).await.unwrap();
}

#[tokio::test]
async fn batch_update_last_used_nonexistent_ids_succeeds() {
    let (_app, state) = common::setup_test_app().await;

    // Should not error even with IDs that don't exist
    db::batch_update_last_used(&state.db, &[999, 1000, 1001])
        .await
        .unwrap();
}

#[tokio::test]
async fn batch_update_large_chunk() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    // Create more keys than the chunk size (100)
    let mut key_ids = Vec::new();
    for i in 0..105 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/keys")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                serde_json::json!({"name": format!("key-{i}")}).to_string(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        key_ids.push(json["id"].as_i64().unwrap() as i32);
    }

    // Batch update should handle chunking correctly
    db::batch_update_last_used(&state.db, &key_ids).await.unwrap();

    // Verify all got updated
    let keys = db::list_api_keys(&state.db).await.unwrap();
    let updated_count = keys.iter().filter(|k| k.last_used_at.is_some()).count();
    assert_eq!(updated_count, 105);
}

// --- Delete expired refresh tokens ---

#[tokio::test]
async fn delete_expired_refresh_tokens_removes_only_expired() {
    let (_app, state) = common::setup_test_app().await;

    // Create an admin user directly
    let user = db::create_admin_user(
        &state.db,
        "admin",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await
    .unwrap();

    // Create an expired token
    db::create_refresh_token(&state.db, user.id, "expired_hash", "2020-01-01 00:00:00")
        .await
        .unwrap();

    // Create a valid (future) token
    db::create_refresh_token(&state.db, user.id, "valid_hash", "2099-01-01 00:00:00")
        .await
        .unwrap();

    // Delete expired
    let deleted = db::delete_expired_refresh_tokens(&state.db).await.unwrap();
    assert_eq!(deleted, 1);

    // Verify the valid token still exists
    let remaining = db::find_refresh_token_by_hash(&state.db, "valid_hash")
        .await
        .unwrap();
    assert!(remaining.is_some(), "valid token should still exist");

    // Verify the expired token is gone
    let gone = db::find_refresh_token_by_hash(&state.db, "expired_hash")
        .await
        .unwrap();
    assert!(gone.is_none(), "expired token should be deleted");
}

#[tokio::test]
async fn delete_expired_refresh_tokens_none_expired() {
    let (_app, state) = common::setup_test_app().await;

    let deleted = db::delete_expired_refresh_tokens(&state.db).await.unwrap();
    assert_eq!(deleted, 0);
}

// --- API key hash storage ---

#[tokio::test]
async fn api_key_lookup_by_hash() {
    let (_app, state) = common::setup_test_app().await;

    let user = db::create_admin_user(
        &state.db,
        "admin",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await
    .unwrap();

    // Create a key with known hash
    use sha2::{Digest, Sha256};
    let raw_key = "test_api_key_value_1234567890abcdef";
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());
    let key_prefix = &raw_key[..8];

    db::create_api_key(&state.db, "test-key", &key_hash, key_prefix, user.id)
        .await
        .unwrap();

    // Look up by hash
    let found = db::find_api_key_by_hash(&state.db, &key_hash)
        .await
        .unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "test-key");
    assert_eq!(found.key_prefix, key_prefix);

    // Look up with wrong hash
    let not_found = db::find_api_key_by_hash(&state.db, "0000000000000000")
        .await
        .unwrap();
    assert!(not_found.is_none());
}

// --- Admin user operations ---

#[tokio::test]
async fn create_first_admin_prevents_second() {
    let (_app, state) = common::setup_test_app().await;

    let result1 = db::create_first_admin_user(
        &state.db,
        "admin1",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await;
    assert!(result1.is_ok());

    let result2 = db::create_first_admin_user(
        &state.db,
        "admin2",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000001",
    )
    .await;
    assert!(result2.is_err());
}

#[tokio::test]
async fn find_admin_by_username_and_id() {
    let (_app, state) = common::setup_test_app().await;

    let user = db::create_admin_user(
        &state.db,
        "testadmin",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await
    .unwrap();

    // Find by username
    let found = db::find_admin_user_by_username(&state.db, "testadmin")
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, user.id);

    // Find by id
    let found = db::find_admin_user_by_id(&state.db, user.id)
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().username, "testadmin");

    // Not found
    let not_found = db::find_admin_user_by_username(&state.db, "nobody")
        .await
        .unwrap();
    assert!(not_found.is_none());

    let not_found = db::find_admin_user_by_id(&state.db, 99999)
        .await
        .unwrap();
    assert!(not_found.is_none());
}

// --- Refresh token CRUD ---

#[tokio::test]
async fn refresh_token_create_find_delete() {
    let (_app, state) = common::setup_test_app().await;

    let user = db::create_admin_user(
        &state.db,
        "admin",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await
    .unwrap();

    // Create
    let token = db::create_refresh_token(
        &state.db,
        user.id,
        "test_token_hash",
        "2099-12-31 23:59:59",
    )
    .await
    .unwrap();
    assert_eq!(token.user_id, user.id);
    assert_eq!(token.token_hash, "test_token_hash");

    // Find
    let found = db::find_refresh_token_by_hash(&state.db, "test_token_hash")
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, token.id);

    // Delete
    db::delete_refresh_token(&state.db, token.id)
        .await
        .unwrap();
    let gone = db::find_refresh_token_by_hash(&state.db, "test_token_hash")
        .await
        .unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
async fn delete_refresh_tokens_for_user_clears_all() {
    let (_app, state) = common::setup_test_app().await;

    let user = db::create_admin_user(
        &state.db,
        "admin",
        "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash000000000000000000000000",
    )
    .await
    .unwrap();

    // Create multiple tokens
    for i in 0..3 {
        db::create_refresh_token(
            &state.db,
            user.id,
            &format!("hash_{i}"),
            "2099-12-31 23:59:59",
        )
        .await
        .unwrap();
    }

    // Delete all for user
    db::delete_refresh_tokens_for_user(&state.db, user.id)
        .await
        .unwrap();

    // All should be gone
    for i in 0..3 {
        let found = db::find_refresh_token_by_hash(&state.db, &format!("hash_{i}"))
            .await
            .unwrap();
        assert!(found.is_none());
    }
}

// --- Global settings ---

#[tokio::test]
async fn global_settings_empty_by_default() {
    let (_app, state) = common::setup_test_app().await;
    let settings = db::get_global_settings(&state.db).await.unwrap();
    assert!(settings.is_empty());
}

#[tokio::test]
async fn set_and_get_global_setting() {
    let (_app, state) = common::setup_test_app().await;
    db::set_global_setting(&state.db, "poster_source", "f").await.unwrap();
    db::set_global_setting(&state.db, "fanart_lang", "de").await.unwrap();

    let settings = db::get_global_settings(&state.db).await.unwrap();
    assert_eq!(settings.get("poster_source").unwrap(), "f");
    assert_eq!(settings.get("fanart_lang").unwrap(), "de");
}

#[tokio::test]
async fn set_global_setting_upserts() {
    let (_app, state) = common::setup_test_app().await;
    db::set_global_setting(&state.db, "poster_source", "t").await.unwrap();
    db::set_global_setting(&state.db, "poster_source", "f").await.unwrap();

    let settings = db::get_global_settings(&state.db).await.unwrap();
    assert_eq!(settings.get("poster_source").unwrap(), "f");
}

#[tokio::test]
async fn set_global_settings_batch_atomic() {
    let (_app, state) = common::setup_test_app().await;
    db::set_global_settings_batch(
        &state.db,
        &[
            ("poster_source", "f"),
            ("fanart_lang", "fr"),
            ("fanart_textless", "true"),
        ],
    )
    .await
    .unwrap();

    let settings = db::get_global_settings(&state.db).await.unwrap();
    assert_eq!(settings.get("poster_source").unwrap(), "f");
    assert_eq!(settings.get("fanart_lang").unwrap(), "fr");
    assert_eq!(settings.get("fanart_textless").unwrap(), "true");
}

// --- Per-key settings ---

#[tokio::test]
async fn api_key_settings_none_by_default() {
    let (_app, state) = common::setup_test_app().await;
    let settings = db::get_api_key_settings(&state.db, 999).await.unwrap();
    assert!(settings.is_none());
}

#[tokio::test]
async fn upsert_and_get_api_key_settings() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create an API key via the API to get a valid id
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "f", fanart_lang: "ja", fanart_textless: true,
        ratings_limit: 0, ratings_order: "", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let settings = db::get_api_key_settings(&state.db, key_id).await.unwrap();
    assert!(settings.is_some());
    let s = settings.unwrap();
    assert_eq!(s.poster_source, "f");
    assert_eq!(s.fanart_lang, "ja");
    assert!(s.fanart_textless);
    assert_eq!(s.ratings_limit, 0);
    assert_eq!(s.ratings_order, "");
    assert_eq!(s.poster_badge_direction, "d");
}

#[tokio::test]
async fn upsert_api_key_settings_with_ratings() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 3, ratings_order: "mal,imdb,trakt", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let s = db::get_api_key_settings(&state.db, key_id).await.unwrap().unwrap();
    assert_eq!(s.ratings_limit, 3);
    assert_eq!(s.ratings_order, "mal,imdb,trakt");
}

#[tokio::test]
async fn effective_settings_includes_ratings_from_global() {
    let (_app, state) = common::setup_test_app().await;
    db::set_global_settings_batch(
        &state.db,
        &[
            ("poster_source", "t"),
            ("ratings_limit", "4"),
            ("ratings_order", "imdb,tmdb,rt,rta"),
        ],
    )
    .await
    .unwrap();

    let s = db::get_effective_poster_settings(&state.db, 999, None).await;
    assert_eq!(s.ratings_limit, 4);
    assert_eq!(s.ratings_order, "imdb,tmdb,rt,rta");
}

#[tokio::test]
async fn effective_settings_per_key_ratings_override_global() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::set_global_settings_batch(
        &state.db,
        &[("ratings_limit", "2"), ("ratings_order", "imdb,tmdb")],
    )
    .await
    .unwrap();

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 5, ratings_order: "mal,lb", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let s = db::get_effective_poster_settings(&state.db, key_id, None).await;
    assert_eq!(s.ratings_limit, 5);
    assert_eq!(s.ratings_order, "mal,lb");
    assert!(!s.is_default);
}

#[tokio::test]
async fn upsert_api_key_settings_overwrites() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 0, ratings_order: "", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();
    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "f", fanart_lang: "de", fanart_textless: true,
        ratings_limit: 0, ratings_order: "", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let s = db::get_api_key_settings(&state.db, key_id).await.unwrap().unwrap();
    assert_eq!(s.poster_source, "f");
    assert_eq!(s.fanart_lang, "de");
    assert!(s.fanart_textless);
}

#[tokio::test]
async fn delete_api_key_settings_removes() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "f", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 0, ratings_order: "", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();
    db::delete_api_key_settings(&state.db, key_id).await.unwrap();

    let s = db::get_api_key_settings(&state.db, key_id).await.unwrap();
    assert!(s.is_none());
}

// --- Effective settings cascade ---

#[tokio::test]
async fn effective_settings_defaults_when_nothing_configured() {
    let (_app, state) = common::setup_test_app().await;
    let s = db::get_effective_poster_settings(&state.db, 999, None).await;
    assert_eq!(s.poster_source, "t");
    assert_eq!(s.fanart_lang, "en");
    assert!(!s.fanart_textless);
    assert!(s.is_default);
}

#[tokio::test]
async fn effective_settings_uses_global_when_no_per_key() {
    let (_app, state) = common::setup_test_app().await;
    db::set_global_settings_batch(
        &state.db,
        &[
            ("poster_source", "f"),
            ("fanart_lang", "fr"),
            ("fanart_textless", "true"),
        ],
    )
    .await
    .unwrap();

    let s = db::get_effective_poster_settings(&state.db, 999, None).await;
    assert_eq!(s.poster_source, "f");
    assert_eq!(s.fanart_lang, "fr");
    assert!(s.fanart_textless);
    assert!(s.is_default); // global settings still marked as "default"
}

#[tokio::test]
async fn effective_settings_per_key_overrides_global() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::json!({"name": "test"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    // Set global to fanart/fr
    db::set_global_settings_batch(
        &state.db,
        &[("poster_source", "f"), ("fanart_lang", "fr")],
    )
    .await
    .unwrap();

    // Set per-key to tmdb/ja
    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "ja", fanart_textless: true,
        ratings_limit: 0, ratings_order: "", poster_position: "bc", logo_ratings_limit: 3, backdrop_ratings_limit: 3,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let s = db::get_effective_poster_settings(&state.db, key_id, None).await;
    assert_eq!(s.poster_source, "t");
    assert_eq!(s.fanart_lang, "ja");
    assert!(s.fanart_textless);
    assert!(!s.is_default); // per-key override
}

#[tokio::test]
async fn upsert_api_key_settings_with_poster_position() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({"name": "pos-test"}).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 3, ratings_order: "imdb,rt", poster_position: "l", logo_ratings_limit: 5, backdrop_ratings_limit: 1,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let settings = db::get_api_key_settings(&state.db, key_id).await.unwrap();
    let s = settings.unwrap();
    assert_eq!(s.poster_position, "l");
    assert_eq!(s.logo_ratings_limit, 5);
    assert_eq!(s.backdrop_ratings_limit, 1);
}

#[tokio::test]
async fn effective_settings_include_new_fields() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/keys")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({"name": "eff-test"}).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key_id = json["id"].as_i64().unwrap() as i32;

    // Without per-key settings, effective should have defaults
    let s = db::get_effective_poster_settings(&state.db, key_id, None).await;
    assert_eq!(s.poster_position, "bc");
    assert_eq!(s.logo_ratings_limit, 5);
    assert_eq!(s.backdrop_ratings_limit, 5);

    // Set per-key with custom values
    db::upsert_api_key_settings(&state.db, UpsertApiKeySettings {
        api_key_id: key_id, poster_source: "t", fanart_lang: "en", fanart_textless: false,
        ratings_limit: 3, ratings_order: "", poster_position: "r", logo_ratings_limit: 2, backdrop_ratings_limit: 0,
        poster_badge_style: "h", logo_badge_style: "h", backdrop_badge_style: "v",
        poster_label_style: "t", logo_label_style: "t", backdrop_label_style: "t", poster_badge_direction: "d",
    }).await.unwrap();

    let s = db::get_effective_poster_settings(&state.db, key_id, None).await;
    assert_eq!(s.poster_position, "r");
    assert_eq!(s.logo_ratings_limit, 2);
    assert_eq!(s.backdrop_ratings_limit, 0);
}
