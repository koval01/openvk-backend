use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "follows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub follower_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_user_id: i64,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
