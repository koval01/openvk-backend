use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "wall_posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub target_id: i64,
    pub local_id: i64,
    pub author_id: i64,
    pub group_id: Option<i64>,
    pub content: String,
    pub created_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
