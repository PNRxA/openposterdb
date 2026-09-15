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
use crate::services::db::{self, default_ratings_limit, default_logo_backdrop_ratings_limit, default_ratings_order, BadgeBackground, BadgeDirection, BadgeShape, BadgeSize, BadgeStyle, LabelStyle, BadgePosition, ImageSource, PosterFit};
use crate::services::validation;
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
    validation::validate_api_key_name(&req.name)?;

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
pub struct RenderSettingsResponse {
    pub image_source: ImageSource,
    pub lang: String,
    pub textless: bool,
    pub fanart_available: bool,
    pub is_default: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub ratings_exclude: String,
    pub poster_position: BadgePosition,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: BadgeStyle,
    pub logo_badge_style: BadgeStyle,
    pub backdrop_badge_style: BadgeStyle,
    pub poster_label_style: LabelStyle,
    pub logo_label_style: LabelStyle,
    pub backdrop_label_style: LabelStyle,
    pub poster_badge_direction: BadgeDirection,
    pub poster_badge_split: bool,
    pub poster_fit: PosterFit,
    pub poster_badge_size: BadgeSize,
    pub logo_badge_size: BadgeSize,
    pub backdrop_badge_size: BadgeSize,
    pub backdrop_position: BadgePosition,
    pub backdrop_badge_direction: BadgeDirection,
    pub backdrop_edge_inset_x: i32,
    pub backdrop_edge_inset_y: i32,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: BadgeStyle,
    pub episode_label_style: LabelStyle,
    pub episode_badge_size: BadgeSize,
    pub episode_position: BadgePosition,
    pub episode_badge_direction: BadgeDirection,
    pub episode_blur: bool,
    pub poster_badge_shape: BadgeShape,
    pub logo_badge_shape: BadgeShape,
    pub backdrop_badge_shape: BadgeShape,
    pub episode_badge_shape: BadgeShape,
    pub poster_badge_background: BadgeBackground,
    pub logo_badge_background: BadgeBackground,
    pub backdrop_badge_background: BadgeBackground,
    pub episode_badge_background: BadgeBackground,
    pub season_ratings_limit: i32,
    pub season_badge_style: BadgeStyle,
    pub season_label_style: LabelStyle,
    pub season_badge_size: BadgeSize,
    pub season_position: BadgePosition,
    pub season_badge_direction: BadgeDirection,
    pub season_badge_shape: BadgeShape,
    pub season_badge_background: BadgeBackground,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<RenderSettingsResponse>, AppError> {
    db::find_api_key_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::IdNotFound(format!("API key {id} not found")))?;
    let settings = db::get_effective_render_settings(&state.db, id, None).await;
    Ok(Json(settings_to_response(&settings, state.fanart.is_some())))
}

