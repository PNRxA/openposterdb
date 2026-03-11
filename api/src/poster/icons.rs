use image::RgbaImage;
use std::sync::LazyLock;

use crate::services::ratings::RatingSource;

static IMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/imdb.png");
static TMDB_BYTES: &[u8] = include_bytes!("../../assets/icons/tmdb.png");
static RT_BYTES: &[u8] = include_bytes!("../../assets/icons/rt.png");
static RTA_BYTES: &[u8] = include_bytes!("../../assets/icons/rta.png");
static MC_BYTES: &[u8] = include_bytes!("../../assets/icons/mc.png");
static TRAKT_BYTES: &[u8] = include_bytes!("../../assets/icons/trakt.png");
static LB_BYTES: &[u8] = include_bytes!("../../assets/icons/lb.png");
static MAL_BYTES: &[u8] = include_bytes!("../../assets/icons/mal.png");

fn decode(bytes: &[u8]) -> RgbaImage {
    image::load_from_memory(bytes)
        .expect("embedded icon PNG should be valid")
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
    }
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
        ];
        for source in sources {
            let img = icon_for_source(&source);
            assert_eq!(img.width(), 48, "wrong width for {:?}", source);
            assert_eq!(img.height(), 48, "wrong height for {:?}", source);
        }
    }
}
