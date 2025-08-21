use crate::{
    config::AppConfig,
    dto::*,
    error::{AppError, Result},
    models::email::EmailStatus,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;
use validator::Validate;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
}

pub async fn create_message(
    State(state): State<AppState>,
    Json(payload): Json<CreateMessageRequest>,
) -> Result<Json<CreateMessageResponse>> {
    let start = Instant::now();

    // 요청 검증
    payload
        .validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let mut total_count = 0;
    let mut tx = state.db.begin().await?;

    for message in payload.messages {
        let now = Utc::now();
        // Validate scheduled_at is not too far in the past
        if let Some(scheduled_at) = message.scheduled_at {
            if scheduled_at < now - chrono::Duration::hours(1) {
                return Err(AppError::Validation(
                    "Scheduled time cannot be more than 1 hour in the past".to_string(),
                ));
            }
        }

        // 내용 생성
        let content_id = sqlx::query_scalar!(
            "INSERT INTO email_contents (subject, content, created_at, updated_at) 
             VALUES ($1, $2, $3, $3) RETURNING id",
            message.subject.trim(),
            message.content.trim(),
            now
        )
        .fetch_one(&mut *tx)
        .await?;

        // 이메일 요청 배치 삽입
        let scheduled_at = message.scheduled_at;
        let topic_id = message.topic_id.unwrap_or(String::new());

        for email in &message.emails {
            let request_id = Uuid::now_v7();

            sqlx::query!(
                "INSERT INTO email_requests (id, topic_id, to_email, content_id, scheduled_at, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
                request_id,
                topic_id,
                email.trim(),
                content_id,
                scheduled_at,
                EmailStatus::Created as i16,
                now
            )
            .execute(&mut *tx)
            .await?;

            total_count += 1;
        }

        debug!(
            "📧 Created {} email requests for topic_id: {}",
            message.emails.len(),
            topic_id
        );
    }

    tx.commit().await?;

    let elapsed = start.elapsed();
    info!(
        "📧 Successfully created {} message requests in {:?}",
        total_count, elapsed
    );

    Ok(Json(CreateMessageResponse {
        count: total_count,
        elapsed: format!("{:?}", elapsed),
    }))
}

pub async fn create_open_event(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response> {
    // 픽셀 응답을 차단하지 않도록 비동기적으로 열림 이벤트 기록
    if let Some(request_id) = params.get("requestId") {
        if let Ok(uuid) = Uuid::parse_str(request_id) {
            let db = state.db.clone();
            tokio::spawn(async move {
                let result = sqlx::query!(
                    "INSERT INTO email_results (request_id, status, raw, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $4)
                     ON CONFLICT (request_id, status) DO NOTHING",
                    uuid,
                    "Open",
                    serde_json::json!({
                        "timestamp": Utc::now(),
                        "user_agent": "tracking-pixel"
                    }),
                    Utc::now()
                )
                .execute(&db)
                .await;

                match result {
                    Ok(result) if result.rows_affected() > 0 => {
                        debug!("📧 Email open event recorded for request_id: {}", uuid);
                    }
                    Ok(_) => {
                        debug!(
                            "📧 Email open event already exists for request_id: {}",
                            uuid
                        );
                    }
                    Err(e) => {
                        warn!("📧 Failed to create open event for {}: {}", uuid, e);
                    }
                }
            });
        } else {
            warn!("📧 Invalid request_id format in open event: {}", request_id);
        }
    }

    // 1x1 투명 PNG 즉시 반환
    const TRANSPARENT_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, // Width: 1
        0x00, 0x00, 0x00, 0x01, // Height: 1
        0x08, 0x06, 0x00, 0x00, 0x00, // Bit depth, color type, compression, filter, interlace
        0x1F, 0x15, 0xC4, 0x89, // CRC
        0x00, 0x00, 0x00, 0x0A, // IDAT chunk length
        0x49, 0x44, 0x41, 0x54, // IDAT
        0x78, 0x9C, 0x62, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, // Compressed data
        0xE2, 0x21, 0xBC, 0x33, // CRC
        0x00, 0x00, 0x00, 0x00, // IEND chunk length
        0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82, // CRC
    ];

    Ok((
        StatusCode::OK,
        [
            ("Content-Type", "image/png"),
            ("Cache-Control", "no-cache, no-store, must-revalidate"),
            ("Content-Length", &TRANSPARENT_PNG.len().to_string()),
        ],
        TRANSPARENT_PNG,
    )
        .into_response())
}

