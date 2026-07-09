use image::RgbaImage;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::services::db::QualityTier;
use crate::services::ratings::{RatingBadge, RatingSource};

static IMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/imdb.png");
static TMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/tmdb.png");
static RT_BYTES: &[u8] = include_bytes!("../../assets/icons/rt.png");
static RTA_BYTES: &[u8] = include_bytes!("../../assets/icons/rta.png");
static MC_BYTES: &[u8] = include_bytes!("../../assets/icons/mc.png");
static TRAKT_BYTES: &[u8] = include_bytes!("../../assets/icons/trakt.png");
static LB_BYTES: &[u8] = include_bytes!("../../assets/icons/lb.png");
static MAL_BYTES: &[u8] = include_bytes!("../../assets/icons/mal.png");
static MDBLIST_BYTES: &[u8] = include_bytes!("../../assets/icons/mdblist.png");
static EBERT_BYTES: &[u8] = include_bytes!("../../assets/icons/ebert.png");

// Official icons
static OFF_IMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/official/imdb.png");
static OFF_TMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/official/tmdb.png");
static OFF_MC_BYTES: &[u8] = include_bytes!("../../assets/icons/official/metacritic.png");
static OFF_TRAKT_BYTES: &[u8] = include_bytes!("../../assets/icons/official/trakt.png");
static OFF_LB_BYTES: &[u8] = include_bytes!("../../assets/icons/official/letterboxd.png");
static OFF_MAL_BYTES: &[u8] = include_bytes!("../../assets/icons/official/mal.webp");
static OFF_MDBLIST_BYTES: &[u8] = include_bytes!("../../assets/icons/official/mdblist.png");
static OFF_EBERT_BYTES: &[u8] = include_bytes!("../../assets/icons/official/ebert.png");
static OFF_RT_CRITIC_POSITIVE_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_critic_positive.png");
static OFF_RT_CRITIC_ROTTEN_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_critic_rotten.png");
static OFF_RT_CRITIC_CERTIFIED_FRESH_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_critic_certified_fresh.png");
static OFF_RT_AUDIENCE_POSITIVE_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_positive_audience.png");
static OFF_RT_AUDIENCE_NEGATIVE_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_negative_audience.png");
static OFF_RT_AUDIENCE_VERIFIED_HOT_BYTES: &[u8] = include_bytes!("../../assets/icons/official/Rotten_Tomatoes_verified_hot_audience.png");

fn decode(bytes: &[u8]) -> RgbaImage {
    image::load_from_memory(bytes)
        .expect("embedded icon should be valid")
        .to_rgba8()
}

static IMDB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(IMDB_BYTES));
static TMDB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(TMDB_BYTES));
static RT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(RT_BYTES));
static RTA_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(RTA_BYTES));
static MC_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(MC_BYTES));
static TRAKT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(TRAKT_BYTES));
static LB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(LB_BYTES));
static MAL_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(MAL_BYTES));
static MDBLIST_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(MDBLIST_BYTES));
static EBERT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(EBERT_BYTES));

// Official icon images
static OFF_IMDB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_IMDB_BYTES));
static OFF_TMDB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_TMDB_BYTES));
static OFF_MC_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_MC_BYTES));
static OFF_TRAKT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_TRAKT_BYTES));
static OFF_LB_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_LB_BYTES));
static OFF_MAL_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_MAL_BYTES));
static OFF_MDBLIST_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_MDBLIST_BYTES));
static OFF_EBERT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_EBERT_BYTES));
static OFF_RT_CRITIC_POSITIVE_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_CRITIC_POSITIVE_BYTES));
static OFF_RT_CRITIC_ROTTEN_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_CRITIC_ROTTEN_BYTES));
static OFF_RT_CRITIC_CERTIFIED_FRESH_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_CRITIC_CERTIFIED_FRESH_BYTES));
static OFF_RT_AUDIENCE_POSITIVE_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_AUDIENCE_POSITIVE_BYTES));
static OFF_RT_AUDIENCE_NEGATIVE_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_AUDIENCE_NEGATIVE_BYTES));
static OFF_RT_AUDIENCE_VERIFIED_HOT_IMG: LazyLock<RgbaImage> = LazyLock::new(|| decode(OFF_RT_AUDIENCE_VERIFIED_HOT_BYTES));

