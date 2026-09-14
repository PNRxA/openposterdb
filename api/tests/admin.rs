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

    assert_eq!(json["total_images"], 0);
    assert_eq!(json["total_api_keys"], 0);
    assert!(json["mem_cache_entries"].is_number());
    assert!(json["id_cache_entries"].is_number());
    assert!(json["ratings_cache_entries"].is_number());
    assert!(json["image_mem_cache_mb"].is_number());
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
    assert_eq!(json["image_source"], "t");
    assert_eq!(json["lang"], "en");
    assert_eq!(json["textless"], false);
    assert_eq!(json["fanart_available"], true);
    assert_eq!(json["ratings_limit"], 3);
    assert_eq!(json["ratings_order"], "mal,imdb,lb,rt,mc,rta,tmdb,trakt,mdblist,ebert");
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
                "image_source": "f",
                "lang": "de",
                "textless": true
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
    assert_eq!(json["image_source"], "f");
    assert_eq!(json["lang"], "de");
    assert_eq!(json["textless"], true);
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
                "image_source": "invalid",
                "lang": "en"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // Invalid enum values are rejected at deserialization (422), not validation (400)
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
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
                "image_source": "f",
                "lang": "../../etc"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_settings_with_ratings_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "ratings_limit": 3,
                "ratings_order": "mal,imdb,rta"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ratings_limit"], 3);
    assert_eq!(json["ratings_order"], "mal,imdb,rta");
}

#[tokio::test]
async fn update_settings_rejects_invalid_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "ratings_limit": 11
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Fetch poster endpoint ---

#[tokio::test]
async fn fetch_poster_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/posters/imdb/tt0111161/fetch")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fetch_poster_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/posters/invalid/tt0111161/fetch")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn fetch_poster_accepts_valid_id_types() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // These will fail at the generation stage (no real TMDB key) but should not
    // be rejected at the id_type validation stage (i.e. not 400).
    // tmdb requires a "movie-" or "series-" prefix on the id value.
    for (id_type, id_value) in &[("imdb", "tt0012345"), ("tmdb", "movie-12345"), ("tvdb", "12345")] {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/admin/posters/{id_type}/{id_value}/fetch"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "id_type {id_type} should not be rejected as invalid"
        );
    }
}

#[tokio::test]
async fn update_settings_rejects_invalid_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "ratings_order": "imdb,bogus"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Logo admin endpoints ---

#[tokio::test]
async fn list_logos_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/logos")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_logos_returns_empty() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/logos?page=1&page_size=10")
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
async fn logo_image_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/logos/imdb/tt0111161")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fetch_logo_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/logos/imdb/tt0111161/fetch")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fetch_logo_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/logos/invalid/tt0111161/fetch")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Backdrop admin endpoints ---

#[tokio::test]
async fn list_backdrops_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/backdrops")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_backdrops_returns_empty() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/backdrops?page=1&page_size=10")
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
async fn backdrop_image_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/backdrops/imdb/tt0111161")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fetch_backdrop_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/backdrops/imdb/tt0111161/fetch")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fetch_backdrop_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/backdrops/invalid/tt0111161/fetch")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_settings_returns_new_field_defaults() {
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
    assert_eq!(json["poster_position"], "bc");
    assert_eq!(json["logo_ratings_limit"], 5);
    assert_eq!(json["backdrop_ratings_limit"], 5);
    assert_eq!(json["poster_badge_style"], "d");
    assert_eq!(json["logo_badge_style"], "v");
    assert_eq!(json["backdrop_badge_style"], "v");
}

#[tokio::test]
async fn update_settings_with_poster_position_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "poster_position": "l",
                "logo_ratings_limit": 5,
                "backdrop_ratings_limit": 2
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poster_position"], "l");
    assert_eq!(json["logo_ratings_limit"], 5);
    assert_eq!(json["backdrop_ratings_limit"], 2);
}

