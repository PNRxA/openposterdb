use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub tmdb_api_key: String,
    pub omdb_api_key: Option<String>,
    pub cache_dir: String,
    pub listen_addr: String,
    pub ratings_min_stale_secs: u64,
    pub ratings_max_age_secs: u64,
    pub poster_stale_secs: u64,
    pub poster_quality: u8,
    pub mdblist_api_key: Option<String>,
    pub poster_mem_cache_mb: u64,
}

impl Config {
    pub fn from_env() -> Self {
        let config = Self {
            tmdb_api_key: env::var("TMDB_API_KEY").expect("TMDB_API_KEY must be set"),
            omdb_api_key: env::var("OMDB_API_KEY").ok(),
            cache_dir: env::var("CACHE_DIR").unwrap_or_else(|_| "./cache".into()),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            ratings_min_stale_secs: env::var("RATINGS_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(86400),
            ratings_max_age_secs: env::var("RATINGS_MAX_AGE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(31_536_000),
            poster_stale_secs: env::var("POSTER_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            poster_quality: env::var("POSTER_QUALITY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(85),
            mdblist_api_key: env::var("MDBLIST_API_KEY").ok(),
            poster_mem_cache_mb: env::var("POSTER_MEM_CACHE_MB")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512),
        };

        if config.omdb_api_key.is_none() && config.mdblist_api_key.is_none() {
            panic!("at least one of OMDB_API_KEY or MDBLIST_API_KEY must be set");
        }

        config
    }
}
