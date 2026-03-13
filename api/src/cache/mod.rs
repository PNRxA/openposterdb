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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Poster,
    Logo,
    Backdrop,
}

impl ImageType {
    pub fn subdir(self) -> &'static str {
        match self {
            ImageType::Poster => "posters",
            ImageType::Logo => "logos",
            ImageType::Backdrop => "backdrops",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ImageType::Poster | ImageType::Backdrop => "jpg",
            ImageType::Logo => "png",
        }
    }

    pub fn db_value(self) -> &'static str {
        match self {
            ImageType::Poster => "p",
            ImageType::Logo => "l",
            ImageType::Backdrop => "b",
        }
    }

    /// Cache key prefix for image kind (poster has none).
    pub fn kind_prefix(self) -> &'static str {
        match self {
            ImageType::Poster => "",
            ImageType::Logo => "_l",
            ImageType::Backdrop => "_b",
        }
    }

    /// Strip the file extension matching this image type from a string.
    pub fn strip_ext(self, s: &str) -> &str {
        match self {
            ImageType::Poster | ImageType::Backdrop => s.strip_suffix(".jpg").unwrap_or(s),
            ImageType::Logo => s.strip_suffix(".png").unwrap_or(s),
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            ImageType::Poster => "poster",
            ImageType::Logo => "logo",
            ImageType::Backdrop => "backdrop",
        }
    }
}

/// Path for a rendered (composited) image: `{cache_dir}/{subdir}/{id_type}/{id_value}.{ext}`
pub fn typed_cache_path(
    cache_dir: &str,
    image_type: ImageType,
    id_type: &str,
    id_value: &str,
) -> Result<PathBuf, AppError> {
    if !is_safe_path_component(id_value) {
        return Err(AppError::BadRequest("invalid id value".into()));
    }
    if !is_safe_path_component(id_type) {
        return Err(AppError::BadRequest("invalid id type".into()));
    }
    let ext = image_type.ext();
    Ok(Path::new(cache_dir)
        .join(image_type.subdir())
        .join(id_type)
        .join(format!("{id_value}.{ext}")))
}

/// Path for a TMDB base poster: `{cache_dir}/base/posters/{tmdb_size}/{filename}`
pub fn base_poster_path(cache_dir: &str, poster_path: &str, tmdb_size: &str) -> Result<PathBuf, AppError> {
    // poster_path is like "/abc123.jpg" from TMDB
    let filename = poster_path.trim_start_matches('/');
    if !is_safe_path_component(filename) {
        return Err(AppError::BadRequest("invalid poster path".into()));
    }
    if !is_safe_path_component(tmdb_size) {
        return Err(AppError::BadRequest("invalid tmdb size".into()));
    }
    Ok(Path::new(cache_dir).join("base").join("posters").join(tmdb_size).join(filename))
}

/// Path for a fanart base image: `{cache_dir}/base/fanart/{fanart_id}.{ext}`
pub fn base_fanart_path(cache_dir: &str, fanart_id: &str, ext: &str) -> Result<PathBuf, AppError> {
    if !is_safe_path_component(fanart_id) {
        return Err(AppError::BadRequest("invalid fanart id".into()));
    }
    if !is_safe_path_component(ext) {
        return Err(AppError::BadRequest("invalid file extension".into()));
    }
    Ok(Path::new(cache_dir).join("base").join("fanart").join(format!("{fanart_id}.{ext}")))
}

