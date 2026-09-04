use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audios")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub media_id: i64,
    pub owner_user_id: i64,
    pub artist: String,
    pub title: String,
    pub duration_ms: i32,
    pub genre: Option<String>,
    pub lyrics: Option<String>,
    pub listens: i64,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
