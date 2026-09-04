//! Versioned SQL migrations compatible with `SeaORM`'s `seaql_migrations` table.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::error::AppError;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "20260903_000001_core",
        include_str!("../../migrations/20260903_000001_core.sql"),
    ),
    (
        "20260903_000002_domains",
        include_str!("../../migrations/20260903_000002_domains.sql"),
    ),
    (
        "20260903_000003_public_ids",
        include_str!("../../migrations/20260903_000003_public_ids.sql"),
    ),
    (
        "20260904_000001_media_public_urls",
        include_str!("../../migrations/20260904_000001_media_public_urls.sql"),
    ),
    (
        "20260904_000002_data_protection",
        include_str!("../../migrations/20260904_000002_data_protection.sql"),
    ),
    (
        "20260904_000003_avatar_storage_key",
        include_str!("../../migrations/20260904_000003_avatar_storage_key.sql"),
    ),
    (
        "20260904_000004_wall_attachments",
        include_str!("../../migrations/20260904_000004_wall_attachments.sql"),
    ),
    (
        "20260904_000005_desk",
        include_str!("../../migrations/20260904_000005_desk.sql"),
    ),
    (
        "20260904_000006_likes",
        include_str!("../../migrations/20260904_000006_likes.sql"),
    ),
];

pub async fn run(db: &DatabaseConnection) -> Result<(), AppError> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS seaql_migrations (
            version VARCHAR(255) PRIMARY KEY,
            applied_at BIGINT NOT NULL
        )",
    )
    .await?;

    db.execute_unprepared("SELECT pg_advisory_lock(8723641)")
        .await?;
    let result = apply(db).await;
    db.execute_unprepared("SELECT pg_advisory_unlock(8723641)")
        .await?;
    result
}

async fn apply(db: &DatabaseConnection) -> Result<(), AppError> {
    adopt_legacy_bootstrap(db).await?;

    for (version, sql) in MIGRATIONS {
        if already_applied(db, version).await? {
            continue;
        }
        tracing::info!(version, "applying migration");
        for statement in split_statements(sql) {
            db.execute_unprepared(&statement).await?;
        }
        record(db, version).await?;
    }

    tracing::info!("database migrations are up to date");
    Ok(())
}

async fn adopt_legacy_bootstrap(db: &DatabaseConnection) -> Result<(), AppError> {
    let users_exist = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = 'users'
            ) AS present",
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<bool>(0).ok())
        .unwrap_or(false);

    if users_exist && !already_applied(db, MIGRATIONS[0].0).await? {
        record(db, MIGRATIONS[0].0).await?;
        tracing::info!("recorded legacy users/wall schema as 20260903_000001_core");
    }
    Ok(())
}

async fn already_applied(db: &DatabaseConnection, version: &str) -> Result<bool, AppError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM seaql_migrations WHERE version = $1",
            [version.into()],
        ))
        .await?;
    Ok(row.is_some())
}

async fn record(db: &DatabaseConnection, version: &str) -> Result<(), AppError> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO seaql_migrations (version, applied_at)
         VALUES ($1, EXTRACT(EPOCH FROM NOW())::BIGINT)
         ON CONFLICT (version) DO NOTHING",
        [version.into()],
    ))
    .await?;
    Ok(())
}

fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    let mut in_single = false;
    let mut dollar_tag: Option<String> = None;

    while index < sql.len() {
        if let Some(tag) = &dollar_tag {
            if sql[index..].starts_with(tag) {
                current.push_str(tag);
                index += tag.len();
                dollar_tag = None;
                continue;
            }
            let ch = sql[index..].chars().next().expect("index in range");
            current.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if in_single {
            let ch = sql[index..].chars().next().expect("index in range");
            current.push(ch);
            index += ch.len_utf8();
            if ch == '\'' {
                if sql[index..].starts_with('\'') {
                    current.push('\'');
                    index += 1;
                } else {
                    in_single = false;
                }
            }
            continue;
        }

        if sql[index..].starts_with("--") {
            if let Some(newline) = sql[index..].find('\n') {
                index += newline + 1;
            } else {
                break;
            }
            continue;
        }

        if sql[index..].starts_with('$') {
            if let Some(tag) = parse_dollar_tag(&sql[index..]) {
                dollar_tag = Some(tag.clone());
                current.push_str(&tag);
                index += tag.len();
                continue;
            }
        }

        let ch = sql[index..].chars().next().expect("index in range");
        if ch == '\'' {
            in_single = true;
            current.push(ch);
            index += 1;
            continue;
        }
        if ch == ';' {
            push_statement(&mut statements, &mut current);
            index += 1;
            continue;
        }

        current.push(ch);
        index += ch.len_utf8();
    }

    push_statement(&mut statements, &mut current);
    statements
}

fn parse_dollar_tag(sql: &str) -> Option<String> {
    let rest = sql.get(1..)?;
    let close = rest.find('$')?;
    let inner = &rest[..close];
    if inner
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(format!("${inner}$"))
    } else {
        None
    }
}

fn push_statement(statements: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_owned());
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::split_statements;

    #[test]
    fn splits_plain_statements_and_dollar_blocks() {
        let sql = "
            CREATE TABLE a (id INT);
            DO $$
            BEGIN
                PERFORM 1;
            END $$;
            INSERT INTO a VALUES (1);
        ";
        let statements = split_statements(sql);
        assert_eq!(statements.len(), 3);
        assert!(statements[1].starts_with("DO $$"));
        assert!(statements[1].ends_with("END $$"));
    }
}
