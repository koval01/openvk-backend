use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "album_photos")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub album_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub media_id: i64,
    pub position: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
