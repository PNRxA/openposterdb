use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait,
};
use zeroize::Zeroizing;

use crate::entity::{admin_user, api_key, refresh_token};
use crate::error::AppError;

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

// --- Admin user CRUD ---

pub async fn count_admin_users(db: &DatabaseConnection) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    admin_user::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn create_admin_user(
    db: &DatabaseConnection,
    username: &str,
    password_hash: &str,
) -> Result<admin_user::Model, AppError> {
    let model = admin_user::ActiveModel {
        id: Default::default(),
        username: Set(username.to_owned()),
        password_hash: Set(password_hash.to_owned()),
        created_at: Set(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
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
        created_at: Set(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
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
    db: &DatabaseConnection,
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
        created_at: Set(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
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
    db: &DatabaseConnection,
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

pub async fn delete_expired_refresh_tokens(db: &DatabaseConnection) -> Result<u64, AppError> {
    let now = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
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
    db: &DatabaseConnection,
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
        created_at: Set(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
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
    db: &DatabaseConnection,
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

pub async fn list_api_keys(db: &DatabaseConnection) -> Result<Vec<api_key::Model>, AppError> {
    api_key::Entity::find()
        .all(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn delete_api_key(db: &DatabaseConnection, id: i32) -> Result<(), AppError> {
    api_key::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

pub async fn update_api_key_last_used(
    db: &DatabaseConnection,
    id: i32,
) -> Result<(), AppError> {
    let key = api_key::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    if let Some(key) = key {
        let mut active: api_key::ActiveModel = <api_key::Model as Into<api_key::ActiveModel>>::into(key);
        active.last_used_at = Set(Some(
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ));
        active
            .update(db)
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
    }

    Ok(())
}
