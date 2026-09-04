use sea_orm::DatabaseConnection;

use crate::error::AppError;
use crate::modules::users::repository::UserRepository;
use crate::vault::Vault;

pub struct AuthRepository<'a> {
    users: UserRepository<'a>,
}

impl<'a> AuthRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection, vault: &'a Vault) -> Self {
        Self {
            users: UserRepository::new(db, vault),
        }
    }

    pub async fn authenticate(&self, login: &str, password: &str) -> Result<i64, AppError> {
        self.users.authenticate(login, password).await
    }

    pub async fn register(&self, login: &str, password: &str) -> Result<i64, AppError> {
        self.users.register(login, password).await
    }
}
