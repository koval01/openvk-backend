use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "playlist_tracks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub playlist_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub audio_id: i64,
    pub position: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
