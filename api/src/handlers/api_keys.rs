use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::auth::AuthUser;
use crate::error::AppError;
use crate::services::db;
use crate::AppState;

#[derive(Serialize)]
pub struct ApiKeyResponse {
    pub id: i32,
    pub name: String,
    pub key_prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ApiKeyResponse>>, AppError> {
    let keys = db::list_api_keys(&state.db).await?;
    let response: Vec<ApiKeyResponse> = keys
        .into_iter()
        .map(|k| ApiKeyResponse {
            id: k.id,
            name: k.name,
            key_prefix: k.key_prefix,
            created_at: k.created_at,
            last_used_at: k.last_used_at,
        })
        .collect();
    Ok(Json(response))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<Value>, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Name must not be empty".into()));
    }

    // Look up the admin user to get their id
    let user = db::find_admin_user_by_username(&state.db, &auth_user.username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Generate random 32-byte key as hex
    let mut raw_bytes = [0u8; 32];
    rand::fill(&mut raw_bytes);
    let raw_key: String = raw_bytes.iter().map(|b| format!("{b:02x}")).collect();

    // Hash with SHA-256 for storage
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());

    // Store first 8 chars as prefix for display
    let key_prefix = raw_key[..8].to_string();

    let key_model =
        db::create_api_key(&state.db, &req.name, &key_hash, &key_prefix, user.id).await?;

    Ok(Json(json!({
        "id": key_model.id,
        "name": key_model.name,
        "key": raw_key,
        "key_prefix": key_model.key_prefix,
        "created_at": key_model.created_at,
    })))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    db::delete_api_key(&state.db, id).await?;
    Ok(Json(json!({ "ok": true })))
}
