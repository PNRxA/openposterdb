mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json;
use tower::ServiceExt;

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn preview_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/preview/poster")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn preview_returns_jpeg_with_defaults() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=60"
    );

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100, "JPEG should have substantial content");
    // JPEG magic bytes
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_respects_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Request with limit=1 — should produce a smaller image (fewer badges)
    let res_small = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1", &token)).await.unwrap();
    assert_eq!(res_small.status(), StatusCode::OK);
    let body_small = res_small.into_body().collect().await.unwrap().to_bytes();

    // Request with limit=10 (show all 10 badges)
    let res_all = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=10", &token)).await.unwrap();
    assert_eq!(res_all.status(), StatusCode::OK);
    let body_all = res_all.into_body().collect().await.unwrap().to_bytes();

    // Both should be valid JPEGs
    assert_eq!(body_small[0], 0xFF);
    assert_eq!(body_all[0], 0xFF);

    // Both are valid and non-empty
    assert!(body_small.len() > 100);
    assert!(body_all.len() > 100);
}

#[tokio::test]
async fn preview_respects_ratings_order() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Two different badge selections should produce different images
    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,tmdb", &token)).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=rt,mc", &token)).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Both valid JPEGs
    assert_eq!(body1[0], 0xFF);
    assert_eq!(body2[0], 0xFF);

    // Different badge selections should produce different image bytes
    assert_ne!(body1, body2);
}

#[tokio::test]
async fn preview_respects_ratings_exclude() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Same order+limit, but excluding a source must produce a different image
    // (otherwise the cache key would collide and serve the wrong image).
    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&ratings_order=imdb,rt,tmdb", &token)).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&ratings_order=imdb,rt,tmdb&ratings_exclude=rt", &token)).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body1[0], 0xFF);
    assert_eq!(body2[0], 0xFF);
    assert_ne!(body1, body2, "excluding a source must change the rendered preview");
}

#[tokio::test]
async fn preview_rejects_invalid_ratings_exclude() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_exclude=bogus_source", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_with_empty_order_still_works() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_order=&ratings_limit=3", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_cache_returns_identical_bytes_for_same_params() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let uri = "/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,tmdb";

    let res1 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Second request should return identical bytes from cache
    assert_eq!(body1, body2);
}

#[tokio::test]
async fn preview_cache_differs_for_different_params() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1&ratings_order=imdb", &token)).await.unwrap();
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=1&ratings_order=rt", &token)).await.unwrap();
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    // Different rating params should produce different images (different cache keys)
    assert_ne!(body1, body2);
}

#[tokio::test]
async fn preview_backdrop_edge_inset_shifts_only_active_axis() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let fetch = |q: &str| {
        let app = app.clone();
        let token = token.clone();
        let uri = format!("/api/admin/preview/backdrop?{q}");
        async move {
            let res = app.oneshot(authed_get(&uri, &token)).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            res.into_body().collect().await.unwrap().to_bytes()
        }
    };

    // Top-right anchors to both edges → both insets change the rendered image.
    let base = fetch("position=tr").await;
    assert_ne!(base, fetch("position=tr&edge_inset_x=12").await);
    assert_ne!(base, fetch("position=tr&edge_inset_y=8").await);

    // Top-center is horizontally centered → the horizontal inset is a no-op and
    // must reuse the same cache key / image, while the vertical inset applies.
    let tc = fetch("position=tc").await;
    assert_eq!(tc, fetch("position=tc&edge_inset_x=12").await);
    assert_ne!(tc, fetch("position=tc&edge_inset_y=8").await);
}

#[tokio::test]
async fn preview_cache_populates_entry_count() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    assert_eq!(state.preview_cache.entry_count(), 0);

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Force pending tasks to run so moka registers the insert
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1);
}

#[tokio::test]
async fn preview_cache_survives_settings_update() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Warm the preview cache
    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    state.preview_cache.run_pending_tasks().await;
    assert!(state.preview_cache.entry_count() > 0, "cache should be populated");

    // Update settings — cache keys encode the config, so no invalidation needed
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            serde_json::json!({
                "image_source": "t",
                "ratings_limit": 5,
                "ratings_order": "imdb,rt,mc,tmdb,trakt,mal,lb,rta"
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Cache should still have the original entry — different settings = different key
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1, "existing cache entry should survive settings update");
}

