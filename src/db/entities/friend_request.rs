use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "friend_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub from_user_id: i64,
    pub to_user_id: i64,
    pub status: String,
    pub created_at: DateTimeUtc,
    pub responded_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
