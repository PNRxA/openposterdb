use std::sync::Arc;

use axum::http::header::{self, HeaderValue};
use axum::http::Request;
use axum::middleware;
use axum::response::IntoResponse;
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{MakeSpan, TraceLayer};

use utoipa::OpenApi;

use crate::config::Config;
use crate::handlers;
use crate::{ApiDoc, AppState};

static OPENAPI_JSON: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let mut spec = ApiDoc::openapi();
    spec.info.version = env!("CARGO_PKG_VERSION").to_string();
    spec.to_json().expect("OpenAPI spec must serialize")
});

fn build_cors_layer(config: &Config) -> CorsLayer {
    match config.cors_origin {
        Some(ref origin) => CorsLayer::new()
            .allow_origin(AllowOrigin::exact(
                HeaderValue::from_str(origin).expect("valid CORS_ORIGIN"),
            ))
            .allow_methods(AllowMethods::list([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
            ]))
            .allow_headers(AllowHeaders::list([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
            ]))
            .allow_credentials(true),
        None => CorsLayer::new(),
    }
}

/// Returns true if `path` looks like an API management route (`/api/...`) or
/// an image route (first segment is a 64-char hex API key). Used by the
/// fallback service to return JSON 404 instead of the SPA HTML page.
fn is_api_or_image_path(path: &str) -> bool {
    if path.starts_with("/api/") || path == "/api" {
        return true;
    }
    // Content-addressed CDN routes
    if path.starts_with("/c/") {
        return true;
    }
    // Check if first path segment looks like an API key (64 lowercase hex chars).
    let Some(without_slash) = path.get(1..) else {
        return false;
    };
    let first_segment = match without_slash.find('/') {
        Some(pos) => &without_slash[..pos],
        None => without_slash,
    };
    first_segment.len() == 64 && first_segment.bytes().all(|b| b.is_ascii_hexdigit())
}

fn redact_path(path: &str) -> String {
    if !path.starts_with("/api/") && !path.starts_with("/c/") {
        // Poster route: /{api_key}/... -> /[REDACTED]/...
        match path.get(1..).and_then(|s| s.find('/')) {
            Some(pos) => format!("/[REDACTED]{}", &path[1 + pos..]),
            None => "/[REDACTED]".into(),
        }
    } else {
        path.to_string()
    }
}

#[derive(Clone)]
struct RedactedMakeSpan;

impl<B> MakeSpan<B> for RedactedMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> tracing::Span {
        let redacted_uri = redact_path(req.uri().path());
        tracing::info_span!("request", method = %req.method(), uri = %redacted_uri, version = ?req.version())
    }
}

