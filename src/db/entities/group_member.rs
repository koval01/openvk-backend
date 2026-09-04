use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "group_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub role: String,
    pub status: String,
    pub joined_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