pub fn icon_for_source(source: &RatingSource) -> &'static RgbaImage {
    match source {
        RatingSource::Imdb => &IMDB_IMG,
        RatingSource::Tmdb => &TMDB_IMG,
        RatingSource::Rt => &RT_IMG,
        RatingSource::RtAudience => &RTA_IMG,
        RatingSource::Metacritic => &MC_IMG,
        RatingSource::Trakt => &TRAKT_IMG,
        RatingSource::Letterboxd => &LB_IMG,
        RatingSource::Mal => &MAL_IMG,
        RatingSource::Mdblist => &MDBLIST_IMG,
        RatingSource::Ebert => &EBERT_IMG,
    }
}

fn parse_percent(value: &str) -> Option<u32> {
    let s = value.trim_end_matches('%');
    // Try integer first, then float (e.g. "8.0" → 8)
    s.parse::<u32>()
        .ok()
        .or_else(|| s.parse::<f32>().ok().map(|f| f as u32))
}

pub fn official_icon_for_badge(badge: &RatingBadge) -> &'static RgbaImage {
    match badge.source {
        RatingSource::Imdb => &OFF_IMDB_IMG,
        RatingSource::Tmdb => &OFF_TMDB_IMG,
        RatingSource::Metacritic => &OFF_MC_IMG,
        RatingSource::Trakt => &OFF_TRAKT_IMG,
        RatingSource::Letterboxd => &OFF_LB_IMG,
        RatingSource::Mal => &OFF_MAL_IMG,
        RatingSource::Mdblist => &OFF_MDBLIST_IMG,
        RatingSource::Ebert => &OFF_EBERT_IMG,
        RatingSource::Rt => {
            let score = parse_percent(&badge.value).unwrap_or(0);
            if score >= 75 {
                &OFF_RT_CRITIC_CERTIFIED_FRESH_IMG
            } else if score >= 60 {
                &OFF_RT_CRITIC_POSITIVE_IMG
            } else {
                &OFF_RT_CRITIC_ROTTEN_IMG
            }
        }
        RatingSource::RtAudience => {
            let score = parse_percent(&badge.value).unwrap_or(0);
            if score >= 75 {
                &OFF_RT_AUDIENCE_VERIFIED_HOT_IMG
            } else if score >= 60 {
                &OFF_RT_AUDIENCE_POSITIVE_IMG
            } else {
                &OFF_RT_AUDIENCE_NEGATIVE_IMG
            }
        }
    }
}

// --- Quality overlay logos (issue #1) ---

/// Embedded quality-tier logo bytes, keyed by `QualityTier::as_str()`.
static QUALITY_LOGO_BYTES: &[(&str, &[u8])] = &[
    ("4k", include_bytes!("../../assets/icons/quality/4k.png")),
    ("1080p", include_bytes!("../../assets/icons/quality/1080p.png")),
    ("720p", include_bytes!("../../assets/icons/quality/720p.png")),
    ("hdr", include_bytes!("../../assets/icons/quality/hdr.png")),
    ("dv", include_bytes!("../../assets/icons/quality/dv.png")),
];

static QUALITY_LOGOS: LazyLock<HashMap<&'static str, RgbaImage>> =
    LazyLock::new(|| QUALITY_LOGO_BYTES.iter().map(|(k, b)| (*k, decode(b))).collect());

/// Logo image for a quality tier, or `None` if no logo asset is bundled (the
/// caller then falls back to a text badge).
pub fn quality_logo_for(tier: QualityTier) -> Option<&'static RgbaImage> {
    QUALITY_LOGOS.get(tier.as_str())
}

// --- Language flags (issue #6) ---

/// Embedded flag bytes, keyed by ISO 3166-1 alpha-2 country code. Fetched by
/// `scripts/fetch-flags.sh`.
macro_rules! flag_bytes {
    ($cc:literal) => {
        ($cc, include_bytes!(concat!("../../assets/icons/flags/", $cc, ".png")) as &[u8])
    };
}

