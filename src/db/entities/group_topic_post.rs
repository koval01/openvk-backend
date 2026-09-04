use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "group_topic_posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub topic_id: i64,
    pub author_id: i64,
    pub content: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
