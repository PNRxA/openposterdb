use std::env;

#[derive(Clone)]
pub struct Config {
    pub tmdb_api_key: String,
    pub omdb_api_key: Option<String>,
    pub cache_dir: String,
    pub db_dir: String,
    pub listen_addr: String,
    pub ratings_min_stale_secs: u64,
    pub ratings_max_age_secs: u64,
    pub image_stale_secs: u64,
    pub image_quality: u8,
    pub mdblist_api_key: Option<String>,
    pub image_mem_cache_mb: u64,
    pub static_dir: Option<String>,
    pub cors_origin: Option<String>,
    pub fanart_api_key: Option<String>,
    pub trakt_client_id: Option<String>,
    pub enable_cdn_redirects: bool,
    pub external_cache_only: bool,
    pub free_key_enabled: Option<bool>,
    pub disable_public_pages: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("tmdb_api_key", &"[REDACTED]")
            .field("omdb_api_key", &self.omdb_api_key.as_ref().map(|_| "[REDACTED]"))
            .field("cache_dir", &self.cache_dir)
            .field("db_dir", &self.db_dir)
            .field("listen_addr", &self.listen_addr)
            .field("ratings_min_stale_secs", &self.ratings_min_stale_secs)
            .field("ratings_max_age_secs", &self.ratings_max_age_secs)
            .field("image_stale_secs", &self.image_stale_secs)
            .field("image_quality", &self.image_quality)
            .field("mdblist_api_key", &self.mdblist_api_key.as_ref().map(|_| "[REDACTED]"))
            .field("image_mem_cache_mb", &self.image_mem_cache_mb)
            .field("static_dir", &self.static_dir)
            .field("cors_origin", &self.cors_origin)
            .field("fanart_api_key", &self.fanart_api_key.as_ref().map(|_| "[REDACTED]"))
            .field("trakt_client_id", &self.trakt_client_id.as_ref().map(|_| "[REDACTED]"))
            .field("enable_cdn_redirects", &self.enable_cdn_redirects)
            .field("external_cache_only", &self.external_cache_only)
            .field("free_key_enabled", &self.free_key_enabled)
            .field("disable_public_pages", &self.disable_public_pages)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Self {
        let config = Self {
            tmdb_api_key: env::var("TMDB_API_KEY").expect("TMDB_API_KEY must be set"),
            omdb_api_key: env::var("OMDB_API_KEY").ok(),
            cache_dir: env::var("CACHE_DIR").unwrap_or_else(|_| "./cache".into()),
            db_dir: env::var("DB_DIR").unwrap_or_else(|_| "./db".into()),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            ratings_min_stale_secs: env::var("RATINGS_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(86400),
            ratings_max_age_secs: env::var("RATINGS_MAX_AGE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(31_536_000),
            image_stale_secs: env::var("IMAGE_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            image_quality: env::var("IMAGE_QUALITY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(85),
            mdblist_api_key: env::var("MDBLIST_API_KEY").ok(),
            image_mem_cache_mb: env::var("IMAGE_MEM_CACHE_MB")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512),
            static_dir: env::var("STATIC_DIR").ok(),
            cors_origin: env::var("CORS_ORIGIN").ok(),
            fanart_api_key: env::var("FANART_API_KEY").ok(),
            trakt_client_id: env::var("TRAKT_CLIENT_ID").ok(),
            enable_cdn_redirects: env::var("ENABLE_CDN_REDIRECTS")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            external_cache_only: env::var("EXTERNAL_CACHE_ONLY")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            free_key_enabled: env::var("FREE_KEY_ENABLED")
                .ok()
                .map(|v| v == "true" || v == "1"),
            disable_public_pages: env::var("DISABLE_PUBLIC_PAGES")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        };

        if config.omdb_api_key.is_none()
            && config.mdblist_api_key.is_none()
            && config.trakt_client_id.is_none()
        {
            panic!(
                "at least one of OMDB_API_KEY, MDBLIST_API_KEY, or TRAKT_CLIENT_ID must be set"
            );
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    /// Helper: remove all env vars that Config::from_env reads.
    ///
    /// # Safety
    /// Tests using this must run serially (via `#[serial]`) to avoid races.
    unsafe fn clear_config_env() {
        for key in [
            "TMDB_API_KEY",
            "OMDB_API_KEY",
            "MDBLIST_API_KEY",
            "CACHE_DIR",
            "DB_DIR",
            "LISTEN_ADDR",
            "RATINGS_STALE_SECS",
            "RATINGS_MAX_AGE_SECS",
            "IMAGE_STALE_SECS",
            "IMAGE_QUALITY",
            "IMAGE_MEM_CACHE_MB",
            "STATIC_DIR",
            "CORS_ORIGIN",
            "FANART_API_KEY",
            "TRAKT_CLIENT_ID",
            "ENABLE_CDN_REDIRECTS",
            "EXTERNAL_CACHE_ONLY",
            "FREE_KEY_ENABLED",
            "DISABLE_PUBLIC_PAGES",
        ] {
            unsafe { env::remove_var(key) };
        }
    }

    #[test]
    #[serial]
    fn test_valid_config_with_omdb() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };

        let cfg = Config::from_env();
        assert_eq!(cfg.tmdb_api_key, "tmdb_test");
        assert_eq!(cfg.omdb_api_key.as_deref(), Some("omdb_test"));
        assert!(cfg.mdblist_api_key.is_none());
        assert_eq!(cfg.cache_dir, "./cache");
        assert_eq!(cfg.db_dir, "./db");
        assert_eq!(cfg.listen_addr, "0.0.0.0:3000");
        assert_eq!(cfg.ratings_min_stale_secs, 86400);
        assert_eq!(cfg.ratings_max_age_secs, 31_536_000);
        assert_eq!(cfg.image_stale_secs, 0);
        assert_eq!(cfg.image_quality, 85);
        assert_eq!(cfg.image_mem_cache_mb, 512);
        assert!(!cfg.enable_cdn_redirects);
        assert!(!cfg.external_cache_only);
        assert!(cfg.free_key_enabled.is_none());
    }

    #[test]
    #[serial]
    fn test_valid_config_with_mdblist() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("MDBLIST_API_KEY", "mdblist_test") };

        let cfg = Config::from_env();
        assert!(cfg.omdb_api_key.is_none());
        assert_eq!(cfg.mdblist_api_key.as_deref(), Some("mdblist_test"));
    }

