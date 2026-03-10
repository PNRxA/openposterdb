use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::handlers;
use crate::AppState;

pub fn auth_routes() -> Router<Arc<AppState>> {
    let rate_limited = Router::new()
        .route("/api/auth/setup", post(handlers::auth::setup))
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/auth/refresh", post(handlers::auth::refresh))
        .route("/api/auth/key-login", post(handlers::auth::key_login));

    #[cfg(not(any(test, feature = "test-support")))]
    let rate_limited = {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use tower_governor::errors::GovernorError;
        use tower_governor::governor::GovernorConfigBuilder;
        use tower_governor::key_extractor::SmartIpKeyExtractor;
        use tower_governor::GovernorLayer;

        const BURST_SIZE: u32 = 10;
        const PERIOD_SECS: u64 = 8;
        const MAX_WAIT: u64 = BURST_SIZE as u64 * PERIOD_SECS;

        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(PERIOD_SECS)
                .burst_size(BURST_SIZE)
                .key_extractor(SmartIpKeyExtractor)
                .use_headers()
                .finish()
                .unwrap(),
        );

        let governor_layer = GovernorLayer::new(governor_conf).error_handler(
            move |err: GovernorError| match err {
                GovernorError::TooManyRequests {
                    wait_time,
                    headers,
                } => {
                    let capped = wait_time.min(MAX_WAIT);
                    let mut res = (
                        StatusCode::TOO_MANY_REQUESTS,
                        format!("Too many requests. Retry in {capped}s."),
                    )
                        .into_response();
                    if let Some(h) = headers {
                        res.headers_mut().extend(h);
                    }
                    res.headers_mut().insert("retry-after", capped.into());
                    res
                }
                other => other.into_response().map(axum::body::Body::from),
            },
        );

        rate_limited.layer(governor_layer)
    };

    Router::new()
        .route("/api/auth/status", get(handlers::auth::auth_status))
        .merge(rate_limited)
}
