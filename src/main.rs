use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

mod api;
mod config;
mod dto;
mod error;
mod models;
mod services;

use config::{AppConfig, Database};
use services::{producer::ProducerService, scheduler::SchedulerService};

#[tokio::main]
async fn main() -> Result<()> {
    // 더 나은 형식으로 트레이싱 초기화
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .with_thread_ids(true)
        .init();

    info!("🚀 Starting Messages API Gateway");

    // 설정 로드
    let config = Arc::new(AppConfig::load().context("Failed to load configuration")?);

    // 데이터베이스 초기화
    let db = Database::connect(&config.database)
        .await
        .context("Failed to initialize database")?;
    Database::migrate(&db)
        .await
        .context("Failed to run database migrations")?;

    // NATS 프로듀서 초기화
    let producer = Arc::new(
        ProducerService::new(&config.nats)
            .await
            .context("Failed to initialize NATS producer")?,
    );

    // 스케줄러 서비스 생성
    let scheduler = SchedulerService::new(db.clone(), producer.clone(), config.clone());

    // 서비스를 동시에 시작
    let scheduler_handle = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            info!("📧 Starting email scheduler service");
            if let Err(e) = scheduler.run().await {
                error!("Scheduler service failed: {:#}", e);
            } else {
                info!("📧 Email scheduler service stopped gracefully");
            }
        }
    });

    let server_handle = tokio::spawn({
        let db = db.clone();
        let config = config.clone();
        async move {
            info!("🌐 Starting HTTP server on port {}", config.server.port);
            if let Err(e) = api::server::run(db, config).await {
                error!("HTTP server failed: {:#}", e);
            } else {
                info!("🌐 HTTP server stopped gracefully");
            }
        }
    });

    // 우아한 종료 처리
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("🛑 Received shutdown signal (Ctrl+C)");
        }
        result = scheduler_handle => {
            match result {
                Ok(_) => info!("📧 Scheduler service completed"),
                Err(e) => error!("📧 Scheduler service panicked: {}", e),
            }
        }
        result = server_handle => {
            match result {
                Ok(_) => info!("🌐 HTTP server completed"),
                Err(e) => error!("🌐 HTTP server panicked: {}", e),
            }
        }
    }

    info!("✅ Application shutdown completed");
    Ok(())
}
