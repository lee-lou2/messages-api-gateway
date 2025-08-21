use crate::{api::handlers, api::middleware::auth_middleware, config::AppConfig};
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use sqlx::PgPool;
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{info, Level};

pub async fn run(db: PgPool, config: Arc<AppConfig>) -> anyhow::Result<()> {
    let app = create_app(db, config.clone()).await;

    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    info!("🌐 HTTP server binding to {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🌐 HTTP server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("🌐 HTTP server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("🌐 Shutdown signal received, starting graceful shutdown");
}

async fn create_app(db: PgPool, config: Arc<AppConfig>) -> Router {
    // 공유 상태 생성
    let state = handlers::AppState { db, config };

    // 미들웨어 스택 생성
    let middleware_stack = ServiceBuilder::new()
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(
                    DefaultMakeSpan::new()
                        .level(Level::INFO)
                        .include_headers(true),
                )
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                )
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // 보호된 라우트 (API 키 필요)
    let protected_routes = Router::new()
        .route("/v1/messages", post(handlers::create_message))
        .route("/v1/topics/:topic_id", get(handlers::get_result_count))
        .route("/v1/events/counts/sent", get(handlers::get_sent_count))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 공개 라우트
    let public_routes = Router::new()
        .route("/v1/events/open", get(handlers::create_open_event))
        .route("/v1/events/results", post(handlers::create_result_event))
        .route("/health", get(handlers::health_check));

    // 모든 라우트 결합
    Router::new()
        .merge(protected_routes)
        .merge(public_routes)
        .layer(middleware_stack)
        .with_state(state)
}
