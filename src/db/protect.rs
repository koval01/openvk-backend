//! Encrypt leftover plaintext PII and mint per-user wrap keys after migrate/seed.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::db::entities::{profile, user};
use crate::error::AppError;
use crate::vault::{
    self, Vault, aad_address, aad_birthday, aad_city, aad_contact_email, aad_email, aad_hometown,
    aad_phone,
};

pub async fn protect_stored_pii(db: &DatabaseConnection, vault: &Vault) -> Result<(), AppError> {
    let users = user::Entity::find().all(db).await?;
    for model in users {
        seal_user(db, vault, model).await?;
    }

    let profiles = profile::Entity::find().all(db).await?;
    for model in profiles {
        seal_profile(db, vault, model).await?;
    }

    tracing::info!("stored pii is sealed");
    Ok(())
}

async fn seal_user(
    db: &DatabaseConnection,
    vault: &Vault,
    model: user::Model,
) -> Result<(), AppError> {
    let id = model.id;
    let mut dirty = false;
    let mut active: user::ActiveModel = model.clone().into();

    if model.wrap_key.is_none() {
        let key = Vault::random_key();
        active.wrap_key = Set(Some(vault.wrap_user_key(id, &key)?));
        dirty = true;
    }

    if let Some((ciphertext, index)) = seal_indexed(
        vault,
        &aad_email(id),
        "email",
        model.email.as_deref(),
        model.email_idx.is_none(),
        vault::normalize_email,
    )? {
        active.email = Set(Some(ciphertext));
        active.email_idx = Set(Some(index));
        dirty = true;
    }

    if let Some((ciphertext, index)) = seal_indexed(
        vault,
        &aad_phone(id),
        "phone",
        model.phone.as_deref(),
        model.phone_idx.is_none(),
        vault::normalize_phone,
    )? {
        active.phone = Set(Some(ciphertext));
        active.phone_idx = Set(Some(index));
        dirty = true;
    }

    if let Some(ciphertext) = seal_plain(vault, &aad_city(id), model.city.as_deref())? {
        active.city = Set(Some(ciphertext));
        dirty = true;
    }

    if dirty {
        active.update(db).await?;
    }
    Ok(())
}

async fn seal_profile(
    db: &DatabaseConnection,
    vault: &Vault,
    model: profile::Model,
) -> Result<(), AppError> {
    let id = model.user_id;
    let mut dirty = false;
    let mut active: profile::ActiveModel = model.clone().into();

    if let Some(ciphertext) = seal_plain(vault, &aad_hometown(id), model.hometown.as_deref())? {
        active.hometown = Set(Some(ciphertext));
        dirty = true;
    }
    if let Some(ciphertext) = seal_plain(vault, &aad_birthday(id), model.birthday.as_deref())? {
        active.birthday = Set(Some(ciphertext));
        dirty = true;
    }
    if let Some(ciphertext) = seal_plain(vault, &aad_address(id), model.address.as_deref())? {
        active.address = Set(Some(ciphertext));
        dirty = true;
    }
    if let Some(ciphertext) = seal_plain(
        vault,
        &aad_contact_email(id),
        model.contact_email.as_deref(),
    )? {
        active.contact_email = Set(Some(ciphertext));
        dirty = true;
    }

    if dirty {
        active.update(db).await?;
    }
    Ok(())
}

fn seal_plain(vault: &Vault, aad: &str, value: Option<&str>) -> Result<Option<String>, AppError> {
    match value {
        Some(plain) if !plain.is_empty() && !Vault::is_sealed(plain) => {
            Ok(Some(vault.encrypt_field(aad, plain)?))
        }
        _ => Ok(None),
    }
}

fn seal_indexed(
    vault: &Vault,
    aad: &str,
    purpose: &str,
    value: Option<&str>,
    missing_index: bool,
    normalize: fn(&str) -> String,
) -> Result<Option<(String, String)>, AppError> {
    let Some(value) = value.filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    if Vault::is_sealed(value) {
        if missing_index {
            let plain = vault.decrypt_field(aad, value)?;
            return Ok(Some((
                value.to_owned(),
                vault.blind_index(purpose, &normalize(&plain)),
            )));
        }
        return Ok(None);
    }
    Ok(Some((
        vault.encrypt_field(aad, value)?,
        vault.blind_index(purpose, &normalize(value)),
    )))
}