fn settings_to_response(settings: &db::RenderSettings, fanart_available: bool) -> RenderSettingsResponse {
    RenderSettingsResponse {
        image_source: settings.image_source,
        lang: settings.lang.to_string(),
        textless: settings.textless,
        fanart_available,
        is_default: settings.is_default,
        ratings_limit: settings.ratings_limit,
        ratings_order: settings.ratings_order.to_string(),
        ratings_exclude: settings.ratings_exclude.to_string(),
        poster_position: settings.poster_position,
        logo_ratings_limit: settings.logo_ratings_limit,
        backdrop_ratings_limit: settings.backdrop_ratings_limit,
        poster_badge_style: settings.poster_badge_style,
        logo_badge_style: settings.logo_badge_style,
        backdrop_badge_style: settings.backdrop_badge_style,
        poster_label_style: settings.poster_label_style,
        logo_label_style: settings.logo_label_style,
        backdrop_label_style: settings.backdrop_label_style,
        poster_badge_direction: settings.poster_badge_direction,
        poster_badge_split: settings.poster_badge_split,
        poster_fit: settings.poster_fit,
        poster_badge_size: settings.poster_badge_size,
        logo_badge_size: settings.logo_badge_size,
        backdrop_badge_size: settings.backdrop_badge_size,
        backdrop_position: settings.backdrop_position,
        backdrop_badge_direction: settings.backdrop_badge_direction,
        backdrop_edge_inset_x: settings.backdrop_edge_inset_x,
        backdrop_edge_inset_y: settings.backdrop_edge_inset_y,
        episode_ratings_limit: settings.episode_ratings_limit,
        episode_badge_style: settings.episode_badge_style,
        episode_label_style: settings.episode_label_style,
        episode_badge_size: settings.episode_badge_size,
        episode_position: settings.episode_position,
        episode_badge_direction: settings.episode_badge_direction,
        episode_blur: settings.episode_blur,
        poster_badge_shape: settings.poster_badge_shape,
        logo_badge_shape: settings.logo_badge_shape,
        backdrop_badge_shape: settings.backdrop_badge_shape,
        episode_badge_shape: settings.episode_badge_shape,
        poster_badge_background: settings.poster_badge_background,
        logo_badge_background: settings.logo_badge_background,
        backdrop_badge_background: settings.backdrop_badge_background,
        episode_badge_background: settings.episode_badge_background,
        season_ratings_limit: settings.season_ratings_limit,
        season_badge_style: settings.season_badge_style,
        season_label_style: settings.season_label_style,
        season_badge_size: settings.season_badge_size,
        season_position: settings.season_position,
        season_badge_direction: settings.season_badge_direction,
        season_badge_shape: settings.season_badge_shape,
        season_badge_background: settings.season_badge_background,
    }
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(alias = "poster_source")]
    pub image_source: ImageSource,
    #[serde(default = "db::default_lang", alias = "fanart_lang")]
    pub lang: String,
    #[serde(default, alias = "fanart_textless")]
    pub textless: bool,
    #[serde(default = "default_ratings_limit")]
    pub ratings_limit: i32,
    #[serde(default = "default_ratings_order")]
    pub ratings_order: String,
    #[serde(default = "db::default_ratings_exclude")]
    pub ratings_exclude: String,
    #[serde(default = "db::default_poster_position")]
    pub poster_position: BadgePosition,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub logo_ratings_limit: i32,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub backdrop_ratings_limit: i32,
    #[serde(default = "db::default_poster_badge_style")]
    pub poster_badge_style: BadgeStyle,
    #[serde(default = "db::default_logo_badge_style")]
    pub logo_badge_style: BadgeStyle,
    #[serde(default = "db::default_backdrop_badge_style")]
    pub backdrop_badge_style: BadgeStyle,
    #[serde(default = "db::default_label_style")]
    pub poster_label_style: LabelStyle,
    #[serde(default = "db::default_label_style")]
    pub logo_label_style: LabelStyle,
    #[serde(default = "db::default_label_style")]
    pub backdrop_label_style: LabelStyle,
    #[serde(default = "db::default_poster_badge_direction")]
    pub poster_badge_direction: BadgeDirection,
    #[serde(default)]
    pub poster_badge_split: bool,
    #[serde(default = "db::default_poster_fit")]
    pub poster_fit: PosterFit,
    #[serde(default = "db::default_badge_size")]
    pub poster_badge_size: BadgeSize,
    #[serde(default = "db::default_badge_size")]
    pub logo_badge_size: BadgeSize,
    #[serde(default = "db::default_badge_size")]
    pub backdrop_badge_size: BadgeSize,
    #[serde(default = "db::default_backdrop_position")]
    pub backdrop_position: BadgePosition,
    #[serde(default = "db::default_backdrop_badge_direction")]
    pub backdrop_badge_direction: BadgeDirection,
    #[serde(default = "db::default_backdrop_edge_inset")]
    pub backdrop_edge_inset_x: i32,
    #[serde(default = "db::default_backdrop_edge_inset")]
    pub backdrop_edge_inset_y: i32,
    #[serde(default = "db::default_episode_ratings_limit")]
    pub episode_ratings_limit: i32,
    #[serde(default = "db::default_episode_badge_style")]
    pub episode_badge_style: BadgeStyle,
    #[serde(default = "db::default_label_style")]
    pub episode_label_style: LabelStyle,
    #[serde(default = "db::default_episode_badge_size")]
    pub episode_badge_size: BadgeSize,
    #[serde(default = "db::default_episode_position")]
    pub episode_position: BadgePosition,
    #[serde(default = "db::default_episode_badge_direction")]
    pub episode_badge_direction: BadgeDirection,
    #[serde(default)]
    pub episode_blur: bool,
    #[serde(default = "db::default_badge_shape")]
    pub poster_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub logo_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub backdrop_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub episode_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_background")]
    pub poster_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub logo_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub backdrop_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub episode_badge_background: BadgeBackground,
    #[serde(default = "db::default_season_ratings_limit")]
    pub season_ratings_limit: i32,
    #[serde(default = "db::default_season_badge_style")]
    pub season_badge_style: BadgeStyle,
    #[serde(default = "db::default_label_style")]
    pub season_label_style: LabelStyle,
    #[serde(default = "db::default_season_badge_size")]
    pub season_badge_size: BadgeSize,
    #[serde(default = "db::default_season_position")]
    pub season_position: BadgePosition,
    #[serde(default = "db::default_season_badge_direction")]
    pub season_badge_direction: BadgeDirection,
    #[serde(default = "db::default_badge_shape")]
    pub season_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_background")]
    pub season_badge_background: BadgeBackground,
}

