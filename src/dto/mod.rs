use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Asia::Seoul;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMessageRequest {
    #[validate(length(min = 1, max = 100, message = "Must have between 1 and 100 messages"))]
    pub messages: Vec<MessageRequest>,
}

// Deserialize Option<DateTime<Utc>> where input may be:
// - null => None
// - RFC3339/ISO8601 with offset (e.g., "2025-01-01T10:00:00+09:00", "2025-01-01T01:00:00Z")
// - naive "YYYY-MM-DD HH:MM:SS[.f]" treated as Asia/Seoul local time
fn deserialize_kst_naive_to_utc_opt<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Accept either a string or null
    let opt = Option::<String>::deserialize(deserializer)?;
    let Some(s) = opt else { return Ok(None) };

    // Try RFC3339 first
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }

    // Try parsing with explicit numeric offset like "+0900" without colon
    if let Ok(dt) = DateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%:z")
        .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%:z"))
        .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%:z"))
    {
        return Ok(Some(dt.with_timezone(&Utc)));
    }

    // Fallback: naive string => interpret as Asia/Seoul
    let naive = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f"))
        .map_err(|e| serde::de::Error::custom(format!("invalid datetime format: {}", e)))?;

    // Localize to Asia/Seoul; use single() to avoid ambiguous times (Seoul does not observe DST)
    let local_dt = Seoul.from_local_datetime(&naive).single().ok_or_else(|| {
        serde::de::Error::custom("ambiguous or nonexistent local time in Asia/Seoul")
    })?;

    Ok(Some(local_dt.with_timezone(&Utc)))
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
    #[serde(default)]
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

    #[serde(default, deserialize_with = "deserialize_kst_naive_to_utc_opt")]
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