#[tokio::test]
async fn preview_serves_from_filesystem_after_memory_eviction() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let uri = "/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt";

    // First request — renders and writes to both memory + filesystem
    let res = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body1 = res.into_body().collect().await.unwrap().to_bytes();

    // Evict from memory cache to simulate TTL expiry
    state.preview_cache.invalidate_all();
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 0);

    // Second request — should serve from filesystem, not re-render
    let res = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body2 = res.into_body().collect().await.unwrap().to_bytes();

    // Should return identical bytes from filesystem
    assert_eq!(body1, body2);

    // Memory cache should be re-populated from filesystem
    state.preview_cache.run_pending_tasks().await;
    assert_eq!(state.preview_cache.entry_count(), 1);
}

#[tokio::test]
async fn preview_accessible_via_self_serve_auth() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/poster")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_respects_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?position=l", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100);
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_cache_differs_for_different_positions() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res1 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt&position=bc", &token)).await.unwrap();
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=2&ratings_order=imdb,rt&position=l", &token)).await.unwrap();
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body1[0], 0xFF);
    assert_eq!(body2[0], 0xFF);
    assert_ne!(body1, body2);
}


/// Verify that the preview endpoint actually renders badges in the correct
/// region of the image by checking pixel data, not just HTTP status codes.
#[tokio::test]
async fn preview_position_places_badges_in_correct_region() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let params = "ratings_limit=3&ratings_order=imdb,tmdb,rt";

    let res_tc = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/poster?{params}&position=tc"), &token,
    )).await.unwrap();
    assert_eq!(res_tc.status(), StatusCode::OK);
    let body_tc = res_tc.into_body().collect().await.unwrap().to_bytes();

    let res_bc = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/poster?{params}&position=bc"), &token,
    )).await.unwrap();
    assert_eq!(res_bc.status(), StatusCode::OK);
    let body_bc = res_bc.into_body().collect().await.unwrap().to_bytes();

    // The sample poster is a dark gray gradient (26–42 per channel).
    // Badge pixels are significantly brighter. Compute the y-centroid of
    // bright pixels to verify badge placement.
    fn badge_y_centroid(jpeg: &[u8]) -> f64 {
        let img = image::load_from_memory(jpeg).unwrap().to_rgba8();
        let h = img.height() as f64;
        let (mut sum_y, mut count) = (0u64, 0u64);
        for (_, y, px) in img.enumerate_pixels() {
            if px[0].max(px[1]).max(px[2]) > 80 {
                sum_y += y as u64;
                count += 1;
            }
        }
        assert!(count > 0, "no badge pixels found");
        sum_y as f64 / count as f64 / h
    }

    let tc_cy = badge_y_centroid(&body_tc);
    let bc_cy = badge_y_centroid(&body_bc);

    assert!(tc_cy < 0.33,
        "TopCenter: badge y-centroid {tc_cy:.2} should be in top third");
    assert!(bc_cy > 0.67,
        "BottomCenter: badge y-centroid {bc_cy:.2} should be in bottom third");
}

#[tokio::test]
async fn preview_rejects_invalid_poster_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?position=invalid", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Badge style tests ---

#[tokio::test]
async fn preview_accepts_all_badge_styles() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for style in ["h", "v", "d"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/poster?badge_style={style}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "badge_style={style} should be accepted");
        assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
    }
}

#[tokio::test]
async fn preview_rejects_invalid_badge_style() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?badge_style=z", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_badge_style_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_h = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&badge_style=h", &token)).await.unwrap();
    let body_h = res_h.into_body().collect().await.unwrap().to_bytes();

    let res_v = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&badge_style=v", &token)).await.unwrap();
    let body_v = res_v.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_h[0], 0xFF);
    assert_eq!(body_v[0], 0xFF);
    assert_ne!(body_h, body_v, "horizontal and vertical badge styles should produce different images");
}

// --- Label style tests ---

#[tokio::test]
async fn preview_accepts_all_label_styles() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for style in ["t", "i", "o"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/poster?label_style={style}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "label_style={style} should be accepted");
    }
}

