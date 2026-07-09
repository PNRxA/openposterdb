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
        .body(Body::from(r#"{"name":"override-test"}"#))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["key"].as_str().unwrap().to_string()
}

// --- Valid override parameters accepted ---

#[tokio::test]
async fn poster_badge_style_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_style=h"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_ratings_limit_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=3"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_ratings_exclude_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_exclude=rt,trakt"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_all_overrides_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_style=v&label_style=i&badge_size=l&badge_direction=h&position=tl&ratings_limit=5&ratings_order=imdb,tmdb&ratings_exclude=rt&poster_source=t&fanart_textless=false"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logo_badge_style_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?badge_style=h&ratings_limit=2&label_style=o"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_badge_size_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?badge_size=xl&badge_style=v"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Invalid override parameters rejected ---

#[tokio::test]
async fn poster_invalid_ratings_limit_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=11"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_ratings_order_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_order=bogus_source"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_ratings_exclude_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_exclude=bogus_source"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_badge_style_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_style=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_negative_ratings_limit_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=-1"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Poster-only params silently ignored on logo/backdrop ---

#[tokio::test]
async fn logo_poster_only_params_ignored() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    // position and badge_direction are poster-only but should not cause errors on logo
    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?position=tl&badge_direction=h&poster_source=f&fanart_textless=true"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_poster_only_params_ignored() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?position=br&badge_direction=v&poster_source=t"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- No overrides still works ---

#[tokio::test]
async fn poster_no_overrides_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- New image_source param name works on all image types ---

#[tokio::test]
async fn poster_image_source_param_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?image_source=f"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logo_image_source_param_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/logo-default/tt0000001.png?image_source=t"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_image_source_param_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?image_source=f"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Backward-compatible aliases still work ---

#[tokio::test]
async fn poster_source_alias_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    // Old `poster_source` param should still work via serde alias
    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?poster_source=f"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn fanart_textless_alias_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    // Old `fanart_textless` param should still work via serde alias
    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?fanart_textless=true"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn textless_new_param_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?textless=true"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Ratings limit boundary values ---

#[tokio::test]
async fn poster_ratings_limit_zero_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=0"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_ratings_limit_ten_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=10"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Quality (#1) + main-language (#6) overlay badges ---

/// Helper: GET a URL and assert it is not a 400.
async fn assert_not_bad_request(app: &axum::Router, uri: String) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST, "unexpected 400");
}

/// Helper: GET a URL and assert it IS a 400.
async fn assert_bad_request(app: &axum::Router, uri: String) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "expected 400");
}

#[tokio::test]
async fn quality_stacked_tiers_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality=4k,dv")).await;
}

#[tokio::test]
async fn quality_logo_style_accepted_all_types() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    for kind in ["poster-default", "logo-default", "backdrop-default"] {
        assert_not_bad_request(&app, format!("/{api_key}/imdb/{kind}/tt0000001.jpg?quality=1080p,hdr&quality_style=logo")).await;
    }
}

#[tokio::test]
async fn lang_icon_flag_and_text_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_icon=flag")).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg?lang_icon=text")).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_icon=off")).await;
}

#[tokio::test]
async fn lang_code_override_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_icon=flag&lang_code=ja")).await;
}

#[tokio::test]
async fn overlay_badges_survive_zero_ratings() {
    // Overlay badges are injected after rating preferences, so they must still
    // be requestable with ratings disabled.
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?ratings_limit=0&quality=4k&quality_style=logo&lang_icon=flag")).await;
}

#[tokio::test]
async fn invalid_quality_tier_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality=8k")).await;
}

#[tokio::test]
async fn invalid_quality_style_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality_style=bogus")).await;
}

#[tokio::test]
async fn invalid_lang_icon_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_icon=bogus")).await;
}

#[tokio::test]
async fn independent_overlay_positions_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    // Quality top-left, language bottom-right — independent of ratings.
    assert_not_bad_request(
        &app,
        format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality=4k,dv&quality_position=tl&lang_icon=flag&lang_position=br"),
    )
    .await;
    assert_not_bad_request(
        &app,
        format!("/{api_key}/imdb/backdrop-default/tt0000001.jpg?quality=hdr&quality_position=bl&lang_icon=text&lang_position=tr"),
    )
    .await;
}

#[tokio::test]
async fn invalid_overlay_position_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality_position=bogus")).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_position=xyz")).await;
}

#[tokio::test]
async fn quality_direction_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    for dir in ["d", "h", "v"] {
        assert_not_bad_request(
            &app,
            format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality=4k,dv&quality_direction={dir}"),
        )
        .await;
    }
}

#[tokio::test]
async fn invalid_quality_direction_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?quality_direction=diagonal")).await;
}

#[tokio::test]
async fn lang_exclude_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(
        &app,
        format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_icon=flag&lang_exclude=en,es"),
    )
    .await;
}

#[tokio::test]
async fn invalid_lang_exclude_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_bad_request(&app, format!("/{api_key}/imdb/poster-default/tt0000001.jpg?lang_exclude=english")).await;
}

#[tokio::test]
async fn episode_accepts_but_ignores_overlay_params() {
    // Episodes never render the quality/language overlay badges, but the shared
    // query params must still be accepted (ignored), not rejected.
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;
    assert_not_bad_request(
        &app,
        format!("/{api_key}/imdb/episode-default/episode-tt0903747-S1E1.jpg?quality=4k,dv&quality_style=logo&lang_icon=flag&quality_position=tl"),
    )
    .await;
}

// --- Badge shape / background / split / fit / edge inset query params ---

#[tokio::test]
async fn poster_badge_shape_and_background_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_shape=p&badge_background=k"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_split_and_fit_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?split=true&fit=cover"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backdrop_edge_inset_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/backdrop-default/tt0000001.jpg?edge_inset_x=8&edge_inset_y=3&position=bl"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_badge_shape_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_shape=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_badge_background_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?badge_background=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn poster_invalid_fit_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/imdb/poster-default/tt0000001.jpg?fit=bogus"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
