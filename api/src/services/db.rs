use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use zeroize::Zeroizing;

use std::collections::HashMap;

use crate::entity::{admin_user, api_key, api_key_settings, global_settings, refresh_token};
use crate::error::AppError;
use crate::services::ratings::RatingSource;

pub fn default_fanart_lang() -> String {
    "en".to_string()
}

pub fn default_ratings_limit() -> i32 {
    3
}

pub fn default_logo_backdrop_ratings_limit() -> i32 {
    5
}

pub fn default_ratings_order() -> String {
    "mal,imdb,lb,rt,mc,rta,tmdb,trakt".to_string()
}

pub fn default_poster_position() -> String {
    "bottom-center".to_string()
}

pub fn default_poster_badge_style() -> String {
    "horizontal".to_string()
}

pub fn default_logo_badge_style() -> String {
    "vertical".to_string()
}

pub fn default_backdrop_badge_style() -> String {
    "vertical".to_string()
}

pub fn default_label_style() -> String {
    "icon".to_string()
}

/// Validate that poster_source is a known value.
pub fn validate_poster_source(source: &str) -> Result<(), AppError> {
    if source == "tmdb" || source == "fanart" {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "poster_source must be 'tmdb' or 'fanart'".into(),
        ))
    }
}

/// Validate ratings_limit is 0–8.
pub fn validate_ratings_limit(limit: i32) -> Result<(), AppError> {
    if (0..=8).contains(&limit) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "ratings_limit must be between 0 and 8".into(),
        ))
    }
}

/// Validate a comma-separated list of rating source keys (no duplicates).
pub fn validate_ratings_order(order: &str) -> Result<(), AppError> {
    if order.is_empty() {
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for key in order.split(',') {
        let key = key.trim();
        if RatingSource::from_key(key).is_none() {
            return Err(AppError::BadRequest(format!(
                "unknown rating source key: '{key}'. Valid keys: {}",
                RatingSource::all_keys().join(", ")
            )));
        }
        if !seen.insert(key) {
            return Err(AppError::BadRequest(format!(
                "duplicate rating source key: '{key}'"
            )));
        }
    }
    Ok(())
}

pub fn validate_badge_style(style: &str) -> Result<(), AppError> {
    match style {
        "horizontal" | "vertical" => Ok(()),
        _ => Err(AppError::BadRequest(
            "badge_style must be 'horizontal' or 'vertical'".into(),
        )),
    }
}

pub fn validate_label_style(style: &str) -> Result<(), AppError> {
    match style {
        "text" | "icon" => Ok(()),
        _ => Err(AppError::BadRequest(
            "label_style must be 'text' or 'icon'".into(),
        )),
    }
}

/// Validate that poster_position is a known value.
pub fn validate_poster_position(pos: &str) -> Result<(), AppError> {
    match pos {
        "bottom-center" | "top-center" | "left" | "right" => Ok(()),
        _ => Err(AppError::BadRequest(
            "poster_position must be 'bottom-center', 'top-center', 'left', or 'right'".into(),
        )),
    }
}

/// Validate a fanart language code: 2–5 ASCII alphanumeric chars or hyphens (e.g. "en", "pt-BR").
pub fn validate_fanart_lang(lang: &str) -> Result<(), AppError> {
    if lang.len() >= 2
        && lang.len() <= 5
        && lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "fanart_lang must be 2-5 ASCII alphanumeric characters (e.g. 'en', 'de', 'pt-BR')"
                .into(),
        ))
    }
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// --- Secret loading from env ---

