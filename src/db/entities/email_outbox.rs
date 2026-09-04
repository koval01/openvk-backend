use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "email_outbox")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub to_email: String,
    pub template: String,
    pub payload: Json,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub scheduled_at: DateTimeUtc,
    pub sent_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