static FLAG_BYTES: &[(&str, &[u8])] = &[
    flag_bytes!("us"), flag_bytes!("gb"), flag_bytes!("jp"), flag_bytes!("kr"),
    flag_bytes!("cn"), flag_bytes!("fr"), flag_bytes!("de"), flag_bytes!("es"),
    flag_bytes!("it"), flag_bytes!("pt"), flag_bytes!("br"), flag_bytes!("ru"),
    flag_bytes!("in"), flag_bytes!("nl"), flag_bytes!("se"), flag_bytes!("dk"),
    flag_bytes!("no"), flag_bytes!("fi"), flag_bytes!("pl"), flag_bytes!("tr"),
    flag_bytes!("th"), flag_bytes!("id"), flag_bytes!("cz"), flag_bytes!("gr"),
    flag_bytes!("il"), flag_bytes!("hu"), flag_bytes!("ro"), flag_bytes!("ua"),
    flag_bytes!("vn"), flag_bytes!("ir"), flag_bytes!("my"), flag_bytes!("ph"),
    flag_bytes!("bd"), flag_bytes!("sa"), flag_bytes!("is"), flag_bytes!("ee"),
    flag_bytes!("lv"), flag_bytes!("lt"), flag_bytes!("sk"), flag_bytes!("si"),
    flag_bytes!("hr"), flag_bytes!("rs"), flag_bytes!("bg"),
];

static FLAGS: LazyLock<HashMap<&'static str, RgbaImage>> =
    LazyLock::new(|| FLAG_BYTES.iter().map(|(k, b)| (*k, decode(b))).collect());

/// Map a title's main language (ISO 639-1, region-stripped) to a representative
/// country whose flag we bundle. TMDB `original_language` is a *language* code,
/// not a country, so this picks the most common representative flag (e.g.
/// `en`→UK, `pt`→Portugal). Returns `None` for unmapped languages, in which
/// case the caller falls back to a text badge.
pub fn flag_country_for_lang(code: &str) -> Option<&'static str> {
    // Canonicalize ("pt-BR"→"pt", TMDB alias "cn"→"zh", drop "no language"
    // sentinels) so the flag lookup matches the exclude/cache identity.
    let base = crate::services::db::canonical_lang(code)?;
    let cc = match base.as_str() {
        "en" => "gb",
        "ja" => "jp",
        "ko" => "kr",
        "zh" => "cn",
        "fr" => "fr",
        "de" => "de",
        "es" | "ca" | "eu" | "gl" => "es",
        "it" => "it",
        "pt" => "pt",
        "ru" => "ru",
        "hi" | "ta" | "te" | "ml" | "kn" | "mr" | "pa" => "in",
        "nl" => "nl",
        "sv" => "se",
        "da" => "dk",
        "no" | "nb" | "nn" => "no",
        "fi" => "fi",
        "pl" => "pl",
        "tr" => "tr",
        "th" => "th",
        "id" => "id",
        "cs" => "cz",
        "el" => "gr",
        "he" => "il",
        "hu" => "hu",
        "ro" => "ro",
        "uk" => "ua",
        "vi" => "vn",
        "fa" => "ir",
        "ms" => "my",
        "tl" => "ph",
        "bn" => "bd",
        "ar" => "sa",
        "is" => "is",
        "et" => "ee",
        "lv" => "lv",
        "lt" => "lt",
        "sk" => "sk",
        "sl" => "si",
        "hr" => "hr",
        "sr" => "rs",
        "bg" => "bg",
        _ => return None,
    };
    Some(cc)
}

