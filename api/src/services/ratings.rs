use crate::id::{MediaType, ResolvedId};
use crate::services::mdblist::MdblistClient;
use crate::services::omdb::OmdbClient;
use crate::services::tmdb::TmdbClient;
use image::Rgba;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingSource {
    Imdb,
    Tmdb,
    Rt,
    RtAudience,
    Metacritic,
    Trakt,
    Letterboxd,
}

impl RatingSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Imdb => "IMDb",
            Self::Tmdb => "TMDB",
            Self::Rt => "RTC",
            Self::RtAudience => "RTA",
            Self::Metacritic => "MC",
            Self::Trakt => "Trakt",
            Self::Letterboxd => "LB",
        }
    }

    pub fn color(&self) -> Rgba<u8> {
        match self {
            Self::Imdb => Rgba([245, 197, 24, 255]),       // gold
            Self::Tmdb => Rgba([1, 210, 119, 255]),        // green
            Self::Rt => Rgba([250, 50, 10, 255]),          // red
            Self::RtAudience => Rgba([250, 50, 10, 255]),  // same RT red
            Self::Metacritic => Rgba([102, 204, 51, 255]), // metacritic green
            Self::Trakt => Rgba([237, 20, 61, 255]),       // trakt red
            Self::Letterboxd => Rgba([0, 210, 120, 255]),  // letterboxd green
        }
    }
}

#[derive(Debug, Clone)]
pub struct RatingBadge {
    pub source: RatingSource,
    pub value: String,
}

pub async fn fetch_ratings(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    omdb: &OmdbClient,
    mdblist: Option<&MdblistClient>,
) -> Vec<RatingBadge> {
    let tmdb_fut = fetch_tmdb_rating(resolved, tmdb);
    let omdb_fut = fetch_omdb_ratings(resolved.imdb_id.as_deref(), omdb);
    let mdblist_fut = fetch_mdblist_ratings(resolved, mdblist);

    let (tmdb_badges, omdb_badges, mdblist_badges) =
        tokio::join!(tmdb_fut, omdb_fut, mdblist_fut);

    // Collect which sources OMDb already provided
    let omdb_has = |src: RatingSource| -> bool {
        omdb_badges
            .as_ref()
            .is_some_and(|list| list.iter().any(|b| b.source == src))
    };
    let has_rt = omdb_has(RatingSource::Rt);
    let has_mc = omdb_has(RatingSource::Metacritic);

    let find_omdb = |src: RatingSource| -> Option<RatingBadge> {
        omdb_badges
            .as_ref()?
            .iter()
            .find(|b| b.source == src)
            .cloned()
    };
    let find_mdb = |src: RatingSource| -> Option<RatingBadge> {
        mdblist_badges
            .as_ref()?
            .iter()
            .find(|b| b.source == src)
            .cloned()
    };

    // Badge order: IMDb, TMDB, RT, RT Audience, MC, Trakt, Letterboxd
    let ordered: Vec<Option<RatingBadge>> = vec![
        find_omdb(RatingSource::Imdb),
        tmdb_badges,
        find_omdb(RatingSource::Rt).or_else(|| if !has_rt { find_mdb(RatingSource::Rt) } else { None }),
        find_mdb(RatingSource::RtAudience),
        find_omdb(RatingSource::Metacritic).or_else(|| if !has_mc { find_mdb(RatingSource::Metacritic) } else { None }),
        find_mdb(RatingSource::Trakt),
        find_mdb(RatingSource::Letterboxd),
    ];

    ordered.into_iter().flatten().collect()
}

async fn fetch_tmdb_rating(resolved: &ResolvedId, tmdb: &TmdbClient) -> Option<RatingBadge> {
    #[derive(Deserialize)]
    struct Details {
        vote_average: Option<f64>,
    }

    let path = match resolved.media_type {
        MediaType::Movie => format!("/movie/{}", resolved.tmdb_id),
        MediaType::Tv => format!("/tv/{}", resolved.tmdb_id),
    };

    let details: Details = tmdb.get(&path, &[]).await.ok()?;
    let score = details.vote_average?;
    if score <= 0.0 {
        return None;
    }

    Some(RatingBadge {
        source: RatingSource::Tmdb,
        value: format!("{:.0}%", score * 10.0),
    })
}

async fn fetch_omdb_ratings(imdb_id: Option<&str>, omdb: &OmdbClient) -> Option<Vec<RatingBadge>> {
    let imdb_id = imdb_id?;
    let resp = omdb.get_ratings(imdb_id).await.ok()?;
    let mut badges = Vec::new();

    // IMDb rating
    if let Some(ref rating) = resp.imdb_rating
        && rating != "N/A"
    {
        badges.push(RatingBadge {
            source: RatingSource::Imdb,
            value: rating.clone(),
        });
    }

    // Rotten Tomatoes from Ratings array
    for r in &resp.ratings {
        if r.source == "Rotten Tomatoes" && r.value != "N/A" {
            badges.push(RatingBadge {
                source: RatingSource::Rt,
                value: r.value.clone(),
            });
        }
    }

    // Metacritic
    if let Some(ref mc) = resp.metascore
        && mc != "N/A"
    {
        badges.push(RatingBadge {
            source: RatingSource::Metacritic,
            value: mc.clone(),
        });
    }

    Some(badges)
}

async fn fetch_mdblist_ratings(
    resolved: &ResolvedId,
    mdblist: Option<&MdblistClient>,
) -> Option<Vec<RatingBadge>> {
    let client = mdblist?;
    let imdb_id = resolved.imdb_id.as_deref()?;

    let resp = client
        .get_ratings(imdb_id, &resolved.media_type)
        .await
        .ok()?;

    let mut badges = Vec::new();

    for r in &resp.ratings {
        let badge = match r.source.as_str() {
            "trakt" => r.score.map(|s| RatingBadge {
                source: RatingSource::Trakt,
                value: format!("{s}%"),
            }),
            "letterboxd" => r.value.map(|v| RatingBadge {
                source: RatingSource::Letterboxd,
                value: format!("{v:.1}"),
            }),
            "popcorn" => r.score.map(|s| RatingBadge {
                source: RatingSource::RtAudience,
                value: format!("{s}%"),
            }),
            "tomatoes" => r.score.map(|s| RatingBadge {
                source: RatingSource::Rt,
                value: format!("{s}%"),
            }),
            "metacritic" => r.score.map(|s| RatingBadge {
                source: RatingSource::Metacritic,
                value: s.to_string(),
            }),
            _ => None,
        };

        if let Some(b) = badge {
            badges.push(b);
        }
    }

    Some(badges)
}