    #[test]
    #[serial]
    fn test_custom_numeric_overrides() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };
        unsafe { env::set_var("RATINGS_STALE_SECS", "100") };
        unsafe { env::set_var("RATINGS_MAX_AGE_SECS", "200") };
        unsafe { env::set_var("IMAGE_STALE_SECS", "300") };
        unsafe { env::set_var("IMAGE_QUALITY", "50") };
        unsafe { env::set_var("IMAGE_MEM_CACHE_MB", "1024") };

        let cfg = Config::from_env();
        assert_eq!(cfg.ratings_min_stale_secs, 100);
        assert_eq!(cfg.ratings_max_age_secs, 200);
        assert_eq!(cfg.image_stale_secs, 300);
        assert_eq!(cfg.image_quality, 50);
        assert_eq!(cfg.image_mem_cache_mb, 1024);
    }

    #[test]
    #[serial]
    fn test_invalid_numeric_falls_back_to_default() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };
        unsafe { env::set_var("IMAGE_QUALITY", "notanumber") };
        unsafe { env::set_var("RATINGS_STALE_SECS", "abc") };

        let cfg = Config::from_env();
        assert_eq!(cfg.image_quality, 85);
        assert_eq!(cfg.ratings_min_stale_secs, 86400);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "TMDB_API_KEY must be set")]
    fn test_panics_without_tmdb_key() {
        unsafe { clear_config_env() };
        Config::from_env();
    }

    #[test]
    #[serial]
    #[should_panic(
        expected = "at least one of OMDB_API_KEY, MDBLIST_API_KEY, or TRAKT_CLIENT_ID must be set"
    )]
    fn test_panics_without_ratings_provider() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        Config::from_env();
    }

    #[test]
    #[serial]
    fn test_boolean_env_true() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };
        unsafe { env::set_var("ENABLE_CDN_REDIRECTS", "true") };
        unsafe { env::set_var("EXTERNAL_CACHE_ONLY", "1") };
        unsafe { env::set_var("DISABLE_PUBLIC_PAGES", "true") };

        let cfg = Config::from_env();
        assert!(cfg.enable_cdn_redirects);
        assert!(cfg.external_cache_only);
        assert!(cfg.disable_public_pages);
    }

    #[test]
    #[serial]
    fn test_boolean_env_defaults_false() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };

        let cfg = Config::from_env();
        assert!(!cfg.enable_cdn_redirects);
        assert!(!cfg.external_cache_only);
        assert!(!cfg.disable_public_pages);
    }

    #[test]
    #[serial]
    fn test_free_key_enabled_some() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };
        unsafe { env::set_var("FREE_KEY_ENABLED", "true") };

        let cfg = Config::from_env();
        assert_eq!(cfg.free_key_enabled, Some(true));
    }

    #[test]
    #[serial]
    fn test_free_key_enabled_none() {
        unsafe { clear_config_env() };
        unsafe { env::set_var("TMDB_API_KEY", "tmdb_test") };
        unsafe { env::set_var("OMDB_API_KEY", "omdb_test") };

        let cfg = Config::from_env();
        assert!(cfg.free_key_enabled.is_none());
    }
}
