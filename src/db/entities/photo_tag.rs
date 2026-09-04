use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "photo_tags")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub media_id: i64,
    pub tagged_user_id: i64,
    pub tagged_by_user_id: i64,
    pub x_percent: f32,
    pub y_percent: f32,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
