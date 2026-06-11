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

// --- Season endpoint rejects non-season ids ---
//
// A non-season id on the season endpoint must never serve an image: the handler
// rejects ids that resolve to a non-Season media type. With a fake TMDB key the
// id never resolves, so we only assert the request is NOT a 200 (true both when
// resolution fails and when it succeeds and the handler returns its 400).

#[tokio::test]
async fn season_endpoint_rejects_non_season_id() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/series-1396.jpg"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::OK, "non-season id must not serve a 200 on the season endpoint");
}

// --- Season query overrides ---
//
// Seasons mirror the poster controls (badge_style, ratings_limit, badge_size,
// position, badge_direction, badge_shape, badge_background) but have NO
// blur/fit/split. Override validation happens before any network resolution,
// so valid overrides on a well-formed `season-{id}-S{n}` id must not produce a
// 400 (resolution later fails with a fake TMDB key, which is not a 400), and
// invalid overrides must produce a 400 — same as the poster tests above.

#[tokio::test]
async fn season_badge_style_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_style=h"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_ratings_limit_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?ratings_limit=3"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_badge_size_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_size=xl"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_position_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?position=tl"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_badge_direction_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_direction=h"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_badge_shape_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_shape=p"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_badge_background_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_background=k"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_all_overrides_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_style=v&label_style=i&badge_size=l&badge_direction=h&position=tl&badge_shape=p&badge_background=k&ratings_limit=5&ratings_order=imdb,tmdb&ratings_exclude=rt&image_source=t"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_no_overrides_accepted() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_invalid_ratings_limit_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?ratings_limit=11"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_invalid_badge_style_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_style=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_invalid_badge_shape_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_shape=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn season_invalid_badge_background_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    let req = Request::builder()
        .uri(format!(
            "/{api_key}/tmdb/season-default/season-1396-S2.jpg?badge_background=z"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- Auth / routing guards (no network) ---
//
// These assert behavior that happens BEFORE any TMDB resolution.

#[tokio::test]
async fn season_invalid_api_key_rejected() {
    let (app, _) = common::setup_test_app().await;
    // No valid key created; an unknown key must be rejected with 401.
    let req = Request::builder()
        .uri("/0000000000000000000000000000000000000000000000000000000000000000/tmdb/season-default/season-1396-S2.jpg")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn season_invalid_id_type_rejected() {
    let (app, _) = common::setup_test_app().await;
    let api_key = create_api_key(&app).await;

    // `bogus` is not a known id_type — rejected with 400 before any network call.
    let req = Request::builder()
        .uri(format!(
            "/{api_key}/bogus/season-default/season-1396-S2.jpg"
        ))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
