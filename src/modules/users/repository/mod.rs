use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, DatabaseConnection, EntityTrait,
    QueryFilter, TransactionTrait,
};

use crate::db::entities::user::Entity as UserEntity;
use crate::db::entities::{group, privacy_setting, profile, user};
use crate::error::AppError;
use crate::ids::{is_id_collision, random_public_id};
use crate::modules::users::models::{PrivacyLevel, UpdateAccount, User};
use crate::password;
use crate::vault::{self, Vault, aad_city, aad_email, aad_phone};

pub struct UserRepository<'a> {
    db: &'a DatabaseConnection,
    vault: &'a Vault,
    media_base_url: &'a str,
}

impl<'a> UserRepository<'a> {
    pub const fn new(
        db: &'a DatabaseConnection,
        vault: &'a Vault,
        media_base_url: &'a str,
    ) -> Self {
        Self {
            db,
            vault,
            media_base_url,
        }
    }

    pub async fn authenticate(&self, login: &str, password: &str) -> Result<i64, AppError> {
        let model = self
            .find_model_by_login(login)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !password::verify(password, &model.password_hash) {
            return Err(AppError::Unauthorized);
        }
        let id = model.id;
        if model.banned {
            if model.banned_until.is_some_and(|until| until <= Utc::now()) {
                let mut active: user::ActiveModel = model.clone().into();
                active.banned = Set(false);
                active.banned_until = Set(None);
                active.ban_reason = Set(None);
                active.update(self.db).await?;
            } else {
                return Err(AppError::Banned(
                    model
                        .ban_reason
                        .clone()
                        .filter(|reason| !reason.is_empty())
                        .unwrap_or_else(|| "account is banned".into()),
                ));
            }
        }
        if password::needs_rehash(&model.password_hash) {
            let mut active: user::ActiveModel = model.into();
            active.password_hash = Set(password::hash(password)?);
            active.password_algo = Set(password::ALGO.into());
            active.update(self.db).await?;
        }
        Ok(id)
    }