pub fn load_secret_from_env(env_var: &str) -> Zeroizing<Vec<u8>> {
    match std::env::var(env_var) {
        Ok(hex) if !hex.is_empty() => {
            let bytes =
                hex_to_bytes(&hex).unwrap_or_else(|e| panic!("{env_var} is not valid hex: {e}"));
            if bytes.len() != 32 {
                panic!(
                    "{env_var} must be 32 bytes (64 hex chars), got {}",
                    bytes.len()
                );
            }
            tracing::info!("{env_var} loaded from environment");
            Zeroizing::new(bytes)
        }
        _ => {
            panic!(
                "{env_var} is not set. This is required.\n\
                 Generate one with: openssl rand -hex 32\n\
                 Then add it to your .env file."
            );
        }
    }
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Odd-length hex string".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_bytes_valid() {
        assert_eq!(hex_to_bytes("abcd").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    fn hex_to_bytes_empty() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn hex_to_bytes_full_32_bytes() {
        let hex = "00".repeat(32);
        let result = hex_to_bytes(&hex).unwrap();
        assert_eq!(result.len(), 32);
        assert!(result.iter().all(|&b| b == 0));
    }

    #[test]
    fn hex_to_bytes_odd_length() {
        assert!(hex_to_bytes("abc").is_err());
    }

    #[test]
    fn hex_to_bytes_invalid_chars() {
        assert!(hex_to_bytes("gg").is_err());
    }

    #[test]
    fn hex_to_bytes_uppercase() {
        assert_eq!(hex_to_bytes("ABCD").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    fn hex_to_bytes_mixed_case() {
        assert_eq!(hex_to_bytes("aBcD").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    #[should_panic(expected = "is not set")]
    fn load_secret_missing_env_var() {
        load_secret_from_env("OPENPOSTERDB_TEST_NONEXISTENT_SECRET_VAR");
    }

    #[test]
    #[should_panic(expected = "must be 32 bytes")]
    fn load_secret_wrong_length() {
        let var_name = "OPENPOSTERDB_TEST_SHORT_SECRET";
        unsafe { std::env::set_var(var_name, "abcd") };
        let result = std::panic::catch_unwind(|| load_secret_from_env(var_name));
        unsafe { std::env::remove_var(var_name) };
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn load_secret_valid_32_bytes() {
        let var_name = "OPENPOSTERDB_TEST_VALID_SECRET";
        let hex = "ab".repeat(32);
        unsafe { std::env::set_var(var_name, &hex) };
        let secret = load_secret_from_env(var_name);
        unsafe { std::env::remove_var(var_name) };
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn default_fanart_lang_returns_en() {
        assert_eq!(default_fanart_lang(), "en");
    }

    #[test]
    fn validate_ratings_limit_accepts_valid() {
        for i in 0..=8 {
            assert!(validate_ratings_limit(i).is_ok(), "limit {i} should be valid");
        }
    }

    #[test]
    fn validate_ratings_limit_rejects_negative() {
        assert!(validate_ratings_limit(-1).is_err());
    }

    #[test]
    fn validate_ratings_limit_rejects_too_large() {
        assert!(validate_ratings_limit(9).is_err());
        assert!(validate_ratings_limit(100).is_err());
    }

    #[test]
    fn validate_ratings_order_accepts_empty() {
        assert!(validate_ratings_order("").is_ok());
    }

    #[test]
    fn validate_ratings_order_accepts_valid_keys() {
        assert!(validate_ratings_order("imdb,tmdb,rt").is_ok());
        assert!(validate_ratings_order("mal,imdb,rta,trakt,lb,mc,tmdb,rt").is_ok());
    }

    #[test]
    fn validate_ratings_order_rejects_unknown_keys() {
        assert!(validate_ratings_order("imdb,bogus").is_err());
        assert!(validate_ratings_order("unknown").is_err());
    }

    #[test]
    fn validate_ratings_order_rejects_duplicates() {
        assert!(validate_ratings_order("imdb,imdb").is_err());
        assert!(validate_ratings_order("rt,tmdb,rt").is_err());
    }

    #[test]
    fn validate_fanart_lang_valid_codes() {
        assert!(validate_fanart_lang("en").is_ok());
        assert!(validate_fanart_lang("de").is_ok());
        assert!(validate_fanart_lang("fr").is_ok());
        assert!(validate_fanart_lang("ja").is_ok());
        assert!(validate_fanart_lang("pt-BR").is_ok());
        assert!(validate_fanart_lang("zh-CN").is_ok());
    }

    #[test]
    fn validate_fanart_lang_rejects_too_short() {
        assert!(validate_fanart_lang("e").is_err());
        assert!(validate_fanart_lang("").is_err());
    }

    #[test]
    fn validate_fanart_lang_rejects_too_long() {
        assert!(validate_fanart_lang("abcdef").is_err());
        assert!(validate_fanart_lang("toolongvalue").is_err());
    }

    #[test]
    fn validate_fanart_lang_rejects_special_chars() {
        assert!(validate_fanart_lang("../../").is_err());
        assert!(validate_fanart_lang("en\0").is_err());
        assert!(validate_fanart_lang("a b").is_err());
        assert!(validate_fanart_lang("en/de").is_err());
    }

    #[test]
    fn validate_poster_position_accepts_valid() {
        assert!(validate_poster_position("bottom-center").is_ok());
        assert!(validate_poster_position("top-center").is_ok());
        assert!(validate_poster_position("left").is_ok());
        assert!(validate_poster_position("right").is_ok());
    }

    #[test]
    fn validate_poster_position_rejects_invalid() {
        assert!(validate_poster_position("center").is_err());
        assert!(validate_poster_position("").is_err());
        assert!(validate_poster_position("bottom").is_err());
        assert!(validate_poster_position("top-left").is_err());
    }

    #[test]
    fn default_poster_position_returns_bottom_center() {
        assert_eq!(default_poster_position(), "bottom-center");
    }

    #[test]
    fn default_poster_badge_style_returns_horizontal() {
        assert_eq!(default_poster_badge_style(), "horizontal");
    }

    #[test]
    fn default_backdrop_badge_style_returns_vertical() {
        assert_eq!(default_backdrop_badge_style(), "vertical");
    }

    #[test]
    fn validate_badge_style_accepts_valid() {
        assert!(validate_badge_style("horizontal").is_ok());
        assert!(validate_badge_style("vertical").is_ok());
    }

    #[test]
    fn validate_badge_style_rejects_invalid() {
        assert!(validate_badge_style("diagonal").is_err());
        assert!(validate_badge_style("").is_err());
    }

    #[test]
    fn validate_label_style_accepts_valid() {
        assert!(validate_label_style("text").is_ok());
        assert!(validate_label_style("icon").is_ok());
    }

    #[test]
    fn validate_label_style_rejects_invalid() {
        assert!(validate_label_style("emoji").is_err());
        assert!(validate_label_style("").is_err());
    }

    #[test]
    fn default_label_style_returns_icon() {
        assert_eq!(default_label_style(), "icon");
    }
}

// --- Admin user CRUD ---

pub async fn count_admin_users(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    admin_user::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn create_admin_user(
    db: &impl ConnectionTrait,
    username: &str,
    password_hash: &str,
) -> Result<admin_user::Model, AppError> {
    let model = admin_user::ActiveModel {
        id: Default::default(),
        username: Set(username.to_owned()),
        password_hash: Set(password_hash.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = admin_user::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    admin_user::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created user".into()))
}

pub async fn create_first_admin_user(
    db: &DatabaseConnection,
    username: &str,
    password_hash: &str,
) -> Result<admin_user::Model, AppError> {
    use sea_orm::PaginatorTrait;

    let txn = db
        .begin()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let count = admin_user::Entity::find()
        .count(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    if count > 0 {
        txn.rollback()
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
        return Err(AppError::Forbidden("Setup already completed".into()));
    }

    let model = admin_user::ActiveModel {
        id: Default::default(),
        username: Set(username.to_owned()),
        password_hash: Set(password_hash.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = admin_user::Entity::insert(model)
        .exec(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let user = admin_user::Entity::find_by_id(result.last_insert_id)
        .one(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created user".into()))?;

    txn.commit()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(user)
}

pub async fn find_admin_user_by_username(
    db: &impl ConnectionTrait,
    username: &str,
) -> Result<Option<admin_user::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    admin_user::Entity::find()
        .filter(admin_user::Column::Username.eq(username))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn find_admin_user_by_id(
    db: &impl ConnectionTrait,
    id: i32,
) -> Result<Option<admin_user::Model>, AppError> {
    admin_user::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

// --- Refresh token CRUD ---

pub async fn create_refresh_token(
    db: &impl ConnectionTrait,
    user_id: i32,
    token_hash: &str,
    expires_at: &str,
) -> Result<refresh_token::Model, AppError> {
    let model = refresh_token::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        token_hash: Set(token_hash.to_owned()),
        expires_at: Set(expires_at.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = refresh_token::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    refresh_token::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created refresh token".into()))
}

pub async fn find_refresh_token_by_hash(
    db: &impl ConnectionTrait,
    token_hash: &str,
) -> Result<Option<refresh_token::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn delete_refresh_token(db: &impl ConnectionTrait, id: i32) -> Result<(), AppError> {
    refresh_token::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(())
}

pub async fn delete_refresh_tokens_for_user(
    db: &impl ConnectionTrait,
    user_id: i32,
) -> Result<(), AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    refresh_token::Entity::delete_many()
        .filter(refresh_token::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(())
}

pub async fn delete_expired_refresh_tokens(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    let now = now_utc();
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    let result = refresh_token::Entity::delete_many()
        .filter(refresh_token::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(result.rows_affected)
}

// --- API key CRUD ---

pub async fn create_api_key(
    db: &impl ConnectionTrait,
    name: &str,
    key_hash: &str,
    key_prefix: &str,
    created_by: i32,
) -> Result<api_key::Model, AppError> {
    let model = api_key::ActiveModel {
        id: Default::default(),
        name: Set(name.to_owned()),
        key_hash: Set(key_hash.to_owned()),
        key_prefix: Set(key_prefix.to_owned()),
        created_by: Set(created_by),
        created_at: Set(now_utc()),
        last_used_at: Set(None),
    };

    let result = api_key::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    api_key::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created API key".into()))
}

pub async fn find_api_key_by_hash(
    db: &impl ConnectionTrait,
    key_hash: &str,
) -> Result<Option<api_key::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    api_key::Entity::find()
        .filter(api_key::Column::KeyHash.eq(key_hash))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn list_api_keys(db: &impl ConnectionTrait) -> Result<Vec<api_key::Model>, AppError> {
    api_key::Entity::find()
        .all(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn find_api_key_by_id(
    db: &impl ConnectionTrait,
    id: i32,
) -> Result<Option<api_key::Model>, AppError> {
    api_key::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn delete_api_key(db: &impl ConnectionTrait, id: i32) -> Result<(), AppError> {
    api_key::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Poster meta queries ---

pub async fn count_poster_meta(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use crate::entity::poster_meta;
    use sea_orm::PaginatorTrait;
    poster_meta::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn count_api_keys(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    api_key::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn list_poster_meta_by_kind(
    db: &impl ConnectionTrait,
    image_type: crate::cache::ImageType,
    page: u64,
    page_size: u64,
) -> Result<(Vec<crate::entity::poster_meta::Model>, u64), AppError> {
    use crate::entity::poster_meta;
    use sea_orm::{PaginatorTrait, QueryFilter, ColumnTrait};

    let paginator = poster_meta::Entity::find()
        .filter(poster_meta::Column::ImageType.eq(image_type.db_value()))
        .paginate(db, page_size);
    let total = paginator.num_items().await.map_err(|e| AppError::DbError(e.to_string()))?;
    let items = paginator
        .fetch_page(page.saturating_sub(1))
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok((items, total))
}

pub async fn batch_update_last_used(
    db: &impl ConnectionTrait,
    ids: &[i32],
) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    let now = now_utc();
    use sea_orm::{ColumnTrait, QueryFilter, sea_query::Expr};
    for chunk in ids.chunks(100) {
        api_key::Entity::update_many()
            .col_expr(api_key::Column::LastUsedAt, Expr::value(now.clone()))
            .filter(api_key::Column::Id.is_in(chunk.iter().copied()))
            .exec(db)
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
    }
    Ok(())
}

// --- Global settings ---

pub async fn get_global_settings(
    db: &impl ConnectionTrait,
) -> Result<HashMap<String, String>, AppError> {
    let rows = global_settings::Entity::find()
        .all(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
}

pub async fn set_global_setting(
    db: &impl ConnectionTrait,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    let model = global_settings::ActiveModel {
        key: Set(key.to_string()),
        value: Set(value.to_string()),
    };
    global_settings::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(global_settings::Column::Key)
                .update_column(global_settings::Column::Value)
                .to_owned(),
        )
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Per-key settings ---

pub async fn get_api_key_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
) -> Result<Option<api_key_settings::Model>, AppError> {
    api_key_settings::Entity::find_by_id(api_key_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub struct UpsertApiKeySettings<'a> {
    pub api_key_id: i32,
    pub poster_source: &'a str,
    pub fanart_lang: &'a str,
    pub fanart_textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: &'a str,
    pub poster_position: &'a str,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: &'a str,
    pub logo_badge_style: &'a str,
    pub backdrop_badge_style: &'a str,
    pub poster_label_style: &'a str,
    pub logo_label_style: &'a str,
    pub backdrop_label_style: &'a str,
}

pub async fn upsert_api_key_settings(
    db: &impl ConnectionTrait,
    params: UpsertApiKeySettings<'_>,
) -> Result<(), AppError> {
    let model = api_key_settings::ActiveModel {
        api_key_id: Set(params.api_key_id),
        poster_source: Set(params.poster_source.to_string()),
        fanart_lang: Set(params.fanart_lang.to_string()),
        fanart_textless: Set(params.fanart_textless),
        ratings_limit: Set(params.ratings_limit),
        ratings_order: Set(params.ratings_order.to_string()),
        poster_position: Set(params.poster_position.to_string()),
        logo_ratings_limit: Set(params.logo_ratings_limit),
        backdrop_ratings_limit: Set(params.backdrop_ratings_limit),
        poster_badge_style: Set(params.poster_badge_style.to_string()),
        logo_badge_style: Set(params.logo_badge_style.to_string()),
        backdrop_badge_style: Set(params.backdrop_badge_style.to_string()),
        poster_label_style: Set(params.poster_label_style.to_string()),
        logo_label_style: Set(params.logo_label_style.to_string()),
        backdrop_label_style: Set(params.backdrop_label_style.to_string()),
    };
    api_key_settings::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(api_key_settings::Column::ApiKeyId)
                .update_columns([
                    api_key_settings::Column::PosterSource,
                    api_key_settings::Column::FanartLang,
                    api_key_settings::Column::FanartTextless,
                    api_key_settings::Column::RatingsLimit,
                    api_key_settings::Column::RatingsOrder,
                    api_key_settings::Column::PosterPosition,
                    api_key_settings::Column::LogoRatingsLimit,
                    api_key_settings::Column::BackdropRatingsLimit,
                    api_key_settings::Column::PosterBadgeStyle,
                    api_key_settings::Column::LogoBadgeStyle,
                    api_key_settings::Column::BackdropBadgeStyle,
                    api_key_settings::Column::PosterLabelStyle,
                    api_key_settings::Column::LogoLabelStyle,
                    api_key_settings::Column::BackdropLabelStyle,
                ])
                .to_owned(),
        )
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

pub async fn delete_api_key_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
) -> Result<(), AppError> {
    api_key_settings::Entity::delete_by_id(api_key_id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Effective poster settings ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct PosterSettings {
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub is_default: bool,
    pub poster_position: String,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: String,
    pub logo_badge_style: String,
    pub backdrop_badge_style: String,
    pub poster_label_style: String,
    pub logo_label_style: String,
    pub backdrop_label_style: String,
}

impl Default for PosterSettings {
    fn default() -> Self {
        Self {
            poster_source: "tmdb".to_string(),
            fanart_lang: "en".to_string(),
            fanart_textless: false,
            ratings_limit: 3,
            ratings_order: "mal,imdb,lb,rt,mc,rta,tmdb,trakt".to_string(),
            is_default: true,
            poster_position: "bottom-center".to_string(),
            logo_ratings_limit: 5,
            backdrop_ratings_limit: 5,
            poster_badge_style: "horizontal".to_string(),
            logo_badge_style: "vertical".to_string(),
            backdrop_badge_style: "vertical".to_string(),
            poster_label_style: "icon".to_string(),
            logo_label_style: "icon".to_string(),
            backdrop_label_style: "icon".to_string(),
        }
    }
}

/// Parse raw global settings (key-value HashMap) into a PosterSettings struct.
pub fn parse_global_poster_settings(globals: &HashMap<String, String>) -> PosterSettings {
    if globals.is_empty() {
        return PosterSettings::default();
    }
    let defaults = PosterSettings::default();
    PosterSettings {
        poster_source: globals
            .get("poster_source")
            .cloned()
            .unwrap_or(defaults.poster_source),
        fanart_lang: globals
            .get("fanart_lang")
            .cloned()
            .unwrap_or(defaults.fanart_lang),
        fanart_textless: globals
            .get("fanart_textless")
            .map(|v| v == "true")
            .unwrap_or(defaults.fanart_textless),
        ratings_limit: globals
            .get("ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.ratings_limit),
        ratings_order: globals
            .get("ratings_order")
            .cloned()
            .unwrap_or(defaults.ratings_order),
        is_default: true,
        poster_position: globals
            .get("poster_position")
            .cloned()
            .unwrap_or(defaults.poster_position),
        logo_ratings_limit: globals
            .get("logo_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.logo_ratings_limit),
        backdrop_ratings_limit: globals
            .get("backdrop_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.backdrop_ratings_limit),
        poster_badge_style: globals
            .get("poster_badge_style")
            .cloned()
            .unwrap_or(defaults.poster_badge_style),
        logo_badge_style: globals
            .get("logo_badge_style")
            .cloned()
            .unwrap_or(defaults.logo_badge_style),
        backdrop_badge_style: globals
            .get("backdrop_badge_style")
            .cloned()
            .unwrap_or(defaults.backdrop_badge_style),
        poster_label_style: globals
            .get("poster_label_style")
            .cloned()
            .unwrap_or(defaults.poster_label_style),
        logo_label_style: globals
            .get("logo_label_style")
            .cloned()
            .unwrap_or(defaults.logo_label_style),
        backdrop_label_style: globals
            .get("backdrop_label_style")
            .cloned()
            .unwrap_or(defaults.backdrop_label_style),
    }
}

pub async fn get_effective_poster_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
    cached_globals: Option<&PosterSettings>,
) -> PosterSettings {
    // Check per-key settings first
    match get_api_key_settings(db, api_key_id).await {
        Ok(Some(s)) => {
            return PosterSettings {
                poster_source: s.poster_source,
                fanart_lang: s.fanart_lang,
                fanart_textless: s.fanart_textless,
                ratings_limit: s.ratings_limit,
                ratings_order: s.ratings_order,
                is_default: false,
                poster_position: s.poster_position,
                logo_ratings_limit: s.logo_ratings_limit,
                backdrop_ratings_limit: s.backdrop_ratings_limit,
                poster_badge_style: s.poster_badge_style,
                logo_badge_style: s.logo_badge_style,
                backdrop_badge_style: s.backdrop_badge_style,
                poster_label_style: s.poster_label_style,
                logo_label_style: s.logo_label_style,
                backdrop_label_style: s.backdrop_label_style,
            };
        }
        Ok(None) => {} // no per-key override, fall through
        Err(e) => {
            tracing::warn!(error = %e, api_key_id, "failed to load per-key settings, falling back");
        }
    }
    // Use cached global settings if provided
    if let Some(globals) = cached_globals {
        return globals.clone();
    }
    // Otherwise load from DB
    match get_global_settings(db).await {
        Ok(ref globals) => parse_global_poster_settings(globals),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load global settings, using defaults");
            PosterSettings::default()
        }
    }
}

pub async fn set_global_settings_batch(
    db: &DatabaseConnection,
    settings: &[(&str, &str)],
) -> Result<(), AppError> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    for (key, value) in settings {
        set_global_setting(&txn, key, value).await?;
    }
    txn.commit()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

