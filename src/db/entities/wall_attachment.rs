use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "wall_attachments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub post_id: i64,
    pub sort: i32,
    pub kind: String,
    pub owner_id: i64,
    pub object_id: i64,
    pub url: String,
    pub title: String,
    pub src: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