fn build_upsert(id: i32, req: &UpdateSettingsRequest) -> db::UpsertApiKeySettings<'_> {
    db::UpsertApiKeySettings {
        api_key_id: id,
        image_source: req.image_source.as_str(),
        lang: &req.lang,
        textless: req.textless,
        ratings_limit: req.ratings_limit,
        ratings_order: &req.ratings_order,
        ratings_exclude: &req.ratings_exclude,
        poster_position: req.poster_position.as_str(),
        logo_ratings_limit: req.logo_ratings_limit,
        backdrop_ratings_limit: req.backdrop_ratings_limit,
        poster_badge_style: req.poster_badge_style.as_str(),
        logo_badge_style: req.logo_badge_style.as_str(),
        backdrop_badge_style: req.backdrop_badge_style.as_str(),
        poster_label_style: req.poster_label_style.as_str(),
        logo_label_style: req.logo_label_style.as_str(),
        backdrop_label_style: req.backdrop_label_style.as_str(),
        poster_badge_direction: req.poster_badge_direction.as_str(),
        poster_badge_split: req.poster_badge_split,
        poster_fit: req.poster_fit.as_str(),
        poster_badge_size: req.poster_badge_size.as_str(),
        logo_badge_size: req.logo_badge_size.as_str(),
        backdrop_badge_size: req.backdrop_badge_size.as_str(),
        backdrop_position: req.backdrop_position.as_str(),
        backdrop_badge_direction: req.backdrop_badge_direction.as_str(),
        episode_ratings_limit: req.episode_ratings_limit,
        episode_badge_style: req.episode_badge_style.as_str(),
        episode_label_style: req.episode_label_style.as_str(),
        episode_badge_size: req.episode_badge_size.as_str(),
        episode_position: req.episode_position.as_str(),
        episode_badge_direction: req.episode_badge_direction.as_str(),
        episode_blur: req.episode_blur,
        poster_badge_shape: req.poster_badge_shape.as_str(),
        logo_badge_shape: req.logo_badge_shape.as_str(),
        backdrop_badge_shape: req.backdrop_badge_shape.as_str(),
        episode_badge_shape: req.episode_badge_shape.as_str(),
        poster_badge_background: req.poster_badge_background.as_str(),
        logo_badge_background: req.logo_badge_background.as_str(),
        backdrop_badge_background: req.backdrop_badge_background.as_str(),
        episode_badge_background: req.episode_badge_background.as_str(),
        backdrop_edge_inset_x: db::clamp_edge_inset(req.backdrop_edge_inset_x),
        backdrop_edge_inset_y: db::clamp_edge_inset(req.backdrop_edge_inset_y),
        season_ratings_limit: req.season_ratings_limit,
        season_badge_style: req.season_badge_style.as_str(),
        season_label_style: req.season_label_style.as_str(),
        season_badge_size: req.season_badge_size.as_str(),
        season_position: req.season_position.as_str(),
        season_badge_direction: req.season_badge_direction.as_str(),
        season_badge_shape: req.season_badge_shape.as_str(),
        season_badge_background: req.season_badge_background.as_str(),
    }
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<Value>, AppError> {
    db::find_api_key_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::IdNotFound(format!("API key {id} not found")))?;
    db::validate_render_settings(&req.lang, req.ratings_limit, &req.ratings_order, &req.ratings_exclude, req.logo_ratings_limit, req.backdrop_ratings_limit, req.episode_ratings_limit, req.season_ratings_limit)?;
    db::upsert_api_key_settings(&state.db, build_upsert(id, &req)).await?;
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
) -> Result<Json<RenderSettingsResponse>, AppError> {
    let settings =
        db::get_effective_render_settings(&state.db, api_key_user.key_id, None).await;
    Ok(Json(settings_to_response(&settings, state.fanart.is_some())))
}

pub async fn update_own_settings(
    State(state): State<Arc<AppState>>,
    Extension(api_key_user): Extension<ApiKeyUser>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<Value>, AppError> {
    let id = api_key_user.key_id;
    db::validate_render_settings(&req.lang, req.ratings_limit, &req.ratings_order, &req.ratings_exclude, req.logo_ratings_limit, req.backdrop_ratings_limit, req.episode_ratings_limit, req.season_ratings_limit)?;
    db::upsert_api_key_settings(&state.db, build_upsert(id, &req)).await?;
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
