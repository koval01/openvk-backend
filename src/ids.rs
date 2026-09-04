//! Public identifiers for globally visible objects.
//!
//! Accounts, communities, media, and other objects that live in a single
//! namespace use a random positive 32-bit id (Telegram-style) so an id does
//! not reveal registration order or table size. Objects scoped to one owner,
//! such as a wall post, keep a sequential local id and are addressed as
//! `wall{owner}_{local}` like VK.

use rand::Rng;
use sea_orm::DbErr;

use crate::error::AppError;

/// Smallest public id. `0` is reserved and never issued.
pub const PUBLIC_ID_MIN: i64 = 1;
/// Largest public id (`i32::MAX`). The sign bit stays free for group walls.
pub const PUBLIC_ID_MAX: i64 = i32::MAX as i64;

const ALLOCATE_ATTEMPTS: u32 = 32;

#[must_use]
pub fn random_public_id() -> i64 {
    i64::from(rand::rng().random_range(1..=i32::MAX))
}

#[must_use]
pub fn is_public_id(id: i64) -> bool {
    (PUBLIC_ID_MIN..=PUBLIC_ID_MAX).contains(&id)
}

#[must_use]
pub fn wall_permalink(owner_id: i64, local_id: i64) -> String {
    format!("wall{owner_id}_{local_id}")
}

#[must_use]
pub fn is_id_collision(error: &DbErr) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("duplicate key")
        && (text.contains("_pkey")
            || text.contains("(id)")
            || text.contains("unique constraint") && text.contains("id"))
}

pub async fn insert_with_random_id<T, F, Fut>(mut insert: F) -> Result<T, AppError>
where
    F: FnMut(i64) -> Fut,
    Fut: std::future::Future<Output = Result<T, DbErr>>,
{
    for _ in 0..ALLOCATE_ATTEMPTS {
        let id = random_public_id();
        match insert(id).await {
            Ok(value) => return Ok(value),
            Err(error) if is_id_collision(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::internal("could not allocate a public id"))
}

#[cfg(test)]
mod tests {
    use super::{PUBLIC_ID_MAX, PUBLIC_ID_MIN, is_public_id, random_public_id, wall_permalink};

    #[test]
    fn public_ids_fit_in_signed_32_bits() {
        for _ in 0..64 {
            let id = random_public_id();
            assert!(is_public_id(id), "{id}");
            assert_ne!(id, 0);
        }
        assert_eq!(PUBLIC_ID_MIN, 1);
        assert_eq!(PUBLIC_ID_MAX, 2_147_483_647);
    }

    #[test]
    fn wall_permalink_matches_vk_shape() {
        assert_eq!(wall_permalink(1_847_201_993, 12), "wall1847201993_12");
    }
}