#[tokio::test]
async fn update_settings_rejects_invalid_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "poster_position": "invalid"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // Invalid enum values are rejected at deserialization (422), not validation (400)
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// --- Cache purge endpoints ---

#[tokio::test]
async fn purge_endpoints_require_auth() {
    let (app, _state) = common::setup_test_app().await;

    let cases = [
        ("POST", "/api/admin/cache/purge"),
        ("DELETE", "/api/admin/posters"),
        ("DELETE", "/api/admin/logos"),
        ("DELETE", "/api/admin/backdrops"),
        ("DELETE", "/api/admin/episodes"),
        ("DELETE", "/api/admin/posters/imdb/tt0111161"),
        ("DELETE", "/api/admin/logos/imdb/tt0111161"),
        ("DELETE", "/api/admin/backdrops/imdb/tt0111161"),
        ("DELETE", "/api/admin/episodes/imdb/tt0111161"),
    ];

    for (method, uri) in cases {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should require auth");
    }
}

#[tokio::test]
async fn purge_poster_rejects_invalid_id_type() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/posters/invalid/tt0111161")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn purge_poster_clears_all_layers_and_spares_siblings() {
    use openposterdb_api::cache::{self, ImageType, MemCacheEntry};
    use openposterdb_api::entity::{available_ratings, image_meta};
    use sea_orm::EntityTrait;
    use std::time::Instant;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-purge-title-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    let target_key = "imdb/tt123@imc";
    let sibling_key = "imdb/tt1234567@imc"; // longer id — must NOT be captured
    let logo_key = "imdb/tt123_l_t_en@i"; // same title, different kind — must survive

    // Disk: two posters + a logo.
    for value in ["tt123@imc", "tt1234567@imc"] {
        let p = cache::typed_cache_path(&cache_dir, ImageType::Poster, "imdb", value).unwrap();
        cache::write(&p, b"x").await.unwrap();
    }
    let logo_path = cache::typed_cache_path(&cache_dir, ImageType::Logo, "imdb", "tt123_l_t_en@i").unwrap();
    cache::write(&logo_path, b"x").await.unwrap();

    // SQLite image_meta + available_ratings.
    cache::upsert_meta_db(&state.db, target_key, None, ImageType::Poster).await.unwrap();
    cache::upsert_meta_db(&state.db, sibling_key, None, ImageType::Poster).await.unwrap();
    cache::upsert_meta_db(&state.db, logo_key, None, ImageType::Logo).await.unwrap();
    cache::upsert_available_ratings(&state.db, "imdb/tt123", "imc", None).await.unwrap();

    // In-memory render cache.
    state
        .image_mem_cache
        .insert(target_key.to_string(), MemCacheEntry { bytes: vec![0u8; 4].into(), last_checked: Instant::now() })
        .await;
    state
        .image_mem_cache
        .insert(sibling_key.to_string(), MemCacheEntry { bytes: vec![0u8; 4].into(), last_checked: Instant::now() })
        .await;
    state.image_mem_cache.run_pending_tasks().await;

    // In-flight render-result cache (request coalescing) — must be evicted too,
    // or a request within its 30s TTL would re-serve the stale bytes.
    state.image_inflight.insert(target_key.to_string(), vec![0u8; 4].into()).await;
    state.image_inflight.insert(sibling_key.to_string(), vec![0u8; 4].into()).await;
    state.image_inflight.run_pending_tasks().await;

    // Purge the target title's posters.
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/posters/imdb/tt123")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["files_deleted"], 1);
    assert_eq!(json["meta_deleted"], 1);
    assert_eq!(json["ratings_deleted"], 1);
    assert_eq!(json["external_cache_only"], false);

    // Disk: target removed; sibling + logo intact.
    assert!(!std::path::Path::new(&cache_dir).join("posters/imdb/tt123@imc.jpg").exists());
    assert!(std::path::Path::new(&cache_dir).join("posters/imdb/tt1234567@imc.jpg").exists());
    assert!(std::path::Path::new(&cache_dir).join("logos/imdb/tt123_l_t_en@i.png").exists());

    // DB: target row gone; sibling + logo remain; ratings row gone.
    assert!(image_meta::Entity::find_by_id(target_key).one(&state.db).await.unwrap().is_none());
    assert!(image_meta::Entity::find_by_id(sibling_key).one(&state.db).await.unwrap().is_some());
    assert!(image_meta::Entity::find_by_id(logo_key).one(&state.db).await.unwrap().is_some());
    assert!(available_ratings::Entity::find_by_id("imdb/tt123").one(&state.db).await.unwrap().is_none());

    // In-memory: target evicted, sibling retained — in both render and in-flight caches.
    state.image_mem_cache.run_pending_tasks().await;
    state.image_inflight.run_pending_tasks().await;
    assert!(state.image_mem_cache.get(target_key).await.is_none());
    assert!(state.image_mem_cache.get(sibling_key).await.is_some());
    assert!(state.image_inflight.get(target_key).await.is_none());
    assert!(state.image_inflight.get(sibling_key).await.is_some());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[tokio::test]
