use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::Extension;
use axum::Json;
use jsonwebtoken::{encode, EncodingKey, Header as JwtHeader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::services::db;
use crate::services::validation;
use crate::AppState;

static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    hash_password("openposterdb-dummy-timing-pad").expect("failed to create dummy hash")
});

const ACCESS_TOKEN_EXPIRY_MINUTES: i64 = 15;
const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 7;
const REFRESH_TOKEN_MAX_AGE_SECS: i64 = REFRESH_TOKEN_EXPIRY_DAYS * 24 * 60 * 60;
const API_KEY_SESSION_EXPIRY_HOURS: i64 = 24;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyClaims {
    pub key_id: i32,
    pub exp: usize,
}

fn generate_salt() -> argon2::password_hash::SaltString {
    let mut raw = [0u8; 16];
    rand::fill(&mut raw);
    argon2::password_hash::SaltString::encode_b64(&raw).expect("valid salt")
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    use argon2::password_hash::PasswordHasher;
    use argon2::Argon2;

    let salt = generate_salt();
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::error!("Failed to hash password: {e}");
            AppError::BadRequest("Account operation failed".into())
        })
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    let parsed = PasswordHash::new(hash).map_err(|e| {
        tracing::error!("Invalid password hash in database: {e}");
        AppError::BadRequest("Authentication failed".into())
    })?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn create_token(username: &str, secret: &[u8]) -> Result<String, AppError> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::minutes(ACCESS_TOKEN_EXPIRY_MINUTES))
        .ok_or_else(|| AppError::BadRequest("Failed to compute token expiry".into()))?
        .timestamp() as usize;

    let claims = Claims {
        sub: username.to_owned(),
        exp,
    };

    encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {e}");
        AppError::BadRequest("Authentication failed".into())
    })
}

fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn hash_api_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn issue_token_pair(
    db: &impl sea_orm::ConnectionTrait,
    jwt_secret: &[u8],
    user_id: i32,
    username: &str,
) -> Result<(String, String), AppError> {
    let access_token = create_token(username, jwt_secret)?;
    let raw_refresh = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_refresh);
    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
        .ok_or_else(|| AppError::BadRequest("Failed to compute refresh token expiry".into()))?
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    db::create_refresh_token(db, user_id, &token_hash, &expires_at).await?;

    Ok((access_token, raw_refresh))
}

#[derive(Clone)]
pub struct AuthUser {
    pub username: String,
}

// --- Auth handlers ---

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

fn refresh_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let mut cookie = format!(
        "refresh_token={token}; HttpOnly; SameSite=Strict; Path=/api/auth/refresh; Max-Age={max_age_secs}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn extract_refresh_token_from_cookies(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let c = c.trim();
            c.strip_prefix("refresh_token=").map(|v| v.to_string())
        })
}

pub async fn auth_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    let count = db::count_admin_users(&state.db).await?;
    Ok(Json(json!({ "setup_required": count == 0 })))
}

pub async fn setup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetupRequest>,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    validation::validate_username(&req.username)?;
    validation::validate_password(&req.password)?;

    let password_hash = hash_password(&req.password)?;
    let user = db::create_first_admin_user(&state.db, &req.username, &password_hash).await?;

    tracing::info!("Admin account setup completed for user '{}'", req.username);

    let (token, raw_refresh) =
        issue_token_pair(&state.db, &state.jwt_secret, user.id, &user.username).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie(
            &raw_refresh,
            REFRESH_TOKEN_MAX_AGE_SECS,
            state.secure_cookies,
        ))
        .expect("valid cookie"),
    );

    Ok((headers, Json(json!({ "token": token }))))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    let user = match db::find_admin_user_by_username(&state.db, &req.username).await? {
        Some(user) => user,
        None => {
            // Perform a dummy password verification to prevent timing side-channel
            let _ = verify_password(&req.password, &DUMMY_HASH);
            tracing::warn!("Login failed: unknown username");
            return Err(AppError::Unauthorized);
        }
    };

    if !verify_password(&req.password, &user.password_hash)? {
        tracing::warn!("Login failed: incorrect password");
        return Err(AppError::Unauthorized);
    }

    tracing::info!("Admin login successful for user '{}'", req.username);

    let (token, raw_refresh) =
        issue_token_pair(&state.db, &state.jwt_secret, user.id, &user.username).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie(
            &raw_refresh,
            REFRESH_TOKEN_MAX_AGE_SECS,
            state.secure_cookies,
        ))
        .expect("valid cookie"),
    );

    Ok((headers, Json(json!({ "token": token }))))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    use sea_orm::TransactionTrait;

    let raw_token =
        extract_refresh_token_from_cookies(&req_headers).ok_or(AppError::Unauthorized)?;

    let txn = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let token_hash = hash_refresh_token(&raw_token);

    let stored = db::find_refresh_token_by_hash(&txn, &token_hash)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Validate expiry
    let expires_at =
        chrono::NaiveDateTime::parse_from_str(&stored.expires_at, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| AppError::DbError(format!("Invalid expiry format: {e}")))?;
    let expires_at_utc = expires_at.and_utc();
    if expires_at_utc < chrono::Utc::now() {
        db::delete_refresh_token(&txn, stored.id).await?;
        txn.commit()
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
        return Err(AppError::Unauthorized);
    }

    // Look up user
    let user = db::find_admin_user_by_id(&txn, stored.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Issue new token pair BEFORE deleting old
    let (token, raw_refresh) =
        issue_token_pair(&txn, &state.jwt_secret, user.id, &user.username).await?;

    // Rotation: delete old refresh token (single-use)
    db::delete_refresh_token(&txn, stored.id).await?;

    txn.commit()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie(
            &raw_refresh,
            REFRESH_TOKEN_MAX_AGE_SECS,
            state.secure_cookies,
        ))
        .expect("valid cookie"),
    );

    Ok((headers, Json(json!({ "token": token }))))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    let user = db::find_admin_user_by_username(&state.db, &auth_user.username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    db::delete_refresh_tokens_for_user(&state.db, user.id).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie("", 0, state.secure_cookies)).expect("valid cookie"),
    );

    Ok((headers, Json(json!({ "ok": true }))))
}