    pub async fn register(&self, login: &str, password: &str) -> Result<i64, AppError> {
        if self.find_model_by_login(login).await?.is_some() {
            return Err(AppError::Validation("login is already taken".into()));
        }

        let (first_name, last_name) = split_display_name(login);
        let login = login.trim().to_lowercase();
        let password_hash = password::hash(password)?;
        let everyone = PrivacyLevel::Everyone.as_db().to_owned();

        for _ in 0..32 {
            let txn = self.db.begin().await?;
            let id = random_public_id();
            let wrap = Vault::random_key();
            let model = user::ActiveModel {
                id: Set(id),
                login: Set(login.clone()),
                password_hash: Set(password_hash.clone()),
                password_algo: Set(password::ALGO.into()),
                first_name: Set(first_name.clone()),
                last_name: Set(last_name.clone()),
                screen_name: Set(Some(login.clone())),
                wrap_key: Set(Some(self.vault.wrap_user_key(id, &wrap)?)),
                privacy_wall: Set(everyone.clone()),
                privacy_messages: Set(everyone.clone()),
                privacy_profile: Set(everyone.clone()),
                privacy_photos: Set(everyone.clone()),
                privacy_audio: Set(everyone.clone()),
                verified: Set(false),
                wall_seq: Set(0),
                created_at: Set(Utc::now()),
                ..Default::default()
            };
            let inserted = match model.insert(&txn).await {
                Ok(inserted) => inserted,
                Err(error) if is_id_collision(&error) => {
                    txn.rollback().await?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            profile::ActiveModel {
                user_id: Set(inserted.id),
                updated_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;

            privacy_setting::ActiveModel {
                user_id: Set(inserted.id),
                profile: Set(everyone.clone()),
                photos: Set(everyone.clone()),
                audio: Set(everyone.clone()),
                wall: Set(everyone.clone()),
                messages: Set(everyone.clone()),
                friends_list: Set(everyone.clone()),
            }
            .insert(&txn)
            .await?;

            txn.commit().await?;
            return Ok(inserted.id);
        }

        Err(AppError::internal("could not allocate a public id"))
    }

    pub async fn ids_without_avatar(&self) -> Result<Vec<i64>, AppError> {
        Ok(UserEntity::find()
            .filter(user::Column::AvatarKey.is_null())
            .all(self.db)
            .await?
            .into_iter()
            .filter(|row| !is_integration_test_login(&row.login))
            .map(|row| row.id)
            .collect())
    }

    pub async fn find_by_key(&self, key: &str) -> Result<Option<User>, AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Ok(None);
        }
        if let Ok(id) = key.parse::<i64>() {
            if let Some(user) = self.find_by_id(id).await? {
                return Ok(Some(user));
            }
            // `/id1` is numeric in OpenVK; demo logins are `id1` with a random public id.
            let Some(model) = self.find_model_by_login(&format!("id{id}")).await? else {
                return Ok(None);
            };
            return self.find_by_id(model.id).await;
        }
        let Some(model) = self.find_model_by_login(key).await? else {
            return Ok(None);
        };
        self.find_by_id(model.id).await
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<User>, AppError> {
        let Some(model) = self.find_model(id).await? else {
            return Ok(None);
        };
        let friends = privacy_setting::Entity::find_by_id(id)
            .one(self.db)
            .await?
            .map(|row| PrivacyLevel::from_db(&row.friends_list))
            .unwrap_or_default();
        let mut user = into_user(self.vault, self.media_base_url, model)?;
        user.privacy_friends = friends;
        Ok(Some(user))
    }

    pub async fn find_model(&self, id: i64) -> Result<Option<user::Model>, AppError> {
        Ok(UserEntity::find_by_id(id).one(self.db).await?)
    }

    pub async fn find_many(&self, ids: &[i64]) -> Result<Vec<User>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = UserEntity::find()
            .filter(user::Column::Id.is_in(ids.iter().copied()))
            .all(self.db)
            .await?;
        models
            .into_iter()
            .map(|model| into_user(self.vault, self.media_base_url, model))
            .collect()
    }

    pub async fn update_account(&self, id: i64, patch: &UpdateAccount) -> Result<User, AppError> {
        let Some(model) = self.find_model(id).await? else {
            return Err(AppError::NotFound);
        };
        let email = empty_to_none(patch.email.as_deref());
        let phone = empty_to_none(patch.phone.as_deref());
        let city = empty_to_none(patch.city.as_deref());

        if let Some(ref email) = email {
            self.ensure_unique_index(id, user::Column::EmailIdx, "email", email)
                .await?;
        }
        if let Some(ref phone) = phone {
            self.ensure_unique_index(id, user::Column::PhoneIdx, "phone", phone)
                .await?;
        }

        let mut active: user::ActiveModel = model.into();
        active.first_name = Set(patch.first_name.trim().to_owned());
        active.last_name = Set(patch.last_name.trim().to_owned());
        active.email = Set(self.vault.encrypt_opt(&aad_email(id), email.as_deref())?);
        active.email_idx = Set(email.as_deref().map(|value| {
            self.vault
                .blind_index("email", &vault::normalize_email(value))
        }));
        active.phone = Set(self.vault.encrypt_opt(&aad_phone(id), phone.as_deref())?);
        active.phone_idx = Set(phone.as_deref().map(|value| {
            self.vault
                .blind_index("phone", &vault::normalize_phone(value))
        }));
        active.city = Set(self.vault.encrypt_opt(&aad_city(id), city.as_deref())?);
        active.privacy_wall = Set(patch.privacy_wall.as_db().to_owned());
        active.privacy_messages = Set(patch.privacy_messages.as_db().to_owned());
        active.privacy_photos = Set(patch.privacy_photos.as_db().to_owned());
        active.privacy_audio = Set(patch.privacy_audio.as_db().to_owned());
        active.privacy_profile = Set(patch.privacy_profile.as_db().to_owned());
        if let Some(ref status) = patch.status {
            active.status = Set(empty_to_none(Some(status.as_str())));
        }
        let updated = active.update(self.db).await?;

        if let Some(settings) = privacy_setting::Entity::find_by_id(id).one(self.db).await? {
            let mut settings: privacy_setting::ActiveModel = settings.into();
            settings.wall = Set(patch.privacy_wall.as_db().to_owned());
            settings.messages = Set(patch.privacy_messages.as_db().to_owned());
            settings.photos = Set(patch.privacy_photos.as_db().to_owned());
            settings.audio = Set(patch.privacy_audio.as_db().to_owned());
            settings.profile = Set(patch.privacy_profile.as_db().to_owned());
            settings.friends_list = Set(patch.privacy_friends.as_db().to_owned());
            settings.update(self.db).await?;
        }

        let mut user = into_user(self.vault, self.media_base_url, updated)?;
        user.privacy_friends = patch.privacy_friends;
        Ok(user)
    }

    pub async fn change_password(
        &self,
        id: i64,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        let Some(model) = self.find_model(id).await? else {
            return Err(AppError::NotFound);
        };
        if !password::verify(current_password, &model.password_hash) {
            return Err(AppError::Unauthorized);
        }
        let mut active: user::ActiveModel = model.into();
        active.password_hash = Set(password::hash(new_password)?);
        active.password_algo = Set(password::ALGO.into());
        active.update(self.db).await?;
        Ok(())
    }

    pub async fn set_avatar_key(
        &self,
        id: i64,
        avatar_key: Option<String>,
    ) -> Result<User, AppError> {
        let Some(model) = self.find_model(id).await? else {
            return Err(AppError::NotFound);
        };
        let mut active: user::ActiveModel = model.into();
        active.avatar_key = Set(avatar_key);
        let updated = active.update(self.db).await?;
        into_user(self.vault, self.media_base_url, updated)
    }

    pub async fn verify_password(&self, id: i64, password: &str) -> Result<(), AppError> {
        let Some(model) = self.find_model(id).await? else {
            return Err(AppError::NotFound);
        };
        if password::verify(password, &model.password_hash) {
            Ok(())
        } else {
            Err(AppError::Unauthorized)
        }
    }

    pub async fn delete_account(&self, id: i64) -> Result<(), AppError> {
        let txn = self.db.begin().await?;
        group::Entity::delete_many()
            .filter(group::Column::OwnerId.eq(id))
            .exec(&txn)
            .await?;
        let result = UserEntity::delete_by_id(id).exec(&txn).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound);
        }
        txn.commit().await?;
        Ok(())
    }

    async fn ensure_unique_index(
        &self,
        id: i64,
        column: user::Column,
        purpose: &str,
        value: &str,
    ) -> Result<(), AppError> {
        let normalized = match purpose {
            "phone" => vault::normalize_phone(value),
            _ => vault::normalize_email(value),
        };
        let index = self.vault.blind_index(purpose, &normalized);
        let taken = UserEntity::find()
            .filter(column.eq(index))
            .filter(user::Column::Id.ne(id))
            .one(self.db)
            .await?;
        if taken.is_some() {
            return Err(AppError::Validation(format!("{purpose} is already taken")));
        }
        Ok(())
    }

    async fn find_model_by_login(&self, login: &str) -> Result<Option<user::Model>, AppError> {
        let key = login.trim().to_lowercase();
        let email_idx = self
            .vault
            .blind_index("email", &vault::normalize_email(&key));
        Ok(UserEntity::find()
            .filter(
                Condition::any()
                    .add(user::Column::Login.eq(&key))
                    .add(user::Column::EmailIdx.eq(&email_idx))
                    .add(user::Column::ScreenName.eq(&key)),
            )
            .one(self.db)
            .await?)
    }
}

