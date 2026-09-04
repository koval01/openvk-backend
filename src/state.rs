use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

use jsonwebtoken::{DecodingKey, EncodingKey};
use moka::future::Cache;
use redis::aio::ConnectionManager;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::sync::broadcast;

use crate::config::Config;
use crate::error::AppError;
use crate::identity::ProcessIdentity;
use crate::modules::users::User;
use crate::modules::users::repository::UserRepository;
use crate::modules::wall::WallPost;
use crate::security::Security;
use crate::storage::Storage;
use crate::turnstile::Turnstile;
use crate::vault::Vault;

const USER_CACHE_TTL: Duration = Duration::from_secs(30);
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const EVENT_BUFFER: usize = 1024;

#[derive(Clone)]
pub struct Caches {
    pub users: Cache<i64, User>,
    pub friend_ids: Cache<i64, Vec<i64>>,
    pub sessions: Cache<String, i64>,
    pub posts: Cache<(i64, i64), WallPost>,
    pub rate_limit: Cache<String, Arc<AtomicU32>>,
}

pub struct JwtKeys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: ConnectionManager,
    #[allow(dead_code)]
    pub redis_client: redis::Client,
    pub caches: Caches,
    pub jwt: Arc<JwtKeys>,
    pub events: broadcast::Sender<String>,
    pub config: Arc<Config>,
    pub storage: Storage,
    pub turnstile: Turnstile,
    pub identity: ProcessIdentity,
    pub security: Security,
    pub vault: Vault,
}

impl AppState {
    pub async fn connect(config: Config) -> Result<Self, AppError> {
        let db = connect_postgres(&config).await?;
        let vault = Vault::from_secret(&config.data_key)?;
        crate::db::migrator::run(&db).await?;
        crate::db::seed::demo_accounts(&db, &vault).await?;
        crate::db::protect::protect_stored_pii(&db, &vault).await?;
        let redis_client = redis::Client::open(config.redis_url.as_str())?;
        let redis = ConnectionManager::new(redis_client.clone()).await?;

        tracing::info!("postgres and redis connections established");

        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let jwt = Arc::new(JwtKeys {
            encoding: EncodingKey::from_secret(config.jwt_secret.as_bytes()),
            decoding: DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        });
        let storage = Storage::from_config(&config)?;
        tracing::info!(backend = storage.backend_name(), "object storage ready");
        let turnstile = Turnstile::new(
            config.turnstile_secret_key.clone(),
            config.turnstile_siteverify_url.clone(),
        );
        let security = Security::new(&config.jwt_secret)?;

        Ok(Self {
            db,
            redis,
            redis_client,
            caches: Caches {
                users: Cache::builder()
                    .max_capacity(50_000)
                    .time_to_live(USER_CACHE_TTL)
                    .build(),
                friend_ids: Cache::builder()
                    .max_capacity(50_000)
                    .time_to_live(USER_CACHE_TTL)
                    .build(),
                sessions: Cache::builder()
                    .max_capacity(200_000)
                    .time_to_live(Duration::from_secs(3600))
                    .build(),
                posts: Cache::builder()
                    .max_capacity(20_000)
                    .time_to_live(USER_CACHE_TTL)
                    .build(),
                rate_limit: Cache::builder()
                    .max_capacity(100_000)
                    .time_to_live(RATE_LIMIT_WINDOW)
                    .build(),
            },
            jwt,
            events,
            config: Arc::new(config),
            storage,
            turnstile,
            identity: ProcessIdentity::new(),
            security,
            vault,
        })
    }

    pub const fn users(&self) -> UserRepository<'_> {
        UserRepository::new(&self.db, &self.vault)
    }

    pub async fn media_storage_key(&self, media_id: i64) -> Result<String, AppError> {
        Ok(
            crate::modules::media::repository::MediaRepository::new(&self.db)
                .get_media(media_id)
                .await?
                .storage_key,
        )
    }
}

async fn connect_postgres(config: &Config) -> Result<DatabaseConnection, AppError> {
    let mut options = ConnectOptions::new(config.database_url.clone());
    options
        .max_connections(config.db_max_connections)
        .min_connections(config.db_min_connections)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(60))
        .max_lifetime(Duration::from_secs(300))
        .sqlx_logging(false);

    Ok(Database::connect(options).await?)
}
