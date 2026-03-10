use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::auth::AuthUser;
use super::middleware::ApiKeyUser;
use crate::error::AppError;
use crate::services::db::{self, validate_fanart_lang, validate_poster_source};
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
    state.api_key_cache.invalidate_all();
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
pub struct PosterSettingsResponse {
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
    pub fanart_available: bool,
    pub is_default: bool,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<PosterSettingsResponse>, AppError> {
    db::find_api_key_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::IdNotFound(format!("API key {id} not found")))?;
    let settings = db::get_effective_poster_settings(&state.db, id, None).await;
    Ok(Json(PosterSettingsResponse {
        poster_source: settings.poster_source,
        fanart_lang: settings.fanart_lang,
        fanart_textless: settings.fanart_textless,
        fanart_available: state.fanart.is_some(),
        is_default: settings.is_default,
    }))
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub poster_source: String,
    #[serde(default = "db::default_fanart_lang")]
    pub fanart_lang: String,
    #[serde(default)]
    pub fanart_textless: bool,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<Value>, AppError> {
    db::find_api_key_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::IdNotFound(format!("API key {id} not found")))?;
    validate_poster_source(&req.poster_source)?;
    validate_fanart_lang(&req.fanart_lang)?;
    db::upsert_api_key_settings(&state.db, id, &req.poster_source, &req.fanart_lang, req.fanart_textless).await?;
    state.settings_cache.invalidate(&id).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    db::find_api_key_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::IdNotFound(format!("API key {id} not found")))?;
    db::delete_api_key_settings(&state.db, id).await?;
    state.settings_cache.invalidate(&id).await;
    Ok(Json(json!({ "ok": true })))
}

// --- Self-service handlers (API key auth) ---

pub async fn get_own_key_info(
    State(state): State<Arc<AppState>>,
    Extension(api_key_user): Extension<ApiKeyUser>,
) -> Result<Json<Value>, AppError> {
    let key = db::find_api_key_by_id(&state.db, api_key_user.key_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(json!({
        "name": key.name,
        "key_prefix": key.key_prefix,
    })))
}

pub async fn get_own_settings(
    State(state): State<Arc<AppState>>,
    Extension(api_key_user): Extension<ApiKeyUser>,
) -> Result<Json<PosterSettingsResponse>, AppError> {
    let settings =
        db::get_effective_poster_settings(&state.db, api_key_user.key_id, None).await;
    Ok(Json(PosterSettingsResponse {
        poster_source: settings.poster_source,
        fanart_lang: settings.fanart_lang,
        fanart_textless: settings.fanart_textless,
        fanart_available: state.fanart.is_some(),
        is_default: settings.is_default,
    }))
}

pub async fn update_own_settings(
    State(state): State<Arc<AppState>>,
    Extension(api_key_user): Extension<ApiKeyUser>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<Value>, AppError> {
    let id = api_key_user.key_id;
    validate_poster_source(&req.poster_source)?;
    validate_fanart_lang(&req.fanart_lang)?;
    db::upsert_api_key_settings(&state.db, id, &req.poster_source, &req.fanart_lang, req.fanart_textless).await?;
    state.settings_cache.invalidate(&id).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn reset_own_settings(
    State(state): State<Arc<AppState>>,
    Extension(api_key_user): Extension<ApiKeyUser>,
) -> Result<Json<Value>, AppError> {
    let id = api_key_user.key_id;
    db::delete_api_key_settings(&state.db, id).await?;
    state.settings_cache.invalidate(&id).await;
    Ok(Json(json!({ "ok": true })))
}
