use crate::{
    config::NatsConfig,
    error::{AppError, Result},
    models::email::*,
};
use async_nats::jetstream::{self, stream::Config as StreamConfig};
use serde_json::json;
use std::time::Duration;
use tracing::{error, info};

pub struct ProducerService {
    jetstream: jetstream::Context,
    subject: String,
}

impl ProducerService {
    pub async fn new(config: &NatsConfig) -> Result<Self> {
        info!("Connecting to NATS at {}", config.url);

        let client = async_nats::connect(&config.url)
            .await
            .map_err(|e| AppError::Nats(e.to_string()))?;
        let jetstream = jetstream::new(client);

        // 스트림이 존재하지 않으면 생성
        let stream_config = StreamConfig {
            name: config.stream.clone(),
            subjects: vec![config.subject.clone()],
            max_age: Duration::from_secs(24 * 60 * 60), // 24 hours
            max_messages: 1_000_000,
            max_bytes: 1_000_000_000, // 1GB
            ..Default::default()
        };

        match jetstream.get_or_create_stream(stream_config).await {
            Ok(_stream_info) => {
                info!("NATS stream '{}' ready", config.stream);
            }
            Err(e) => {
                error!("Failed to create NATS stream '{}': {}", config.stream, e);
                return Err(AppError::Nats(e.to_string()));
            }
        }

        Ok(Self {
            jetstream,
            subject: config.subject.clone(),
        })
    }

    pub async fn publish_email(
        &self,
        request: &EmailRequestWithContent,
        server_host: &str,
    ) -> Result<()> {
        let content_with_tracking = request.content_with_tracking(server_host);

        let payload = json!({
            "uuid": request.id,
            "email": request.to_email,
            "subject": request.subject.as_deref().unwrap_or(""),
            "body": content_with_tracking,
            "topic_id": request.topic_id,
            "timestamp": chrono::Utc::now(),
        });

        let payload_bytes = serde_json::to_vec(&payload)?;

        match self
            .jetstream
            .publish(self.subject.clone(), payload_bytes.into())
            .await
        {
            Ok(ack) => {
                let sequence = ack
                    .await
                    .map_err(|e| AppError::Nats(e.to_string()))?
                    .sequence;
                info!(
                    "Message published successfully: request_id={}, stream_seq={}",
                    request.id, sequence
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to publish message for request_id {}: {}",
                    request.id, e
                );
                Err(AppError::Nats(e.to_string()))
            }
        }
    }

    #[allow(dead_code)]
    pub async fn health_check(&self) -> Result<()> {
        // Simple health check by getting stream info
        self.jetstream
            .get_stream(&self.subject)
            .await
            .map_err(|e| AppError::Nats(e.to_string()))?;
        Ok(())
    }
}
