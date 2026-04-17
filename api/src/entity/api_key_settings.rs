use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
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
    pub poster_badge_size: String,
    pub poster_badge_scale_override: f32,
    pub logo_badge_size: String,
    pub logo_badge_scale_override: f32,
    pub backdrop_badge_size: String,
    pub backdrop_badge_scale_override: f32,
    pub backdrop_position: String,
    pub backdrop_badge_direction: String,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: String,
    pub episode_label_style: String,
    pub episode_badge_size: String,
    pub episode_badge_scale_override: f32,
    pub episode_position: String,
    pub episode_badge_direction: String,
    pub episode_blur: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
