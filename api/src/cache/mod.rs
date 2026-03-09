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

fn is_safe_path_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.contains('\\') && !s.contains('\0')
}

pub fn cache_path(cache_dir: &str, id_type: &str, id_value: &str) -> Result<PathBuf, AppError> {
    if !is_safe_path_component(id_value) {
        return Err(AppError::BadRequest("invalid id value".into()));
    }
    Ok(Path::new(cache_dir).join(id_type).join(format!("{id_value}.jpg")))
}

pub fn poster_cache_path(cache_dir: &str, poster_path: &str) -> Result<PathBuf, AppError> {
    // poster_path is like "/abc123.jpg" from TMDB
    let filename = poster_path.trim_start_matches('/');
    if !is_safe_path_component(filename) {
        return Err(AppError::BadRequest("invalid poster path".into()));
    }
    Ok(Path::new(cache_dir).join("posters").join(filename))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_stale_no_release_date() {
        let result = compute_stale_secs(None, 86400, 31_536_000);
        assert_eq!(result, 86400);
    }

    #[test]
    fn compute_stale_invalid_date() {
        let result = compute_stale_secs(Some("not-a-date"), 86400, 31_536_000);
        assert_eq!(result, 86400);
    }

    #[test]
    fn compute_stale_future_film() {
        let result = compute_stale_secs(Some("2099-01-01"), 86400, 31_536_000);
        assert_eq!(result, 86400);
    }

    #[test]
    fn compute_stale_old_film() {
        // Film from 2000 — age far exceeds max_age of 1 year
        let result = compute_stale_secs(Some("2000-01-01"), 86400, 31_536_000);
        assert_eq!(result, 0);
    }

    #[test]
    fn date_str_to_epoch_known_value() {
        // 1970-01-02 should be exactly 86400 seconds
        assert_eq!(date_str_to_epoch("1970-01-02"), Some(86400));
    }

    #[test]
    fn date_str_to_epoch_epoch_start() {
        assert_eq!(date_str_to_epoch("1970-01-01"), Some(0));
    }

    #[test]
    fn date_str_to_epoch_invalid_month() {
        assert_eq!(date_str_to_epoch("2020-13-01"), None);
    }

    #[test]
    fn date_str_to_epoch_invalid_day() {
        assert_eq!(date_str_to_epoch("2020-01-32"), None);
    }

    #[test]
    fn date_str_to_epoch_pre_epoch() {
        assert_eq!(date_str_to_epoch("1969-01-01"), None);
    }

    #[test]
    fn date_str_to_epoch_garbage() {
        assert_eq!(date_str_to_epoch("garbage"), None);
    }

    #[test]
    fn cache_path_construction() {
        let p = cache_path("/tmp/cache", "imdb", "tt1234567").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/imdb/tt1234567.jpg"));
    }

    #[test]
    fn poster_cache_path_strips_leading_slash() {
        let p = poster_cache_path("/tmp/cache", "/abc123.jpg").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/posters/abc123.jpg"));
    }

    #[test]
    fn poster_cache_path_no_leading_slash() {
        let p = poster_cache_path("/tmp/cache", "abc123.jpg").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/posters/abc123.jpg"));
    }

    #[test]
    fn cache_path_rejects_traversal() {
        assert!(cache_path("/tmp/cache", "imdb", "../../etc/passwd").is_err());
        assert!(cache_path("/tmp/cache", "imdb", "..").is_err());
        assert!(cache_path("/tmp/cache", "imdb", ".").is_err());
        assert!(cache_path("/tmp/cache", "imdb", "").is_err());
        assert!(cache_path("/tmp/cache", "imdb", "foo/bar").is_err());
        assert!(cache_path("/tmp/cache", "imdb", "foo\\bar").is_err());
    }

    #[test]
    fn poster_cache_path_rejects_traversal() {
        assert!(poster_cache_path("/tmp/cache", "/../etc/passwd").is_err());
        assert!(poster_cache_path("/tmp/cache", "..").is_err());
        assert!(poster_cache_path("/tmp/cache", "").is_err());
    }

    #[test]
    fn is_leap_year_cases() {
        assert!(is_leap(2000)); // divisible by 400
        assert!(is_leap(2024)); // divisible by 4, not by 100
        assert!(!is_leap(1900)); // divisible by 100, not by 400
        assert!(!is_leap(2023)); // not divisible by 4
    }
}