#[tokio::test]
async fn preview_rejects_invalid_label_style() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?label_style=x", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_label_style_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_text = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&label_style=t", &token)).await.unwrap();
    let body_text = res_text.into_body().collect().await.unwrap().to_bytes();

    let res_icon = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&label_style=i", &token)).await.unwrap();
    let body_icon = res_icon.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_text[0], 0xFF);
    assert_eq!(body_icon[0], 0xFF);
    assert_ne!(body_text, body_icon, "text and icon label styles should produce different images");
}

// --- Badge size tests ---

#[tokio::test]
async fn preview_accepts_all_badge_sizes() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for size in ["xs", "s", "m", "l", "xl"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/poster?badge_size={size}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "badge_size={size} should be accepted");
    }
}

#[tokio::test]
async fn preview_rejects_invalid_badge_size() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?badge_size=xxl", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_badge_size_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_xs = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&badge_size=xs", &token)).await.unwrap();
    let body_xs = res_xs.into_body().collect().await.unwrap().to_bytes();

    let res_xl = app.clone().oneshot(authed_get("/api/admin/preview/poster?ratings_limit=3&badge_size=xl", &token)).await.unwrap();
    let body_xl = res_xl.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_xs[0], 0xFF);
    assert_eq!(body_xl[0], 0xFF);
    assert_ne!(body_xs, body_xl, "xs and xl badge sizes should produce different images");
}

// --- Badge direction tests ---

#[tokio::test]
async fn preview_accepts_all_badge_directions() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for dir in ["d", "h", "v"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/poster?badge_direction={dir}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "badge_direction={dir} should be accepted");
    }
}

#[tokio::test]
async fn preview_rejects_invalid_badge_direction() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/poster?badge_direction=x", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Logo preview tests ---

#[tokio::test]
async fn preview_logo_returns_png() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/logo", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/png");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..4], &[0x89, b'P', b'N', b'G']);
}

#[tokio::test]
async fn preview_logo_accepts_badge_style_and_size() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get(
        "/api/admin/preview/logo?badge_style=h&badge_size=l&label_style=t", &token
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- Backdrop preview tests ---

#[tokio::test]
async fn preview_backdrop_returns_jpeg() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/backdrop", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_backdrop_accepts_badge_style_and_size() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get(
        "/api/admin/preview/backdrop?badge_style=v&badge_size=xs&label_style=o", &token
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- Combined param tests ---

#[tokio::test]
async fn preview_all_new_params_combined() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get(
        "/api/admin/preview/poster?ratings_limit=4&ratings_order=imdb,tmdb,rt,mc&position=tl&badge_style=v&label_style=i&badge_direction=v&badge_size=l",
        &token
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body[0], 0xFF);
    assert!(body.len() > 100);
}

#[tokio::test]
async fn preview_self_serve_logo() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/logo?badge_style=h&badge_size=s")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
}

#[tokio::test]
async fn preview_self_serve_backdrop() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/backdrop?badge_size=xl&label_style=t")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

// --- Episode preview tests ---

#[tokio::test]
async fn preview_episode_returns_jpeg() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/episode", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100, "JPEG should have substantial content");
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_episode_accepts_all_settings() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get(
        "/api/admin/preview/episode?ratings_limit=3&ratings_order=imdb,tmdb,rt&position=tl&badge_style=h&label_style=i&badge_direction=h&badge_size=m&blur=true",
        &token
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body[0], 0xFF);
    assert!(body.len() > 100);
}

#[tokio::test]
async fn preview_episode_blur_produces_different_image() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let params = "ratings_limit=2&ratings_order=imdb,tmdb";

    let res_noblur = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/episode?{params}"), &token
    )).await.unwrap();
    let body_noblur = res_noblur.into_body().collect().await.unwrap().to_bytes();

    let res_blur = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/episode?{params}&blur=true"), &token
    )).await.unwrap();
    let body_blur = res_blur.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_noblur[0], 0xFF);
    assert_eq!(body_blur[0], 0xFF);
    assert_ne!(body_noblur, body_blur, "blur should produce a different image");
}

#[tokio::test]
async fn preview_episode_cache_returns_identical_bytes() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let uri = "/api/admin/preview/episode?ratings_limit=2&ratings_order=imdb,tmdb";

    let res1 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    let body1 = res1.into_body().collect().await.unwrap().to_bytes();

    let res2 = app.clone().oneshot(authed_get(uri, &token)).await.unwrap();
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body1, body2);
}

