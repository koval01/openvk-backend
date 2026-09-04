use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "likes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub origin: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub object_id: i64,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
