use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_key_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub api_key_id: i32,
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
