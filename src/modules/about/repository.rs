use chrono::{Duration, Utc};
use sea_orm::{
    ColumnTrait, Condition, DatabaseBackend, DatabaseConnection, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, Statement,
};

use crate::db::entities::{group, user, wall_post};
use crate::error::AppError;
use crate::modules::about::models::{InstanceAbout, PopularGroup};

#[derive(FromQueryResult)]
struct PopularRow {
    id: i64,
    name: String,
    members: i64,
}

pub struct AboutRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> AboutRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn instance(&self) -> Result<InstanceAbout, AppError> {
        let now = Utc::now();
        let users = i64_count(user::Entity::find().count(self.db).await?);
        let groups = i64_count(group::Entity::find().count(self.db).await?);
        let wall_posts = i64_count(
            wall_post::Entity::find()
                .filter(wall_post::Column::DeletedAt.is_null())
                .count(self.db)
                .await?,
        );
        let online_users = i64_count(
            user::Entity::find()
                .filter(user::Column::LastSeenAt.gt(now - Duration::minutes(15)))
                .count(self.db)
                .await?,
        );
        let active_users = i64_count(
            user::Entity::find()
                .filter(
                    Condition::any()
                        .add(user::Column::LastSeenAt.gt(now - Duration::days(30)))
                        .add(user::Column::CreatedAt.gt(now - Duration::days(30))),
                )
                .count(self.db)
                .await?,
        );
        let popular_groups = self.popular_groups().await?;
        Ok(InstanceAbout {
            users,
            online_users,
            active_users,
            groups,
            wall_posts,
            popular_groups,
        })
    }

    async fn popular_groups(&self) -> Result<Vec<PopularGroup>, AppError> {
        let rows = PopularRow::find_by_statement(Statement::from_string(
            DatabaseBackend::Postgres,
            "
            SELECT g.id, g.name, COUNT(m.user_id)::bigint AS members
            FROM groups g
            LEFT JOIN group_members m
                ON m.group_id = g.id AND m.status = 'active'
            GROUP BY g.id, g.name
            ORDER BY members DESC, g.name ASC
            LIMIT 20
            ",
        ))
        .all(self.db)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PopularGroup {
                id: row.id,
                name: row.name,
                members: row.members,
            })
            .collect())
    }
}

fn i64_count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
