use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "conversation_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub conversation_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub role: String,
    pub last_read_message_id: Option<i64>,
    pub typing_at: Option<DateTimeUtc>,
    pub joined_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