pub fn into_user(
    vault: &Vault,
    media_base_url: &str,
    model: user::Model,
) -> Result<User, AppError> {
    let id = model.id;
    Ok(User {
        id,
        first_name: model.first_name,
        last_name: model.last_name,
        screen_name: model.screen_name,
        status: model.status,
        city: vault.maybe_decrypt_opt(&aad_city(id), model.city)?,
        email: vault.maybe_decrypt_opt(&aad_email(id), model.email)?,
        phone: vault.maybe_decrypt_opt(&aad_phone(id), model.phone)?,
        avatar_url: model
            .avatar_key
            .map(|key| crate::modules::media::kinds::public_media_url(media_base_url, &key)),
        verified: model.verified,
        privacy_wall: PrivacyLevel::from_db(&model.privacy_wall),
        privacy_messages: PrivacyLevel::from_db(&model.privacy_messages),
        privacy_photos: PrivacyLevel::from_db(&model.privacy_photos),
        privacy_audio: PrivacyLevel::from_db(&model.privacy_audio),
        privacy_profile: PrivacyLevel::from_db(&model.privacy_profile),
        privacy_friends: PrivacyLevel::Everyone,
        created_at: model.created_at,
        coins: model.coins,
        rating: model.rating,
        role: model.role,
        banned: model.banned,
        ban_reason: model.ban_reason,
        banned_until: model.banned_until,
        support_banned: model.support_banned,
        support_ban_reason: model.support_ban_reason,
        posting_allowed: model.posting_allowed,
        messaging_allowed: model.messaging_allowed,
    })
}

fn split_display_name(login: &str) -> (String, String) {
    let trimmed = login.trim();
    if let Some((first, last)) = trimmed.split_once(' ') {
        (first.to_owned(), last.to_owned())
    } else {
        (trimmed.to_owned(), "User".into())
    }
}

fn empty_to_none(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

/// `tests/common` issues `t{millis}{seq}` logins. Skip them so a local `cargo run`
/// does not fetch a DiceBear portrait for every leftover API test account.
fn is_integration_test_login(login: &str) -> bool {
    login
        .strip_prefix('t')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
}
