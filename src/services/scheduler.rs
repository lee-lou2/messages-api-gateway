use crate::{
    config::AppConfig,
    error::{AppError, Result},
    models::email::*,
    services::producer::ProducerService,
};
use chrono::Utc;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::time;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct SchedulerService {
    db: PgPool,
    producer: Arc<ProducerService>,
    config: Arc<AppConfig>,
}

impl SchedulerService {
    pub fn new(db: PgPool, producer: Arc<ProducerService>, config: Arc<AppConfig>) -> Self {
        Self {
            db,
            producer,
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let mut interval = time::interval(Duration::from_secs(self.config.scheduler.interval_secs));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        info!(
            "📧 Email scheduler started: batch_size={}, interval={}s",
            self.config.scheduler.batch_size, self.config.scheduler.interval_secs
        );

        loop {
            interval.tick().await;

            let start = std::time::Instant::now();
            match self.process_scheduled_emails().await {
                Ok(processed) => {
                    if processed > 0 {
                        info!(
                            "📧 Scheduler cycle completed: processed={}, duration={:?}",
                            processed,
                            start.elapsed()
                        );
                    } else {
                        debug!("📧 Scheduler cycle completed: no emails to process");
                    }
                }
                Err(e) => {
                    error!("📧 Scheduler cycle failed: {:#}", e);
                }
            }
        }
    }

    async fn process_scheduled_emails(&self) -> Result<usize> {
        let mut total_processed = 0;

        loop {
            let batch_start = std::time::Instant::now();
            let now = Utc::now();

            // FOR UPDATE SKIP LOCKED를 사용하여 요청을 원자적으로 가져오고 업데이트
            let requests = sqlx::query_as!(
                EmailRequestWithContent,
                r#"
                WITH locked_requests AS (
                    SELECT er.id
                    FROM email_requests er
                    WHERE er.status = $1 
                      AND (er.scheduled_at <= $2 OR er.scheduled_at IS NULL)
                    ORDER BY 
                        CASE WHEN er.scheduled_at IS NULL THEN 0 ELSE 1 END,
                        er.scheduled_at ASC NULLS FIRST,
                        er.created_at ASC
                    LIMIT $3
                    FOR UPDATE SKIP LOCKED
                )
                UPDATE email_requests
                SET status = $4, updated_at = $5
                FROM locked_requests lr
                WHERE email_requests.id = lr.id
                RETURNING 
                    email_requests.id,
                    email_requests.topic_id,
                    email_requests.to_email,
                    email_requests.content_id,
                    email_requests.scheduled_at,
                    email_requests.status as "status: EmailStatus",
                    email_requests.error,
                    email_requests.created_at,
                    email_requests.updated_at,
                    (SELECT ec.subject FROM email_contents ec WHERE ec.id = email_requests.content_id) as subject,
                    (SELECT ec.content FROM email_contents ec WHERE ec.id = email_requests.content_id) as content
                "#,
                EmailStatus::Created as i16,
                now,
                self.config.scheduler.batch_size as i32,
                EmailStatus::Processing as i16,
                now
            )
            .fetch_all(&self.db)
            .await?;

            if requests.is_empty() {
                break;
            }

            debug!("📧 Processing email batch of size: {}", requests.len());

            // 제한된 동시성을 사용하여 요청을 병렬로 처리
            let semaphore = Arc::new(tokio::sync::Semaphore::new(10)); // Max 10 concurrent
            let mut tasks = Vec::new();

            for request in requests {
                let producer = self.producer.clone();
                let server_host = self.config.server.host.clone();
                let permit = semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|e| AppError::Semaphore(e.to_string()))?;

                let task = tokio::spawn(async move {
                    let _permit = permit; // Keep permit until task completes
                    let result = producer.publish_email(&request, &server_host).await;
                    (request.id, result)
                });

                tasks.push(task);
            }

            // 결과 수집
            let mut updates = Vec::new();
            let mut success_count = 0;

            for task in tasks {
                match task.await {
                    Ok((request_id, Ok(_))) => {
                        success_count += 1;
                        updates.push((request_id, EmailStatus::Sent, None));
                    }
                    Ok((request_id, Err(e))) => {
                        warn!(
                            "📧 Failed to publish email for request {}: {}",
                            request_id, e
                        );
                        updates.push((request_id, EmailStatus::Failed, Some(e.to_string())));
                    }
                    Err(e) => {
                        error!("📧 Task panicked: {}", e);
                        // We can't identify the specific request, so we'll let the database
                        // timeout handle the stuck "Processing" status
                    }
                }
            }

            // 상태 일괄 업데이트
            let batch_count = updates.len();
            if !updates.is_empty() {
                self.bulk_update_requests(updates).await?;
            }
            let success_rate = if batch_count > 0 {
                (success_count as f64 / batch_count as f64) * 100.0
            } else {
                0.0
            };

            info!(
                "📧 Batch processed: success={}, failed={}, rate={:.1}%, duration={:?}",
                success_count,
                batch_count - success_count,
                success_rate,
                batch_start.elapsed()
            );

            total_processed += batch_count;

            // Small delay between batches to prevent overwhelming the system
            if batch_count == self.config.scheduler.batch_size {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(total_processed)
    }

    async fn bulk_update_requests(
        &self,
        updates: Vec<(uuid::Uuid, EmailStatus, Option<String>)>,
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let update_start = std::time::Instant::now();
        let mut tx = self.db.begin().await?;

        // 보다 효율적인 일괄 업데이트 접근 방식 사용
        let now = Utc::now();
        let mut success_count = 0;

        for (id, status, error) in updates {
            let rows_affected = sqlx::query!(
                "UPDATE email_requests SET status = $1, error = $2, updated_at = $3 WHERE id = $4",
                status as i16,
                error,
                now,
                id
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();

            if rows_affected > 0 {
                success_count += 1;
            } else {
                warn!("📧 Failed to update request {}: not found", id);
            }
        }

        tx.commit().await?;

        debug!(
            "📧 Bulk update completed: updated={}, duration={:?}",
            success_count,
            update_start.elapsed()
        );

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_stats(&self) -> Result<SchedulerStats> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                status,
                COUNT(*) as count
            FROM email_requests 
            WHERE created_at > NOW() - INTERVAL '24 hours'
            GROUP BY status
            "#
        )
        .fetch_all(&self.db)
        .await?;

        let mut scheduler_stats = SchedulerStats::default();

        for row in stats {
            match row.status {
                0 => scheduler_stats.created = row.count.unwrap_or(0) as usize,
                1 => scheduler_stats.processing = row.count.unwrap_or(0) as usize,
                2 => scheduler_stats.sent = row.count.unwrap_or(0) as usize,
                3 => scheduler_stats.failed = row.count.unwrap_or(0) as usize,
                4 => scheduler_stats.stopped = row.count.unwrap_or(0) as usize,
                _ => {}
            }
        }

        Ok(scheduler_stats)
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct SchedulerStats {
    pub created: usize,
    pub processing: usize,
    pub sent: usize,
    pub failed: usize,
    pub stopped: usize,
}

impl SchedulerStats {
    #[allow(dead_code)]
    pub fn total(&self) -> usize {
        self.created + self.processing + self.sent + self.failed + self.stopped
    }

    #[allow(dead_code)]
    pub fn success_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.sent as f64 / total as f64) * 100.0
        }
    }
}