/// Path for a preview image: `{cache_dir}/preview/{subdir}/{suffix}.{ext}`
pub fn preview_path(cache_dir: &str, image_type: ImageType, suffix: &str, ext: &str) -> Result<PathBuf, AppError> {
    if !is_safe_path_component(suffix) {
        return Err(AppError::BadRequest("invalid preview suffix".into()));
    }
    if !is_safe_path_component(ext) {
        return Err(AppError::BadRequest("invalid file extension".into()));
    }
    Ok(Path::new(cache_dir).join("preview").join(image_type.subdir()).join(format!("{suffix}.{ext}")))
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
    image_type: ImageType,
) -> Result<(), AppError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let model = poster_meta::ActiveModel {
        cache_key: Set(cache_key.to_string()),
        release_date: Set(release_date.map(|s| s.to_string())),
        image_type: Set(image_type.db_value().to_string()),
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
    fn typed_cache_path_poster() {
        let p = typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "tt1234567").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/posters/imdb/tt1234567.jpg"));
    }

    #[test]
    fn typed_cache_path_logo() {
        let p = typed_cache_path("/tmp/cache", ImageType::Logo, "imdb", "tt1234567").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/logos/imdb/tt1234567.png"));
    }

    #[test]
    fn typed_cache_path_backdrop() {
        let p = typed_cache_path("/tmp/cache", ImageType::Backdrop, "imdb", "tt1234567").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/backdrops/imdb/tt1234567.jpg"));
    }

    #[test]
    fn typed_cache_path_rejects_traversal() {
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "../../etc/passwd").is_err());
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "..").is_err());
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", ".").is_err());
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "").is_err());
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "foo/bar").is_err());
        assert!(typed_cache_path("/tmp/cache", ImageType::Poster, "imdb", "foo\\bar").is_err());
    }

    #[test]
    fn base_poster_path_strips_leading_slash() {
        let p = base_poster_path("/tmp/cache", "/abc123.jpg", "w500").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/base/posters/w500/abc123.jpg"));
    }

    #[test]
    fn base_poster_path_no_leading_slash() {
        let p = base_poster_path("/tmp/cache", "abc123.jpg", "w500").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/base/posters/w500/abc123.jpg"));
    }

    #[test]
    fn base_poster_path_original_size() {
        let p = base_poster_path("/tmp/cache", "/abc123.jpg", "original").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/base/posters/original/abc123.jpg"));
    }

    #[test]
    fn base_poster_path_rejects_traversal() {
        assert!(base_poster_path("/tmp/cache", "/../etc/passwd", "w500").is_err());
        assert!(base_poster_path("/tmp/cache", "..", "w500").is_err());
        assert!(base_poster_path("/tmp/cache", "", "w500").is_err());
    }

    #[test]
    fn base_poster_path_rejects_invalid_tmdb_size() {
        assert!(base_poster_path("/tmp/cache", "abc123.jpg", "..").is_err());
        assert!(base_poster_path("/tmp/cache", "abc123.jpg", "").is_err());
        assert!(base_poster_path("/tmp/cache", "abc123.jpg", "../etc").is_err());
    }

    #[test]
    fn base_fanart_path_valid() {
        let p = base_fanart_path("/tmp/cache", "12345", "png").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/base/fanart/12345.png"));
    }

    #[test]
    fn base_fanart_path_rejects_traversal() {
        assert!(base_fanart_path("/tmp/cache", "..", "png").is_err());
        assert!(base_fanart_path("/tmp/cache", "12345", "..").is_err());
        assert!(base_fanart_path("/tmp/cache", "", "png").is_err());
        assert!(base_fanart_path("/tmp/cache", "foo/bar", "png").is_err());
    }

    #[test]
    fn preview_path_valid() {
        let p = preview_path("/tmp/cache", ImageType::Poster, "r_imdb", "jpg").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/preview/posters/r_imdb.jpg"));
    }

    #[test]
    fn preview_path_logo() {
        let p = preview_path("/tmp/cache", ImageType::Logo, "r_imdb", "png").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/preview/logos/r_imdb.png"));
    }

    #[test]
    fn preview_path_backdrop() {
        let p = preview_path("/tmp/cache", ImageType::Backdrop, "r_imdb", "jpg").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cache/preview/backdrops/r_imdb.jpg"));
    }

    #[test]
    fn preview_path_rejects_traversal() {
        assert!(preview_path("/tmp/cache", ImageType::Poster, "..", "jpg").is_err());
        assert!(preview_path("/tmp/cache", ImageType::Poster, "", "jpg").is_err());
        assert!(preview_path("/tmp/cache", ImageType::Poster, "foo", "..").is_err());
    }

    #[test]
    fn image_type_subdir() {
        assert_eq!(ImageType::Poster.subdir(), "posters");
        assert_eq!(ImageType::Logo.subdir(), "logos");
        assert_eq!(ImageType::Backdrop.subdir(), "backdrops");
    }

    #[test]
    fn image_type_ext() {
        assert_eq!(ImageType::Poster.ext(), "jpg");
        assert_eq!(ImageType::Logo.ext(), "png");
        assert_eq!(ImageType::Backdrop.ext(), "jpg");
    }

    #[test]
    fn is_leap_year_cases() {
        assert!(is_leap(2000)); // divisible by 400
        assert!(is_leap(2024)); // divisible by 4, not by 100
        assert!(!is_leap(1900)); // divisible by 100, not by 400
        assert!(!is_leap(2023)); // not divisible by 4
    }
}
