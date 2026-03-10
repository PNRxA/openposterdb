use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation};

use super::auth::{ApiKeyClaims, AuthUser, Claims};
use crate::AppState;

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(&state.jwt_secret),
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(AuthUser {
        username: token_data.claims.sub,
    });

    Ok(next.run(req).await)
}

#[derive(Clone, Debug)]
pub struct ApiKeyUser {
    pub key_id: i32,
}

pub async fn require_api_key_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = decode::<ApiKeyClaims>(
        token,
        &DecodingKey::from_secret(&state.jwt_secret),
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut()
        .insert(ApiKeyUser { key_id: token_data.claims.key_id });

    Ok(next.run(req).await)
}
