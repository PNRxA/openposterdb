use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_key_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub api_key_id: i32,
    pub image_source: String,
    pub lang: String,
    pub textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub poster_position: String,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: String,
    pub logo_badge_style: String,
    pub backdrop_badge_style: String,
    pub poster_label_style: String,
    pub logo_label_style: String,
    pub backdrop_label_style: String,
    pub poster_badge_direction: String,
    pub poster_badge_split: bool,
    pub poster_fit: String,
    pub poster_badge_size: String,
    pub logo_badge_size: String,
    pub backdrop_badge_size: String,
    pub backdrop_position: String,
    pub backdrop_badge_direction: String,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: String,
    pub episode_label_style: String,
    pub episode_badge_size: String,
    pub episode_position: String,
    pub episode_badge_direction: String,
    pub episode_blur: bool,
    pub ratings_exclude: String,
    pub poster_badge_shape: String,
    pub logo_badge_shape: String,
    pub backdrop_badge_shape: String,
    pub episode_badge_shape: String,
    pub poster_badge_background: String,
    pub logo_badge_background: String,
    pub backdrop_badge_background: String,
    pub episode_badge_background: String,
    /// Distance (percent of width, 0–50) to inset backdrop ratings from the
    /// anchored horizontal edge (left or right). Ignored for centered positions.
    pub backdrop_edge_inset_x: i32,
    /// Distance (percent of height, 0–50) to inset backdrop ratings from the
    /// anchored vertical edge (top or bottom). Ignored for centered positions.
    pub backdrop_edge_inset_y: i32,
    /// How the quality overlay badge renders: `text` or `logo`.
    pub quality_style: String,
    /// Whether/how the main-language overlay badge renders on posters: `off`, `flag`, `text`.
    pub poster_lang_icon: String,
    /// Main-language overlay badge on logos: `off`, `flag`, `text`.
    pub logo_lang_icon: String,
    /// Main-language overlay badge on backdrops: `off`, `flag`, `text`.
    pub backdrop_lang_icon: String,
    /// Comma-separated languages to exclude from the language badge (e.g. `en`).
    pub lang_exclude: String,
    /// Poster anchor for the quality overlay badge (e.g. `tr`).
    pub poster_quality_position: String,
    /// Backdrop anchor for the quality overlay badge (e.g. `tl`).
    pub backdrop_quality_position: String,
    /// Layout direction for stacked quality badges (`d` auto, `h`, `v`).
    pub quality_direction: String,
    /// Poster anchor for the main-language overlay badge (e.g. `tl`).
    pub poster_lang_position: String,
    /// Backdrop anchor for the main-language overlay badge (e.g. `bl`).
    pub backdrop_lang_position: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
