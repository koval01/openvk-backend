use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub hometown: Option<String>,
    pub birthday: Option<String>,
    pub sex: Option<String>,
    pub relationship: Option<String>,
    pub university: Option<String>,
    pub school: Option<String>,
    pub activities: Option<String>,
    pub interests: Option<String>,
    pub favorite_music: Option<String>,
    pub favorite_films: Option<String>,
    pub favorite_books: Option<String>,
    pub favorite_quotes: Option<String>,
    pub about: Option<String>,
    pub website: Option<String>,
    pub address: Option<String>,
    pub contact_email: Option<String>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
