mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

fn authed_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json");
    match body {
        Some(b) => builder.body(Body::from(b.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn parse_json(res: axum::http::Response<Body>) -> Value {
    let body = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn create_api_key(app: &axum::Router, token: &str) -> (i32, String) {
    let req = authed_request("POST", "/api/keys", token, Some(json!({"name":"e2e-test"})));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = parse_json(res).await;
    (json["id"].as_i64().unwrap() as i32, json["key"].as_str().unwrap().to_string())
}

// --- Auth enforcement ---

#[tokio::test]
async fn global_settings_get_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn global_settings_put_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .body(Body::from(json!({"image_source": "t"}).to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn per_key_settings_endpoints_require_auth() {
    let (app, _state) = common::setup_test_app().await;

    for (method, uri) in [
        ("GET", "/api/keys/1/settings"),
        ("PUT", "/api/keys/1/settings"),
        ("DELETE", "/api/keys/1/settings"),
    ] {
        let body = if method == "PUT" {
            Body::from(json!({"image_source": "t"}).to_string())
        } else {
            Body::empty()
        };
        let mut builder = Request::builder().method(method).uri(uri);
        if method == "PUT" {
            builder = builder.header("content-type", "application/json");
        }
        let req = builder.body(body).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} should require admin auth"
        );
    }
}

#[tokio::test]
async fn self_service_settings_endpoints_require_auth() {
    let (app, _state) = common::setup_test_app().await;

    for (method, uri) in [
        ("GET", "/api/key/me/settings"),
        ("PUT", "/api/key/me/settings"),
        ("DELETE", "/api/key/me/settings"),
    ] {
        let body = if method == "PUT" {
            Body::from(json!({"image_source": "t"}).to_string())
        } else {
            Body::empty()
        };
        let mut builder = Request::builder().method(method).uri(uri);
        if method == "PUT" {
            builder = builder.header("content-type", "application/json");
        }
        let req = builder.body(body).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} should require API key auth"
        );
    }
}

// --- Cross-key isolation via admin endpoints ---

#[tokio::test]
async fn per_key_settings_are_isolated_between_keys() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Create two API keys
    let (key_a_id, _) = create_api_key(&app, &token).await;
    let (key_b_id, _) = create_api_key(&app, &token).await;

    // Customize key A's settings
    let update = json!({
        "image_source": "f",
        "lang": "ja",
        "ratings_limit": 1,
        "ratings_order": "imdb",
    });
    let req = authed_request("PUT", &format!("/api/keys/{key_a_id}/settings"), &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Key B should still have defaults
    let req = authed_request("GET", &format!("/api/keys/{key_b_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;
    assert_eq!(settings["is_default"], true, "key B should not be affected by key A's settings");
    assert_eq!(settings["image_source"], "t");

    // Key A should have its custom settings
    let req = authed_request("GET", &format!("/api/keys/{key_a_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;
    assert_eq!(settings["is_default"], false);
    assert_eq!(settings["image_source"], "f");
    assert_eq!(settings["lang"], "ja");
}

// --- Global settings round-trip ---

#[tokio::test]
async fn global_settings_round_trip_all_fields() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Update global settings with non-default values
    let update = json!({
        "image_source": "f",
        "lang": "de",
        "textless": true,
        "ratings_limit": 5,
        "ratings_order": "imdb,rt,mc,tmdb,trakt,mal,lb,rta",
        "ratings_exclude": "rt,mc",
        "poster_position": "tl",
        "logo_ratings_limit": 2,
        "backdrop_ratings_limit": 7,
        "poster_badge_style": "v",
        "logo_badge_style": "h",
        "backdrop_badge_style": "h",
        "poster_label_style": "t",
        "logo_label_style": "i",
        "backdrop_label_style": "o",
        "poster_badge_direction": "h",
        "poster_badge_size": "xl",
        "logo_badge_size": "xs",
        "backdrop_badge_size": "l",
        "poster_font_size": "xl",
        "logo_font_size": "l",
        "backdrop_font_size": "s",
        "episode_font_size": "xs",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update.clone()));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read them back
    let req = authed_request("GET", "/api/admin/settings", &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;

    assert_eq!(settings["image_source"], "f");
    assert_eq!(settings["lang"], "de");
    assert_eq!(settings["textless"], true);
    assert_eq!(settings["ratings_limit"], 5);
    assert_eq!(settings["ratings_order"], "imdb,rt,mc,tmdb,trakt,mal,lb,rta");
    assert_eq!(settings["ratings_exclude"], "rt,mc");
    assert_eq!(settings["poster_position"], "tl");
    assert_eq!(settings["logo_ratings_limit"], 2);
    assert_eq!(settings["backdrop_ratings_limit"], 7);
    assert_eq!(settings["poster_badge_style"], "v");
    assert_eq!(settings["logo_badge_style"], "h");
    assert_eq!(settings["backdrop_badge_style"], "h");
    assert_eq!(settings["poster_label_style"], "t");
    assert_eq!(settings["logo_label_style"], "i");
    assert_eq!(settings["backdrop_label_style"], "o");
    assert_eq!(settings["poster_badge_direction"], "h");
    assert_eq!(settings["poster_badge_size"], "xl");
    assert_eq!(settings["logo_badge_size"], "xs");
    assert_eq!(settings["backdrop_badge_size"], "l");
    assert_eq!(settings["poster_font_size"], "xl");
    assert_eq!(settings["logo_font_size"], "l");
    assert_eq!(settings["backdrop_font_size"], "s");
    assert_eq!(settings["episode_font_size"], "xs");
}

// --- Per-key settings round-trip ---

#[tokio::test]
async fn per_key_settings_round_trip() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let (key_id, _raw_key) = create_api_key(&app, &token).await;

    // Read defaults (should be global/default)
    let req = authed_request("GET", &format!("/api/keys/{key_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let defaults = parse_json(res).await;
    assert_eq!(defaults["is_default"], true);
    assert_eq!(defaults["poster_badge_size"], "m");
    assert_eq!(defaults["poster_font_size"], "m");
    assert_eq!(defaults["episode_font_size"], "m");

    // Set per-key overrides
    let update = json!({
        "image_source": "t",
        "ratings_limit": 1,
        "ratings_order": "tmdb",
        "ratings_exclude": "lb,mal",
        "poster_position": "r",
        "logo_ratings_limit": 4,
        "backdrop_ratings_limit": 6,
        "poster_badge_style": "h",
        "logo_badge_style": "v",
        "backdrop_badge_style": "v",
        "poster_label_style": "o",
        "logo_label_style": "o",
        "backdrop_label_style": "t",
        "poster_badge_direction": "v",
        "poster_badge_size": "s",
        "logo_badge_size": "l",
        "backdrop_badge_size": "xl",
        "poster_font_size": "l",
        "logo_font_size": "xl",
        "backdrop_font_size": "xs",
        "episode_font_size": "s",
    });
    let req = authed_request("PUT", &format!("/api/keys/{key_id}/settings"), &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read back — should reflect per-key values
    let req = authed_request("GET", &format!("/api/keys/{key_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;

    assert_eq!(settings["is_default"], false);
    assert_eq!(settings["ratings_limit"], 1);
    assert_eq!(settings["ratings_exclude"], "lb,mal");
    assert_eq!(settings["poster_position"], "r");
    assert_eq!(settings["poster_badge_style"], "h");
    assert_eq!(settings["logo_badge_style"], "v");
    assert_eq!(settings["poster_label_style"], "o");
    assert_eq!(settings["poster_badge_direction"], "v");
    assert_eq!(settings["poster_badge_size"], "s");
    assert_eq!(settings["logo_badge_size"], "l");
    assert_eq!(settings["backdrop_badge_size"], "xl");
    assert_eq!(settings["poster_font_size"], "l");
    assert_eq!(settings["logo_font_size"], "xl");
    assert_eq!(settings["backdrop_font_size"], "xs");
    assert_eq!(settings["episode_font_size"], "s");
    assert_eq!(settings["logo_ratings_limit"], 4);
    assert_eq!(settings["backdrop_ratings_limit"], 6);
}

// --- Per-key reset falls back to global ---

#[tokio::test]
async fn per_key_reset_falls_back_to_global() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let (key_id, _raw_key) = create_api_key(&app, &token).await;

    // Set global to something non-default
    let global = json!({
        "image_source": "f",
        "ratings_limit": 7,
        "ratings_order": "rt,imdb",
        "poster_position": "bl",
        "poster_badge_style": "v",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(global));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Set per-key override
    let per_key = json!({
        "image_source": "t",
        "ratings_limit": 2,
        "ratings_order": "tmdb",
        "poster_badge_style": "h",
    });
    let req = authed_request("PUT", &format!("/api/keys/{key_id}/settings"), &token, Some(per_key));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Delete per-key settings
    let req = authed_request("DELETE", &format!("/api/keys/{key_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Invalidate settings cache to force fresh load
    _state.settings_cache.invalidate(&key_id).await;

    // Read back — should reflect global values
    let req = authed_request("GET", &format!("/api/keys/{key_id}/settings"), &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;

    assert_eq!(settings["is_default"], true);
    assert_eq!(settings["image_source"], "f");
    assert_eq!(settings["ratings_limit"], 7);
    assert_eq!(settings["poster_position"], "bl");
    assert_eq!(settings["poster_badge_style"], "v");
}

// --- Self-service settings round-trip ---

#[tokio::test]
async fn self_service_settings_round_trip() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    // Read defaults
    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let defaults = parse_json(res).await;
    assert_eq!(defaults["is_default"], true);

    // Update own settings
    let update = json!({
        "image_source": "t",
        "ratings_limit": 4,
        "ratings_order": "imdb,tmdb,rt,mc",
        "poster_position": "tc",
        "poster_badge_style": "h",
        "logo_badge_style": "h",
        "backdrop_badge_style": "h",
        "poster_label_style": "t",
        "logo_label_style": "t",
        "backdrop_label_style": "t",
        "poster_badge_direction": "h",
        "poster_badge_size": "l",
        "logo_badge_size": "s",
        "backdrop_badge_size": "xs",
        "poster_font_size": "xl",
    });
    let req = Request::builder()
        .method("PUT")
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {api_key_token}"))
        .header("content-type", "application/json")
        .body(Body::from(update.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read back
    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;

    assert_eq!(settings["is_default"], false);
    assert_eq!(settings["ratings_limit"], 4);
    assert_eq!(settings["poster_position"], "tc");
    assert_eq!(settings["poster_badge_style"], "h");
    assert_eq!(settings["poster_badge_direction"], "h");
    assert_eq!(settings["poster_badge_size"], "l");
    assert_eq!(settings["logo_badge_size"], "s");
    assert_eq!(settings["backdrop_badge_size"], "xs");
    assert_eq!(settings["poster_font_size"], "xl");
    // Omitted font sizes fall back to the default.
    assert_eq!(settings["logo_font_size"], "m");

    // Reset own settings
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Invalidate cache
    _state.settings_cache.invalidate_all();

    // Read back — should be defaults again
    let req = Request::builder()
        .uri("/api/key/me/settings")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let settings = parse_json(res).await;
    assert_eq!(settings["is_default"], true);
}

// --- Validation: global settings rejects invalid values ---

#[tokio::test]
async fn global_settings_rejects_invalid_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({
        "image_source": "t",
        "ratings_limit": 11,
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn global_settings_rejects_invalid_lang() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({
        "image_source": "t",
        "lang": "x",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn global_settings_rejects_invalid_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({
        "image_source": "t",
        "ratings_order": "bogus",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn global_settings_rejects_invalid_ratings_exclude() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({
        "image_source": "t",
        "ratings_exclude": "bogus",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn global_ratings_exclude_propagates_to_free_key_renders() {
    // A global ratings_exclude must change what the free key renders (and its
    // cache key), proving exclusion flows through the global settings path.
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({ "image_source": "t", "ratings_exclude": "rt" });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Read back via the admin GET to confirm persistence.
    let req = authed_request("GET", "/api/admin/settings", &token, None);
    let res = app.clone().oneshot(req).await.unwrap();
    let settings = parse_json(res).await;
    assert_eq!(settings["ratings_exclude"], "rt");
}

#[tokio::test]
async fn global_settings_rejects_invalid_badge_style() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let update = json!({
        "image_source": "t",
        "poster_badge_style": "z",
    });
    let req = authed_request("PUT", "/api/admin/settings", &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    // Invalid enum values fail at deserialization (422) rather than validation (400)
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn per_key_settings_rejects_invalid_logo_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let (key_id, _) = create_api_key(&app, &token).await;

    let update = json!({
        "image_source": "t",
        "logo_ratings_limit": 11,
    });
    let req = authed_request("PUT", &format!("/api/keys/{key_id}/settings"), &token, Some(update));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Preview generates different images for different badge styles ---

#[tokio::test]
async fn preview_badge_style_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_h = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_style=h", &token, None),
    ).await.unwrap();
    assert_eq!(res_h.status(), StatusCode::OK);
    let body_h = res_h.into_body().collect().await.unwrap().to_bytes();

    let res_v = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_style=v", &token, None),
    ).await.unwrap();
    assert_eq!(res_v.status(), StatusCode::OK);
    let body_v = res_v.into_body().collect().await.unwrap().to_bytes();

    // Both valid JPEGs
    assert_eq!(body_h[0], 0xFF);
    assert_eq!(body_v[0], 0xFF);
    // Different badge styles should produce different images
    assert_ne!(body_h, body_v);
}

#[tokio::test]
async fn preview_label_style_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_text = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&label_style=t", &token, None),
    ).await.unwrap();
    assert_eq!(res_text.status(), StatusCode::OK);
    let body_text = res_text.into_body().collect().await.unwrap().to_bytes();

    let res_icon = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&label_style=i", &token, None),
    ).await.unwrap();
    assert_eq!(res_icon.status(), StatusCode::OK);
    let body_icon = res_icon.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_text[0], 0xFF);
    assert_eq!(body_icon[0], 0xFF);
    assert_ne!(body_text, body_icon);
}

#[tokio::test]
async fn preview_badge_size_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_s = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_size=xs", &token, None),
    ).await.unwrap();
    assert_eq!(res_s.status(), StatusCode::OK);
    let body_s = res_s.into_body().collect().await.unwrap().to_bytes();

    let res_xl = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_size=xl", &token, None),
    ).await.unwrap();
    assert_eq!(res_xl.status(), StatusCode::OK);
    let body_xl = res_xl.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_s[0], 0xFF);
    assert_eq!(body_xl[0], 0xFF);
    assert_ne!(body_s, body_xl);
}

#[tokio::test]
async fn preview_font_size_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let fetch = |q: &str| {
        let app = app.clone();
        let token = token.clone();
        let uri = format!("/api/admin/preview/poster?ratings_limit=3{q}");
        async move {
            let res = app.oneshot(authed_request("GET", &uri, &token, None)).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            res.into_body().collect().await.unwrap().to_bytes()
        }
    };

    let default = fetch("").await;
    // Explicit `m` is the default — same image (and same cache key).
    assert_eq!(default, fetch("&font_size=m").await);
    let xl = fetch("&font_size=xl").await;
    let xs = fetch("&font_size=xs").await;
    assert_eq!(xl[0], 0xFF);
    assert_ne!(default, xl);
    assert_ne!(default, xs);
    assert_ne!(xs, xl);
}

#[tokio::test]
async fn preview_font_size_rejects_invalid() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    let res = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?font_size=huge", &token, None),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_font_size_accepted_for_all_kinds() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;
    for kind in ["logo", "backdrop", "episode"] {
        for q in ["", "?font_size=xl"] {
            let res = app.clone().oneshot(
                authed_request("GET", &format!("/api/admin/preview/{kind}{q}"), &token, None),
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{kind}{q}");
        }
    }
}

// --- Logo/Backdrop preview endpoints ---

#[tokio::test]
async fn preview_logo_returns_png() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/logo?ratings_limit=2&badge_style=h", &token, None),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/png");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(&body[..4] == &[0x89, b'P', b'N', b'G'], "should be valid PNG");
}

#[tokio::test]
async fn preview_backdrop_returns_jpeg() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/backdrop?ratings_limit=2&badge_style=v", &token, None),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_logo_badge_size_accepted() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/logo?badge_size=l&label_style=t", &token, None),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn preview_backdrop_badge_size_accepted() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/backdrop?badge_size=xs&label_style=o", &token, None),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- E2E: Settings affect image generation ---

#[tokio::test]
async fn preview_reflects_badge_direction_change() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_default = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_direction=d&position=l", &token, None),
    ).await.unwrap();
    assert_eq!(res_default.status(), StatusCode::OK);
    let body_default = res_default.into_body().collect().await.unwrap().to_bytes();

    let res_h = app.clone().oneshot(
        authed_request("GET", "/api/admin/preview/poster?ratings_limit=3&badge_direction=h&position=l", &token, None),
    ).await.unwrap();
    assert_eq!(res_h.status(), StatusCode::OK);
    let body_h = res_h.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_default[0], 0xFF);
    assert_eq!(body_h[0], 0xFF);
    // direction=d with position=l resolves to vertical, while direction=h is horizontal — different images
    assert_ne!(body_default, body_h);
}
