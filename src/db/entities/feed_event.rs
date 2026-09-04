use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "feed_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub actor_id: i64,
    pub verb: String,
    pub object_type: String,
    pub object_id: i64,
    pub group_id: Option<i64>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
