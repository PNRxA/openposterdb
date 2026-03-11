use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_key_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub api_key_id: i32,
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
