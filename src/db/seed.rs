use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, PaginatorTrait, Statement,
};

use crate::db::entities::wall_post;
use crate::error::AppError;
use crate::ids::{is_id_collision, random_public_id};
use crate::password;
use crate::vault::{self, Vault, aad_city, aad_email, aad_phone};

pub async fn demo_accounts(db: &DatabaseConnection, vault: &Vault) -> Result<(), AppError> {
    let hash = password::hash("openvk")?;
    ensure_user(
        db,
        vault,
        &hash,
        "id1",
        "Ivan",
        "Petrov",
        "id1",
        "OpenVK rewrite in progress",
        "Saint Petersburg",
        "ivan@openvk.local",
        Some("+7 812 000-00-01"),
        "everyone",
        "everyone",
        "everyone",
        "everyone",
        "everyone",
    )
    .await?;
    ensure_user(
        db,
        vault,
        &hash,
        "anna",
        "Anna",
        "Sokolova",
        "anna",
        "Hello from Moscow",
        "Moscow",
        "anna@openvk.local",
        None,
        "friends",
        "everyone",
        "everyone",
        "friends",
        "everyone",
    )
    .await?;
    ensure_user(
        db,
        vault,
        &hash,
        "pavel",
        "Pavel",
        "Orlov",
        "pavel",
        "Migrating walls to Rust",
        "Kazan",
        "pavel@openvk.local",
        None,
        "everyone",
        "friends",
        "everyone",
        "everyone",
        "friends",
    )
    .await?;

    upgrade_demo_passwords(db, &hash).await?;

    let id1 = user_id(db, "id1").await?;
    let anna = user_id(db, "anna").await?;
    let pavel = user_id(db, "pavel").await?;

    db.execute_unprepared(&format!(
        "
        INSERT INTO friendships (user_id, friend_id) VALUES
            ({id1}, {anna}), ({anna}, {id1}), ({id1}, {pavel}), ({pavel}, {id1})
        ON CONFLICT DO NOTHING
        "
    ))
    .await?;

    db.execute_unprepared(&format!(
        "
        INSERT INTO profiles (user_id, hometown, university, interests, favorite_quotes)
        VALUES
            ({id1}, 'Saint Petersburg', 'ITMO', 'Rust, vintage VK', 'The wall is a guestbook.'),
            ({anna}, 'Moscow', NULL, 'Photography', NULL),
            ({pavel}, 'Kazan', 'KFU', 'Audio encoding', NULL)
        ON CONFLICT (user_id) DO NOTHING
        "
    ))
    .await?;
    db.execute_unprepared(&format!(
        "
        UPDATE profiles SET hometown = CASE user_id
            WHEN {id1} THEN 'Saint Petersburg'
            WHEN {anna} THEN 'Moscow'
            WHEN {pavel} THEN 'Kazan'
        END
        WHERE hometown IS NULL AND user_id IN ({id1}, {anna}, {pavel})
        "
    ))
    .await?;

    db.execute_unprepared(
        "
        INSERT INTO privacy_settings (user_id, profile, photos, audio, wall, messages, friends_list)
        SELECT id, privacy_profile, privacy_photos, privacy_audio, privacy_wall, privacy_messages, 'everyone'
        FROM users
        ON CONFLICT (user_id) DO NOTHING
        ",
    )
    .await?;

    ensure_group(db, "openvk", "OpenVK", "Classic VKontakte rewrite.", id1).await?;
    let group_id = group_id(db, "openvk").await?;

    db.execute_unprepared(&format!(
        "
        INSERT INTO group_members (group_id, user_id, role, status) VALUES
            ({group_id}, {id1}, 'owner', 'active'),
            ({group_id}, {anna}, 'member', 'active'),
            ({group_id}, {pavel}, 'moderator', 'active')
        ON CONFLICT DO NOTHING
        "
    ))
    .await?;

    let wall_count = wall_post::Entity::find().count(db).await?;
    if wall_count == 0 {
        db.execute_unprepared(&format!(
            "
            INSERT INTO wall_posts (target_id, author_id, content, local_id) VALUES
                ({id1}, {anna}, 'Welcome to the classic wall — this is a guestbook, not a microblog.', 1),
                ({id1}, {pavel}, 'Left a note on your page, the 2007 way.', 2),
                ({anna}, {id1}, 'Anna, thanks for dropping by.', 1)
            "
        ))
        .await?;
        db.execute_unprepared(&format!("UPDATE users SET wall_seq = 2 WHERE id = {id1}"))
            .await?;
        db.execute_unprepared(&format!(
            "UPDATE users SET wall_seq = GREATEST(wall_seq, 1) WHERE id = {anna}"
        ))
        .await?;
    }

    Ok(())
}