/// Flag image for a title's main language, or `None` if the language has no
/// mapped flag (the caller then falls back to a text badge).
pub fn flag_for_lang(code: &str) -> Option<&'static RgbaImage> {
    flag_country_for_lang(code).and_then(|cc| FLAGS.get(cc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_decode_to_48x48() {
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];
        for source in sources {
            let img = icon_for_source(&source);
            assert_eq!(img.width(), 48, "wrong width for {:?}", source);
            assert_eq!(img.height(), 48, "wrong height for {:?}", source);
        }
    }

    #[test]
    fn all_official_icons_fit_48x48() {
        let badges = [
            RatingBadge { source: RatingSource::Imdb, value: "8.5".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".into() },
            RatingBadge { source: RatingSource::Metacritic, value: "80".into() },
            RatingBadge { source: RatingSource::Trakt, value: "90%".into() },
            RatingBadge { source: RatingSource::Letterboxd, value: "4.0".into() },
            RatingBadge { source: RatingSource::Mal, value: "8.5".into() },
            RatingBadge { source: RatingSource::Mdblist, value: "89".into() },
            RatingBadge { source: RatingSource::Ebert, value: "3.5".into() },
            // RT critics — all three variants
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Rt, value: "65%".into() },
            RatingBadge { source: RatingSource::Rt, value: "40%".into() },
            // RT audience — all three variants
            RatingBadge { source: RatingSource::RtAudience, value: "90%".into() },
            RatingBadge { source: RatingSource::RtAudience, value: "65%".into() },
            RatingBadge { source: RatingSource::RtAudience, value: "40%".into() },
        ];
        for badge in &badges {
            let img = official_icon_for_badge(badge);
            assert!(img.width() <= 48, "width {} > 48 for {:?} with value {}", img.width(), badge.source, badge.value);
            assert!(img.height() <= 48, "height {} > 48 for {:?} with value {}", img.height(), badge.source, badge.value);
            assert!(img.width() == 48 || img.height() == 48, "neither dimension is 48 for {:?} with value {}", badge.source, badge.value);
        }
    }

    #[test]
    fn parse_percent_works() {
        assert_eq!(parse_percent("95%"), Some(95));
        assert_eq!(parse_percent("8.0"), Some(8));
        assert_eq!(parse_percent("72"), Some(72));
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_percent("N/A"), None);
    }

    #[test]
    fn rt_critic_icon_thresholds() {
        let cf = RatingBadge { source: RatingSource::Rt, value: "75%".into() };
        let fresh = RatingBadge { source: RatingSource::Rt, value: "60%".into() };
        let rotten = RatingBadge { source: RatingSource::Rt, value: "59%".into() };

        assert!(std::ptr::eq(official_icon_for_badge(&cf), &*OFF_RT_CRITIC_CERTIFIED_FRESH_IMG));
        assert!(std::ptr::eq(official_icon_for_badge(&fresh), &*OFF_RT_CRITIC_POSITIVE_IMG));
        assert!(std::ptr::eq(official_icon_for_badge(&rotten), &*OFF_RT_CRITIC_ROTTEN_IMG));
    }

    #[test]
    fn rt_audience_icon_thresholds() {
        let hot = RatingBadge { source: RatingSource::RtAudience, value: "75%".into() };
        let pos = RatingBadge { source: RatingSource::RtAudience, value: "60%".into() };
        let neg = RatingBadge { source: RatingSource::RtAudience, value: "59%".into() };

        assert!(std::ptr::eq(official_icon_for_badge(&hot), &*OFF_RT_AUDIENCE_VERIFIED_HOT_IMG));
        assert!(std::ptr::eq(official_icon_for_badge(&pos), &*OFF_RT_AUDIENCE_POSITIVE_IMG));
        assert!(std::ptr::eq(official_icon_for_badge(&neg), &*OFF_RT_AUDIENCE_NEGATIVE_IMG));
    }

    #[test]
    fn every_quality_tier_has_a_logo() {
        for tier in [
            QualityTier::Uhd4k,
            QualityTier::P1080,
            QualityTier::P720,
            QualityTier::Hdr,
            QualityTier::Dv,
        ] {
            let img = quality_logo_for(tier).unwrap_or_else(|| panic!("missing logo for {tier:?}"));
            assert!(img.width() > 0 && img.height() > 0, "empty logo for {tier:?}");
        }
    }

    #[test]
    fn all_bundled_flags_decode() {
        for (cc, _) in FLAG_BYTES {
            let img = FLAGS.get(cc).unwrap_or_else(|| panic!("missing flag {cc}"));
            assert!(img.width() > 0 && img.height() > 0, "empty flag {cc}");
        }
    }

    #[test]
    fn flag_for_lang_maps_and_falls_back() {
        // Known languages resolve to a bundled flag.
        assert_eq!(flag_country_for_lang("en"), Some("gb")); // English → Union Jack
        assert!(flag_for_lang("en").is_some());
        assert!(flag_for_lang("ja").is_some());
        // Region suffixes are stripped before mapping.
        assert!(std::ptr::eq(flag_for_lang("pt-BR").unwrap(), flag_for_lang("pt").unwrap()));
        // TMDB alias `cn` canonicalizes to `zh` → the China flag.
        assert_eq!(flag_country_for_lang("cn"), Some("cn"));
        assert!(std::ptr::eq(flag_for_lang("cn").unwrap(), flag_for_lang("zh").unwrap()));
        // "No language" sentinel → None (caller falls back to text/omits).
        assert!(flag_country_for_lang("xx").is_none());
        assert!(flag_for_lang("xx").is_none());
        // Unmapped language → None (caller falls back to text).
        assert!(flag_country_for_lang("zz").is_none());
    }
}