pub async fn create_result_event(
    State(state): State<AppState>,
    Json(payload): Json<SnsMessage>,
) -> Result<Json<serde_json::Value>> {
    if payload.message_type == "SubscriptionConfirmation" {
        info!(
            "SNS subscription confirmation required: {:?}",
            payload.subscribe_url
        );
        return Ok(Json(
            serde_json::json!({"message": "Subscription confirmation required"}),
        ));
    }

    if payload.message_type != "Notification" {
        info!(
            "Non-notification SNS message received: {}",
            payload.message_type
        );
        return Ok(Json(
            serde_json::json!({"message": "Other message type received"}),
        ));
    }

    let ses_notification: SesNotification =
        serde_json::from_str(&payload.message).map_err(|_| {
            warn!("Non-SES notification received");
            AppError::Validation("Non-SES notification received".to_string())
        })?;

    let request_id = ses_notification
        .mail
        .tags
        .get("request_id")
        .and_then(|tags| tags.first())
        .ok_or_else(|| AppError::Validation("Custom message_id not found in tags".to_string()))?;

    let uuid = Uuid::parse_str(request_id)?;

    sqlx::query!(
        "INSERT INTO email_results (request_id, status, raw, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $4)",
        uuid,
        ses_notification.notification_type,
        serde_json::to_value(&payload.message)?,
        Utc::now()
    )
    .execute(&state.db)
    .await?;

    info!(
        "SES result event saved: request_id={}, notification_type={}",
        request_id, ses_notification.notification_type
    );

    Ok(Json(serde_json::json!({"message": "OK"})))
}

pub async fn get_result_count(
    State(state): State<AppState>,
    Path(topic_id): Path<String>,
) -> Result<Json<ResultCountResponse>> {
    // 토픽이 존재하는지 확인
    let total_requests = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM email_requests WHERE topic_id = $1",
        topic_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    if total_requests == 0 {
        return Ok(Json(ResultCountResponse {
            request: RequestCounts {
                total: 0,
                created: 0,
                sent: 0,
                failed: 0,
                stopped: 0,
            },
            result: ResultCounts {
                statuses: HashMap::new(),
            },
        }));
    }

    // 상태별 요청 수 가져오기
    let request_counts = sqlx::query!(
        "SELECT status, COUNT(*) as count FROM email_requests WHERE topic_id = $1 GROUP BY status",
        topic_id
    )
    .fetch_all(&state.db)
    .await?;

    let mut req_counts = RequestCounts {
        total: total_requests,
        created: 0,
        sent: 0,
        failed: 0,
        stopped: 0,
    };

    for row in request_counts {
        match row.status {
            0 => req_counts.created = row.count.unwrap_or(0),
            1 => {} // Processing - not included in original Go code
            2 => req_counts.sent = row.count.unwrap_or(0),
            3 => req_counts.failed = row.count.unwrap_or(0),
            4 => req_counts.stopped = row.count.unwrap_or(0),
            _ => warn!("Unknown request status: {}", row.status),
        }
    }

    // 결과 수 가져오기
    let result_counts = sqlx::query!(
        "SELECT status, COUNT(DISTINCT request_id) as count 
         FROM email_results 
         WHERE request_id IN (SELECT id FROM email_requests WHERE topic_id = $1)
         GROUP BY status",
        topic_id
    )
    .fetch_all(&state.db)
    .await?;

    let mut statuses = HashMap::new();
    for row in result_counts {
        statuses.insert(row.status, row.count.unwrap_or(0));
    }

    Ok(Json(ResultCountResponse {
        request: req_counts,
        result: ResultCounts { statuses },
    }))
}

#[derive(Deserialize)]
pub struct SentCountQuery {
    hours: Option<i32>,
}

pub async fn get_sent_count(
    State(state): State<AppState>,
    Query(query): Query<SentCountQuery>,
) -> Result<Json<SentCountResponse>> {
    let hours = query.hours.unwrap_or(24);

    if hours <= 0 || hours > 168 {
        return Err(AppError::Validation(
            "hours must be between 1 and 168".to_string(),
        ));
    }

    let start_time = Utc::now() - chrono::Duration::hours(hours as i64);

    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM email_requests 
         WHERE updated_at > $1 AND status = $2",
        start_time,
        EmailStatus::Sent as i16
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    info!(
        "Successfully retrieved sent count: {} (hours: {})",
        count, hours
    );

    Ok(Json(SentCountResponse { count }))
}

pub async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>> {
    // 데이터베이스 연결 테스트
    sqlx::query("SELECT 1").execute(&state.db).await?;

    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: Utc::now(),
    }))
}