pub fn build_app(state: Arc<AppState>) -> Router {
    let admin_routes = super::api_keys::api_key_routes()
        .merge(super::admin::admin_routes())
        .route(
            "/api/auth/logout",
            axum::routing::post(handlers::auth::logout),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::middleware::require_auth,
        ));

    let key_self_routes = super::api_keys::api_key_self_routes().layer(
        middleware::from_fn_with_state(
            state.clone(),
            handlers::middleware::require_api_key_auth,
        ),
    );

    let compressed_routes = Router::new()
        .merge(super::auth::auth_routes())
        .merge(admin_routes)
        .merge(key_self_routes)
        .route(
            "/api/openapi.json",
            axum::routing::get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "application/json"),
                        (header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    OPENAPI_JSON.as_str(),
                )
            }),
        )
        .layer(CompressionLayer::new());

    let mut app = Router::new()
        .merge(super::image::image_routes())
        .merge(super::image::is_valid_route());

    // Content-addressed CDN routes — only registered when ENABLE_CDN_REDIRECTS is set,
    // since nothing populates the settings_hash_registry otherwise.
    if state.config.enable_cdn_redirects {
        app = app.merge(super::image::cdn_routes());
    }

    let mut app = app
        .merge(compressed_routes);

    // Serve static frontend files when STATIC_DIR is set.
    // Falls back to index.html for SPA client-side routing, but returns
    // a proper JSON 404 for unmatched /api/ paths and paths that look like
    // image requests (64-char hex first segment) so API consumers get JSON
    // errors instead of HTML.
    if let Some(ref dir) = state.config.static_dir {
        use tower::ServiceExt as _;
        use tower_http::services::{ServeDir, ServeFile};

        let index = format!("{dir}/index.html");
        let spa = tower::ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::if_not_present(
                header::CACHE_CONTROL,
                HeaderValue::from_static(
                    "public, max-age=60, stale-while-revalidate=3600, stale-if-error=86400",
                ),
            ))
            .service(ServeDir::new(dir).fallback(ServeFile::new(index)));

        app = app.fallback_service(tower::service_fn(move |req: Request<axum::body::Body>| {
            let spa = spa.clone();
            async move {
                let path = req.uri().path();
                if is_api_or_image_path(path) {
                    // Constant-time-ish: always return the same JSON 404
                    // regardless of whether the key exists, to avoid leaking
                    // valid key prefixes via timing.
                    Ok((
                        axum::http::StatusCode::NOT_FOUND,
                        axum::Json(serde_json::json!({"error": "not found"})),
                    )
                        .into_response())
                } else {
                    spa.oneshot(req).await.map(|r| r.into_response())
                }
            }
        }));
    }

    let cors_layer = build_cors_layer(&state.config);

    app = app.layer(TraceLayer::new_for_http().make_span_with(RedactedMakeSpan));

    if state.secure_cookies {
        app = app.layer(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        ));
    }

    // Security headers — always present regardless of HTTPS mode
    app = app
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ));

    app.layer(cors_layer).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_path_api_route_unchanged() {
        assert_eq!(redact_path("/api/auth/status"), "/api/auth/status");
        assert_eq!(redact_path("/api/keys"), "/api/keys");
    }

    #[test]
    fn redact_path_image_route_hides_key() {
        assert_eq!(
            redact_path("/abc123def456/imdb/poster-default/tt1234567.jpg"),
            "/[REDACTED]/imdb/poster-default/tt1234567.jpg"
        );
    }

    #[test]
    fn redact_path_single_segment() {
        assert_eq!(redact_path("/abc123def456"), "/[REDACTED]");
    }

    #[test]
    fn redact_path_root() {
        // "/" — path[1..] is empty, find('/') returns None
        assert_eq!(redact_path("/"), "/[REDACTED]");
    }

    #[test]
    fn is_api_path() {
        assert!(is_api_or_image_path("/api/auth/login"));
        assert!(is_api_or_image_path("/api/keys"));
        assert!(is_api_or_image_path("/api"));
    }

    #[test]
    fn is_image_path_valid_key() {
        let key = "a".repeat(64);
        assert!(is_api_or_image_path(&format!("/{key}/imdb/poster-default/tt123.jpg")));
        assert!(is_api_or_image_path(&format!("/{key}/bad-path")));
        // Key alone (no trailing path)
        assert!(is_api_or_image_path(&format!("/{key}")));
    }

    #[test]
    fn is_image_path_invalid_key() {
        // Too short
        assert!(!is_api_or_image_path("/abcdef/imdb/poster-default/tt123.jpg"));
        // Not hex
        let key = "g".repeat(64);
        assert!(!is_api_or_image_path(&format!("/{key}/imdb/poster-default/tt123.jpg")));
    }

    #[test]
    fn spa_paths_not_matched() {
        assert!(!is_api_or_image_path("/"));
        assert!(!is_api_or_image_path("/login"));
        assert!(!is_api_or_image_path("/settings"));
        assert!(!is_api_or_image_path("/posters"));
    }

    #[test]
    fn cdn_paths_matched() {
        assert!(is_api_or_image_path("/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"));
        assert!(is_api_or_image_path("/c/abc123/tmdb/logo-default/12345.png"));
    }

    #[test]
    fn redact_path_cdn_route_unchanged() {
        assert_eq!(
            redact_path("/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"),
            "/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"
        );
    }
}
