use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub nats: NatsConfig,
    pub scheduler: SchedulerConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime_secs: u64,
    pub idle_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NatsConfig {
    pub url: String,
    pub stream: String,
    pub subject: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    pub batch_size: usize,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub api_key: String,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let config = Self {
            server: ServerConfig {
                port: parse_env("SERVER_PORT", "3000").context("Failed to parse SERVER_PORT")?,
                host: std::env::var("SERVER_HOST")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
                max_connections: parse_env("DB_MAX_CONNECTIONS", "25")?,
                min_connections: parse_env("DB_MIN_CONNECTIONS", "5")?,
                max_lifetime_secs: parse_env("DB_MAX_LIFETIME_SECS", "3600")?,
                idle_timeout_secs: parse_env("DB_IDLE_TIMEOUT_SECS", "900")?,
            },
            nats: NatsConfig {
                url: std::env::var("NATS_URL")
                    .unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string()),
                stream: std::env::var("NATS_STREAM").unwrap_or_else(|_| "messages".to_string()),
                subject: std::env::var("NATS_SUBJECT")
                    .unwrap_or_else(|_| "messages.email".to_string()),
            },
            scheduler: SchedulerConfig {
                batch_size: parse_env("BATCH_SIZE", "1000")
                    .context("Failed to parse BATCH_SIZE")?,
                interval_secs: parse_env("SCHEDULER_INTERVAL", "60")
                    .context("Failed to parse SCHEDULER_INTERVAL")?,
            },
            security: SecurityConfig {
                api_key: std::env::var("API_KEY").context("API_KEY must be set")?,
            },
        };

        info!("설정 로드 성공");
        Ok(config)
    }
}

fn parse_env<T>(key: &str, default: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    std::env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", key, e))
}

pub struct Database;

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> Result<PgPool> {
        info!("데이터베이스에 연결 중...");

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
            .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
            .connect(&config.url)
            .await
            .context("Failed to connect to database")?;

        info!("데이터베이스 연결 성공");
        Ok(pool)
    }

    pub async fn migrate(pool: &PgPool) -> Result<()> {
        info!("데이터베이스 마이그레이션 실행 중...");
        sqlx::migrate!("./migrations")
            .run(pool)
            .await
            .context("Failed to run database migrations")?;
        info!("데이터베이스 마이그레이션 완료");
        Ok(())
    }
}
