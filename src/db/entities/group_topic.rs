use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "group_topics")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub group_id: i64,
    pub author_id: i64,
    pub title: String,
    pub closed: bool,
    pub pinned: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
