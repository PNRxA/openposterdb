use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use sea_orm::*;
use tokio::fs;

use crate::entity::poster_meta;
use crate::error::AppError;

pub struct CacheEntry {
    pub bytes: Vec<u8>,
    pub is_stale: bool,
}

#[derive(Clone)]
pub struct MemCacheEntry {
    pub bytes: bytes::Bytes,
    pub last_checked: Instant,
}

pub fn cache_path(cache_dir: &str, id_type: &str, id_value: &str) -> PathBuf {
    Path::new(cache_dir).join(id_type).join(format!("{id_value}.jpg"))
}

pub fn poster_cache_path(cache_dir: &str, poster_path: &str) -> PathBuf {
    // poster_path is like "/abc123.jpg" from TMDB
    let filename = poster_path.trim_start_matches('/');
    Path::new(cache_dir).join("posters").join(filename)
}

/// Read a cached file. `stale_secs = 0` means never stale.
pub async fn read(path: &Path, stale_secs: u64) -> Option<CacheEntry> {
    let bytes = fs::read(path).await.ok()?;
    let metadata = fs::metadata(path).await.ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        .as_secs();

    Some(CacheEntry {
        bytes,
        is_stale: stale_secs > 0 && age > stale_secs,
    })
}

pub async fn write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, bytes).await?;
    Ok(())
}

pub async fn read_meta_db(db: &DatabaseConnection, cache_key: &str) -> Option<String> {
    poster_meta::Entity::find_by_id(cache_key)
        .one(db)
        .await
        .ok()
        .flatten()
        .and_then(|m| m.release_date)
}

pub async fn upsert_meta_db(
    db: &DatabaseConnection,
    cache_key: &str,
    release_date: Option<&str>,
) -> Result<(), AppError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let model = poster_meta::ActiveModel {
        cache_key: Set(cache_key.to_string()),
        release_date: Set(release_date.map(|s| s.to_string())),
        created_at: Set(now),
        updated_at: Set(now),
    };

    poster_meta::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(poster_meta::Column::CacheKey)
                .update_columns([poster_meta::Column::ReleaseDate, poster_meta::Column::UpdatedAt])
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}

/// Parse "YYYY-MM-DD" to Unix epoch seconds. Returns `None` for invalid input.
fn date_str_to_epoch(s: &str) -> Option<u64> {
    let mut parts = s.split('-');
    let year: u64 = parts.next()?.parse().ok()?;
    let month: u64 = parts.next()?.parse().ok()?;
    let day: u64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || year < 1970 {
        return None;
    }

    // Days from epoch to start of year
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        days += days_in_month[m as usize] as u64;
        if m == 2 && is_leap(year) {
            days += 1;
        }
    }
    days += day - 1;
    Some(days * 86400)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Compute dynamic stale_secs based on release date.
/// Returns 0 (never stale) for films older than `max_age`.
pub fn compute_stale_secs(
    release_date_str: Option<&str>,
    min_stale: u64,
    max_age: u64,
) -> u64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let epoch = match release_date_str.and_then(date_str_to_epoch) {
        Some(e) => e,
        None => return min_stale,
    };

    if epoch > now {
        // Unreleased / future film
        return min_stale;
    }

    let film_age = now - epoch;
    if film_age >= max_age {
        return 0; // never stale
    }

    // Linear interpolation: min_stale at age=0, approaches max_age at age=max_age
    min_stale + film_age * (max_age - min_stale) / max_age
}
