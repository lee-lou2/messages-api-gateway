use chrono::{DateTime, Utc};
use serde::{self, de::Error, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMessageRequest {
    #[validate(length(min = 1, max = 100, message = "Must have between 1 and 100 messages"))]
    pub messages: Vec<MessageRequest>,
}

// Deserialize Option<DateTime<Utc>>
fn deserialize_rfc3339_to_utc_opt<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    // JSON 값이 null이면 None, 문자열이면 Some(String)으로 받음
    let opt_s = Option::<String>::deserialize(deserializer)?;
    let Some(s) = opt_s else {
        return Ok(None); // null 값은 그대로 None으로 처리
    };

    // 문자열 s를 RFC3339 형식으로 파싱 시도
    match DateTime::parse_from_rfc3339(&s) {
        // 파싱 성공 시, UTC 시간으로 변환하여 반환
        Ok(dt) => Ok(Some(dt.with_timezone(&Utc))),
        // 파싱 실패 시, 어떤 형식을 기대했는지 명확하게 에러 메시지 반환
        Err(e) => Err(Error::custom(format!(
            "Invalid RFC3339 format. Expected 'YYYY-MM-DDTHH:MM:SSZ' or similar, but got '{}'. Error: {}",
            s, e
        ))),
    }
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct MessageRequest {
    #[validate(length(
        min = 0,
        max = 50,
        message = "Topic ID must be between 0 and 50 characters"
    ))]
    #[validate(regex(
        path = "TOPIC_ID_REGEX",
        message = "Topic ID must contain only alphanumeric characters, hyphens, and underscores"
    ))]
    #[serde(default, rename = "topicId")]
    pub topic_id: Option<String>,

    #[validate(length(min = 1, max = 1000, message = "Must have between 1 and 1000 emails"))]
    #[validate(custom = "validate_emails")]
    pub emails: Vec<String>,

    #[validate(length(
        min = 1,
        max = 255,
        message = "Subject must be between 1 and 255 characters"
    ))]
    pub subject: String,

    #[validate(length(
        min = 1,
        max = 65535,
        message = "Content must be between 1 and 65535 characters"
    ))]
    pub content: String,

    #[serde(
        default,
        deserialize_with = "deserialize_rfc3339_to_utc_opt",
        rename = "scheduledAt"
    )]
    pub scheduled_at: Option<DateTime<Utc>>,
}

lazy_static::lazy_static! {
    static ref TOPIC_ID_REGEX: regex::Regex = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    static ref EMAIL_REGEX: regex::Regex = regex::Regex::new(
        r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
    ).unwrap();
}

fn validate_emails(emails: &[String]) -> Result<(), ValidationError> {
    for email in emails.iter() {
        let trimmed = email.trim();
        if trimmed.is_empty() {
            return Err(ValidationError::new("email_empty"));
        }
        if trimmed.len() > 254 {
            return Err(ValidationError::new("email_too_long"));
        }
        if !EMAIL_REGEX.is_match(trimmed) {
            return Err(ValidationError::new("email_invalid_format"));
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct CreateMessageResponse {
    pub count: usize,
    pub elapsed: String,
}

#[derive(Debug, Serialize)]
pub struct ResultCountResponse {
    pub request: RequestCounts,
    pub result: ResultCounts,
}

#[derive(Debug, Serialize)]
pub struct RequestCounts {
    pub total: i64,
    pub created: i64,
    pub sent: i64,
    pub failed: i64,
    pub stopped: i64,
}

#[derive(Debug, Serialize)]
pub struct ResultCounts {
    pub statuses: HashMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct SentCountResponse {
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SnsMessage {
    #[serde(rename = "Type")]
    pub message_type: String,

    #[serde(rename = "Message")]
    pub message: String,

    #[serde(rename = "MessageId")]
    #[allow(dead_code)]
    pub message_id: String,

    #[serde(rename = "SubscribeURL")]
    pub subscribe_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SesNotification {
    #[serde(rename = "notificationType")]
    pub notification_type: String,

    pub mail: SesMailInfo,
}

#[derive(Debug, Deserialize)]
pub struct SesMailInfo {
    pub tags: HashMap<String, Vec<String>>,
}