#[tokio::test]
async fn preview_episode_position_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let params = "ratings_limit=3&ratings_order=imdb,tmdb,rt";

    let res_tr = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/episode?{params}&position=tr"), &token
    )).await.unwrap();
    let body_tr = res_tr.into_body().collect().await.unwrap().to_bytes();

    let res_bc = app.clone().oneshot(authed_get(
        &format!("/api/admin/preview/episode?{params}&position=bc"), &token
    )).await.unwrap();
    let body_bc = res_bc.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_tr[0], 0xFF);
    assert_eq!(body_bc[0], 0xFF);
    assert_ne!(body_tr, body_bc, "different positions should produce different images");
}

#[tokio::test]
async fn preview_episode_self_serve() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/episode?badge_size=s&blur=true")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_episode_accepts_all_badge_styles() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for style in ["h", "v", "d"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/episode?badge_style={style}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "episode badge_style={style} should be accepted");
    }
}

#[tokio::test]
async fn preview_episode_rejects_invalid_badge_style() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/episode?badge_style=z", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_episode_accepts_all_badge_sizes() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for size in ["xs", "s", "m", "l", "xl"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/episode?badge_size={size}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "episode badge_size={size} should be accepted");
    }
}

#[tokio::test]
async fn preview_episode_accepts_all_badge_directions() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    for dir in ["d", "h", "v"] {
        let res = app.clone().oneshot(authed_get(
            &format!("/api/admin/preview/episode?badge_direction={dir}"), &token
        )).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "episode badge_direction={dir} should be accepted");
    }
}

#[tokio::test]
async fn preview_episode_badge_style_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_h = app.clone().oneshot(authed_get("/api/admin/preview/episode?ratings_limit=3&badge_style=h", &token)).await.unwrap();
    let body_h = res_h.into_body().collect().await.unwrap().to_bytes();

    let res_v = app.clone().oneshot(authed_get("/api/admin/preview/episode?ratings_limit=3&badge_style=v", &token)).await.unwrap();
    let body_v = res_v.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(body_h[0], 0xFF);
    assert_eq!(body_v[0], 0xFF);
    assert_ne!(body_h, body_v, "horizontal and vertical episode badge styles should differ");
}

#[tokio::test]
async fn preview_episode_rejects_invalid_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/episode?position=invalid", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_episode_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/preview/episode")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Season preview tests ---

#[tokio::test]
async fn preview_season_returns_jpeg() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/season", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() > 100, "JPEG should have substantial content");
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_season_accepts_all_settings() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get(
        "/api/admin/preview/season?ratings_limit=3&ratings_order=imdb,tmdb,rt&position=tl&badge_style=h&label_style=i&badge_direction=h&badge_size=m&badge_shape=p&badge_background=k",
        &token
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body[0], 0xFF);
    assert_eq!(body[1], 0xD8);
}

#[tokio::test]
async fn preview_season_position_produces_different_images() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res_bc = app.clone().oneshot(authed_get("/api/admin/preview/season?position=bc", &token)).await.unwrap();
    let res_tr = app.clone().oneshot(authed_get("/api/admin/preview/season?position=tr", &token)).await.unwrap();
    assert_eq!(res_bc.status(), StatusCode::OK);
    assert_eq!(res_tr.status(), StatusCode::OK);
    let body_bc = res_bc.into_body().collect().await.unwrap().to_bytes();
    let body_tr = res_tr.into_body().collect().await.unwrap().to_bytes();
    assert_ne!(body_bc, body_tr, "different season positions should render differently");
}

#[tokio::test]
async fn preview_season_self_serve() {
    let (app, _state) = common::setup_test_app().await;
    let api_key_token = common::setup_api_key_session(&app).await;

    let req = Request::builder()
        .uri("/api/key/me/preview/season?badge_size=s&position=tr")
        .header("authorization", format!("Bearer {api_key_token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
}

#[tokio::test]
async fn preview_season_rejects_invalid_badge_style() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/season?badge_style=z", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_season_rejects_invalid_position() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/season?position=invalid", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_season_rejects_invalid_ratings_limit() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let res = app.clone().oneshot(authed_get("/api/admin/preview/season?ratings_limit=99", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn preview_season_requires_auth() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/admin/preview/season")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