async fn upgrade_demo_passwords(db: &DatabaseConnection, hash: &str) -> Result<(), AppError> {
    for login in ["id1", "anna", "pavel"] {
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users SET password_hash = $1, password_algo = $2
             WHERE login = $3 AND password_hash NOT LIKE '$argon2%'",
            [hash.into(), password::ALGO.into(), login.into()],
        ))
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn ensure_user(
    db: &DatabaseConnection,
    vault: &Vault,
    hash: &str,
    login: &str,
    first_name: &str,
    last_name: &str,
    screen_name: &str,
    status: &str,
    city: &str,
    email: &str,
    phone: Option<&str>,
    privacy_wall: &str,
    privacy_messages: &str,
    privacy_profile: &str,
    privacy_photos: &str,
    privacy_audio: &str,
) -> Result<(), AppError> {
    if lookup_id(db, "SELECT id FROM users WHERE login = $1", login)
        .await?
        .is_some()
    {
        return Ok(());
    }
    for _ in 0..32 {
        let id = random_public_id();
        let wrap = Vault::random_key();
        let email_ct = vault.encrypt_field(&aad_email(id), email)?;
        let email_idx = vault.blind_index("email", &vault::normalize_email(email));
        let city_ct = vault.encrypt_field(&aad_city(id), city)?;
        let phone_ct = phone
            .map(|value| vault.encrypt_field(&aad_phone(id), value))
            .transpose()?;
        let phone_idx =
            phone.map(|value| vault.blind_index("phone", &vault::normalize_phone(value)));
        let wrap_key = vault.wrap_user_key(id, &wrap)?;
        match db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "
                INSERT INTO users (
                    id, login, password_hash, password_algo, first_name, last_name, screen_name,
                    status, city, email, email_idx, phone, phone_idx, wrap_key, verified,
                    privacy_wall, privacy_messages, privacy_profile, privacy_photos, privacy_audio,
                    wall_seq
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7,
                    $8, $9, $10, $11, $12, $13, $14, FALSE,
                    $15, $16, $17, $18, $19, 0
                )
                ON CONFLICT (login) DO NOTHING
                ",
                [
                    id.into(),
                    login.into(),
                    hash.into(),
                    password::ALGO.into(),
                    first_name.into(),
                    last_name.into(),
                    screen_name.into(),
                    status.into(),
                    city_ct.into(),
                    email_ct.into(),
                    email_idx.into(),
                    phone_ct.into(),
                    phone_idx.into(),
                    wrap_key.into(),
                    privacy_wall.into(),
                    privacy_messages.into(),
                    privacy_profile.into(),
                    privacy_photos.into(),
                    privacy_audio.into(),
                ],
            ))
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) if is_id_collision(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::internal("could not allocate a public id"))
}

async fn ensure_group(
    db: &DatabaseConnection,
    slug: &str,
    name: &str,
    about: &str,
    owner_id: i64,
) -> Result<(), AppError> {
    if lookup_id(db, "SELECT id FROM groups WHERE slug = $1", slug)
        .await?
        .is_some()
    {
        return Ok(());
    }
    for _ in 0..32 {
        let id = random_public_id();
        let sql = format!(
            "
            INSERT INTO groups (id, slug, name, about, kind, owner_id)
            VALUES ({id}, '{slug}', '{name}', '{about}', 'open', {owner_id})
            ON CONFLICT (slug) DO NOTHING
            "
        );
        match db.execute_unprepared(&sql).await {
            Ok(_) => return Ok(()),
            Err(error) if is_id_collision(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::internal("could not allocate a public id"))
}

async fn user_id(db: &DatabaseConnection, login: &str) -> Result<i64, AppError> {
    lookup_id(db, "SELECT id FROM users WHERE login = $1", login)
        .await?
        .ok_or_else(|| AppError::internal(format!("demo user {login} is missing")))
}

async fn group_id(db: &DatabaseConnection, slug: &str) -> Result<i64, AppError> {
    lookup_id(db, "SELECT id FROM groups WHERE slug = $1", slug)
        .await?
        .ok_or_else(|| AppError::internal(format!("demo group {slug} is missing")))
}

async fn lookup_id(db: &DatabaseConnection, sql: &str, key: &str) -> Result<Option<i64>, AppError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [key.into()],
        ))
        .await?;
    Ok(row.and_then(|row| row.try_get_by_index::<i64>(0).ok()))
}