// --- API key login (issues a short-lived JWT for self-service UI) ---

fn create_api_key_token(key_id: i32, secret: &[u8]) -> Result<String, AppError> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(API_KEY_SESSION_EXPIRY_HOURS))
        .ok_or_else(|| AppError::BadRequest("Failed to compute token expiry".into()))?
        .timestamp() as usize;

    let claims = ApiKeyClaims { key_id, exp };

    encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| {
        tracing::error!("Failed to create API key JWT: {e}");
        AppError::BadRequest("Authentication failed".into())
    })
}

#[derive(Deserialize)]
pub struct KeyLoginRequest {
    api_key: String,
}

pub async fn key_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeyLoginRequest>,
) -> Result<Json<Value>, AppError> {
    let key_hash = hash_api_key(&req.api_key);

    let key = db::find_api_key_by_hash(&state.db, &key_hash)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = create_api_key_token(key.id, &state.jwt_secret)?;

    Ok(Json(json!({
        "token": token,
        "name": key.name,
        "key_prefix": key.key_prefix,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verify_password_roundtrip() {
        let hash = hash_password("testpassword").unwrap();
        assert!(verify_password("testpassword", &hash).unwrap());
    }

    #[test]
    fn verify_wrong_password() {
        let hash = hash_password("correct").unwrap();
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn hash_refresh_token_deterministic() {
        let a = hash_refresh_token("token123");
        let b = hash_refresh_token("token123");
        assert_eq!(a, b);
    }

    #[test]
    fn different_inputs_different_hashes() {
        let a = hash_refresh_token("token_a");
        let b = hash_refresh_token("token_b");
        assert_ne!(a, b);
    }

    #[test]
    fn refresh_cookie_secure() {
        let cookie = refresh_cookie("abc123", 604800, true);
        assert!(cookie.contains("refresh_token=abc123"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/api/auth/refresh"));
        assert!(cookie.contains("Max-Age=604800"));
        assert!(cookie.contains("; Secure"));
    }

    #[test]
    fn refresh_cookie_insecure() {
        let cookie = refresh_cookie("abc123", 604800, false);
        assert!(cookie.contains("refresh_token=abc123"));
        assert!(!cookie.contains("; Secure"));
    }

    #[test]
    fn refresh_cookie_zero_max_age() {
        let cookie = refresh_cookie("", 0, false);
        assert!(cookie.contains("refresh_token="));
        assert!(cookie.contains("Max-Age=0"));
    }

    #[test]
    fn extract_refresh_token_present() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("refresh_token=mytoken123"),
        );
        assert_eq!(
            extract_refresh_token_from_cookies(&headers),
            Some("mytoken123".to_string())
        );
    }

    #[test]
    fn extract_refresh_token_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=value"),
        );
        assert_eq!(extract_refresh_token_from_cookies(&headers), None);
    }

    #[test]
    fn extract_refresh_token_multiple_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("foo=bar; refresh_token=mytoken; baz=qux"),
        );
        assert_eq!(
            extract_refresh_token_from_cookies(&headers),
            Some("mytoken".to_string())
        );
    }

    #[test]
    fn extract_refresh_token_no_cookie_header() {
        let headers = HeaderMap::new();
        assert_eq!(extract_refresh_token_from_cookies(&headers), None);
    }
}