async fn purge_all_clears_disk_db_and_memory() {
    use openposterdb_api::cache::{self, ImageType, MemCacheEntry};
    use openposterdb_api::services::db;
    use std::time::Instant;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-purge-all-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    let poster_path = cache::typed_cache_path(&cache_dir, ImageType::Poster, "imdb", "tt1@i").unwrap();
    cache::write(&poster_path, b"x").await.unwrap();
    cache::upsert_meta_db(&state.db, "imdb/tt1@i", None, ImageType::Poster).await.unwrap();
    cache::upsert_meta_db(&state.db, "imdb/tt2_l@i", None, ImageType::Logo).await.unwrap();
    cache::upsert_available_ratings(&state.db, "imdb/tt1", "i", None).await.unwrap();
    state
        .image_mem_cache
        .insert("imdb/tt1@i".to_string(), MemCacheEntry { bytes: vec![0u8; 4].into(), last_checked: Instant::now() })
        .await;
    state.image_mem_cache.run_pending_tasks().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/cache/purge")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["meta_deleted"], 2);
    assert_eq!(json["ratings_deleted"], 1);
    assert_eq!(json["external_cache_only"], false);
    assert_eq!(json["dirs_cleared"], 1); // only posters/ existed

    assert_eq!(db::count_image_meta(&state.db).await.unwrap(), 0);
    assert!(!std::path::Path::new(&cache_dir).join("posters").exists());

    state.image_mem_cache.run_pending_tasks().await;
    assert!(state.image_mem_cache.get("imdb/tt1@i").await.is_none());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[tokio::test]
async fn purge_title_external_cache_only_clears_db_without_disk() {
    use openposterdb_api::cache::{self, ImageType};
    use openposterdb_api::entity::image_meta;
    use sea_orm::EntityTrait;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-purge-eco-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        external_cache_only: true,
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    cache::upsert_meta_db(&state.db, "imdb/tt9@i", None, ImageType::Poster).await.unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/posters/imdb/tt9")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["external_cache_only"], true);
    assert_eq!(json["files_deleted"], 0);
    assert_eq!(json["meta_deleted"], 1);

    assert!(image_meta::Entity::find_by_id("imdb/tt9@i").one(&state.db).await.unwrap().is_none());
    // No cache directory is ever created under EXTERNAL_CACHE_ONLY.
    assert!(!std::path::Path::new(&cache_dir).exists());
}

#[tokio::test]
async fn purge_poster_variant_clears_only_that_key() {
    use openposterdb_api::cache::{self, ImageType, MemCacheEntry};
    use openposterdb_api::entity::{available_ratings, image_meta};
    use sea_orm::EntityTrait;
    use std::time::Instant;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-purge-variant-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    let target_key = "imdb/tt0111161_t_de@imc";
    let sibling_key = "imdb/tt0111161@imc"; // same title, different variant — must survive

    for value in ["tt0111161_t_de@imc", "tt0111161@imc"] {
        let p = cache::typed_cache_path(&cache_dir, ImageType::Poster, "imdb", value).unwrap();
        cache::write(&p, b"x").await.unwrap();
    }
    cache::upsert_meta_db(&state.db, target_key, None, ImageType::Poster).await.unwrap();
    cache::upsert_meta_db(&state.db, sibling_key, None, ImageType::Poster).await.unwrap();
    cache::upsert_available_ratings(&state.db, "imdb/tt0111161", "imc", None).await.unwrap();
    state
        .image_mem_cache
        .insert(target_key.to_string(), MemCacheEntry { bytes: vec![0u8; 4].into(), last_checked: Instant::now() })
        .await;
    state
        .image_mem_cache
        .insert(sibling_key.to_string(), MemCacheEntry { bytes: vec![0u8; 4].into(), last_checked: Instant::now() })
        .await;
    state.image_mem_cache.run_pending_tasks().await;
    state.image_inflight.insert(target_key.to_string(), vec![0u8; 4].into()).await;
    state.image_inflight.insert(sibling_key.to_string(), vec![0u8; 4].into()).await;
    state.image_inflight.run_pending_tasks().await;

    // Purge ONLY the target variant (?scope=variant; cache value '@' percent-encoded).
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/posters/imdb/tt0111161_t_de%40imc?scope=variant")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["files_deleted"], 1);
    assert_eq!(json["meta_deleted"], 1);
    // A single-variant purge leaves the title's shared available_ratings index alone.
    assert_eq!(json["ratings_deleted"], 0);

    // Disk + DB: only the target variant is gone; the sibling variant survives.
    assert!(!std::path::Path::new(&cache_dir).join("posters/imdb/tt0111161_t_de@imc.jpg").exists());
    assert!(std::path::Path::new(&cache_dir).join("posters/imdb/tt0111161@imc.jpg").exists());
    assert!(image_meta::Entity::find_by_id(target_key).one(&state.db).await.unwrap().is_none());
    assert!(image_meta::Entity::find_by_id(sibling_key).one(&state.db).await.unwrap().is_some());
    assert!(available_ratings::Entity::find_by_id("imdb/tt0111161").one(&state.db).await.unwrap().is_some());

    // In-memory: only the target variant evicted, in both render and in-flight caches.
    state.image_mem_cache.run_pending_tasks().await;
    state.image_inflight.run_pending_tasks().await;
    assert!(state.image_mem_cache.get(target_key).await.is_none());
    assert!(state.image_mem_cache.get(sibling_key).await.is_some());
    assert!(state.image_inflight.get(target_key).await.is_none());
    assert!(state.image_inflight.get(sibling_key).await.is_some());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[tokio::test]
async fn clear_all_stages_aside_then_background_removes_and_sweeps() {
    use openposterdb_api::cache;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-stage-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    // Seed three cache subdirs (incl. the preview thumbnails) with files; leave the rest absent.
    for sub in ["posters", "base", "preview"] {
        std::fs::create_dir_all(format!("{cache_dir}/{sub}/imdb")).unwrap();
        std::fs::write(format!("{cache_dir}/{sub}/imdb/x.jpg"), b"x").unwrap();
    }

    // Staging renames the existing subdirs aside instantly (rename, not delete).
    let staged = cache::stage_cache_for_clear(&cache_dir).await.unwrap();
    assert_eq!(staged.len(), 3);
    assert!(!std::path::Path::new(&cache_dir).join("posters").exists());
    assert!(!std::path::Path::new(&cache_dir).join("base").exists());
    assert!(!std::path::Path::new(&cache_dir).join("preview").exists());
    for p in &staged {
        assert!(p.exists());
        assert!(p.file_name().unwrap().to_str().unwrap().starts_with(".purging."));
    }

    // Background removal clears the staged dirs entirely.
    cache::remove_staged_dirs(staged).await;
    let leftovers: Vec<_> = std::fs::read_dir(&cache_dir).unwrap().filter_map(|e| e.ok()).collect();
    assert!(leftovers.is_empty(), "staged dirs should be gone after removal");

    // Startup sweep removes a leftover staged dir but spares a real cache subdir.
    std::fs::create_dir_all(format!("{cache_dir}/.purging.posters.123-0/imdb")).unwrap();
    std::fs::create_dir_all(format!("{cache_dir}/posters/imdb")).unwrap();
    cache::cleanup_staged_dirs(&cache_dir).await.unwrap();
    assert!(!std::path::Path::new(&cache_dir).join(".purging.posters.123-0").exists());
    assert!(std::path::Path::new(&cache_dir).join("posters").exists());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[tokio::test]
async fn purge_all_external_cache_only_skips_disk() {
    use openposterdb_api::cache::{self, ImageType};

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-purge-all-eco-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        external_cache_only: true,
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    cache::upsert_meta_db(&state.db, "imdb/tt1@i", None, ImageType::Poster).await.unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/cache/purge")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["external_cache_only"], true);
    assert_eq!(json["dirs_cleared"], 0); // disk staging skipped under EXTERNAL_CACHE_ONLY
    assert_eq!(json["meta_deleted"], 1);

    // DB is still cleared; no cache directory is ever created.
    assert_eq!(openposterdb_api::services::db::count_image_meta(&state.db).await.unwrap(), 0);
    assert!(!std::path::Path::new(&cache_dir).exists());
}

#[tokio::test]
async fn clear_posters_removes_only_posters() {
    use openposterdb_api::cache::{self, ImageType};
    use openposterdb_api::entity::image_meta;
    use sea_orm::EntityTrait;

    let cache_dir = std::env::temp_dir()
        .join(format!("opdb-clear-kind-test-{}", std::process::id()))
        .to_string_lossy()
        .to_string();
    let _ = std::fs::remove_dir_all(&cache_dir);

    let (app, state) = common::setup_test_app_with_options(common::TestAppOptions {
        cache_dir_override: Some(cache_dir.clone()),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    // Two posters + one logo, on disk and in image_meta.
    for v in ["tt1@i", "tt2@i"] {
        let p = cache::typed_cache_path(&cache_dir, ImageType::Poster, "imdb", v).unwrap();
        cache::write(&p, b"x").await.unwrap();
        cache::upsert_meta_db(&state.db, &format!("imdb/{v}"), None, ImageType::Poster).await.unwrap();
    }
    let logo_path = cache::typed_cache_path(&cache_dir, ImageType::Logo, "imdb", "tt1_l@i").unwrap();
    cache::write(&logo_path, b"x").await.unwrap();
    cache::upsert_meta_db(&state.db, "imdb/tt1_l@i", None, ImageType::Logo).await.unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/posters")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["dir_cleared"], true);
    assert_eq!(json["meta_deleted"], 2);

    // The posters dir is staged aside (gone); the logo dir + rows are untouched.
    assert!(!std::path::Path::new(&cache_dir).join("posters").exists());
    assert!(std::path::Path::new(&cache_dir).join("logos/imdb/tt1_l@i.png").exists());
    assert!(image_meta::Entity::find_by_id("imdb/tt1@i").one(&state.db).await.unwrap().is_none());
    assert!(image_meta::Entity::find_by_id("imdb/tt2@i").one(&state.db).await.unwrap().is_none());
    assert!(image_meta::Entity::find_by_id("imdb/tt1_l@i").one(&state.db).await.unwrap().is_some());

    let _ = std::fs::remove_dir_all(&cache_dir);
}
