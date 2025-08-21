use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[repr(i16)]
pub enum EmailStatus {
    Created = 0,
    Processing = 1,
    Sent = 2,
    Failed = 3,
    Stopped = 4,
}

impl fmt::Display for EmailStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmailStatus::Created => write!(f, "created"),
            EmailStatus::Processing => write!(f, "processing"),
            EmailStatus::Sent => write!(f, "sent"),
            EmailStatus::Failed => write!(f, "failed"),
            EmailStatus::Stopped => write!(f, "stopped"),
        }
    }
}

impl EmailStatus {
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            EmailStatus::Sent | EmailStatus::Failed | EmailStatus::Stopped
        )
    }

    #[allow(dead_code)]
    pub fn can_transition_to(&self, new_status: EmailStatus) -> bool {
        use EmailStatus::*;
        matches!(
            (self, new_status),
            (Created, Processing) | (Processing, Sent | Failed) | (Created | Processing, Stopped)
        )
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailContent {
    pub id: i32,
    pub subject: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EmailContent {
    #[allow(dead_code)]
    pub fn new(subject: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: 0, // 데이터베이스에서 설정됨
            subject: subject.trim().to_string(),
            content: content.trim().to_string(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailRequest {
    pub id: Uuid,
    pub topic_id: String,
    pub to_email: String,
    pub content_id: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub status: EmailStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EmailRequest {
    #[allow(dead_code)]
    pub fn new(
        topic_id: String,
        to_email: String,
        content_id: i32,
        scheduled_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            topic_id,
            to_email: to_email.trim().to_string(),
            content_id,
            scheduled_at,
            status: EmailStatus::Created,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[allow(dead_code)]
    pub fn update_status(&mut self, status: EmailStatus, error: Option<String>) {
        self.status = status;
        self.error = error;
        self.updated_at = Utc::now();
    }

    #[allow(dead_code)]
    pub fn is_ready_to_send(&self, now: DateTime<Utc>) -> bool {
        self.status == EmailStatus::Created
            && self.scheduled_at.is_none_or(|scheduled| scheduled <= now)
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailResult {
    pub id: i32,
    pub request_id: Uuid,
    pub status: String,
    pub raw: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EmailResult {
    #[allow(dead_code)]
    pub fn new(request_id: Uuid, status: String, raw: serde_json::Value) -> Self {
        let now = Utc::now();
        Self {
            id: 0, // 데이터베이스에서 설정됨
            request_id,
            status,
            raw,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmailRequestWithContent {
    pub id: Uuid,
    pub topic_id: String,
    pub to_email: String,
    pub content_id: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub status: EmailStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub subject: Option<String>,
    pub content: Option<String>,
}

impl EmailRequestWithContent {
    pub fn generate_tracking_pixel(&self, server_host: &str) -> String {
        format!(
            r#"<img src="{}/v1/events/open?requestId={}" width="1" height="1" style="display:none;" alt="">"#,
            server_host, self.id
        )
    }

    pub fn content_with_tracking(&self, server_host: &str) -> String {
        let content = self.content.as_deref().unwrap_or("");
        format!("{}{}", content, self.generate_tracking_pixel(server_host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_email_status_display() {
        // EmailStatus의 Display trait 구현 테스트
        assert_eq!(EmailStatus::Created.to_string(), "created");
        assert_eq!(EmailStatus::Processing.to_string(), "processing");
        assert_eq!(EmailStatus::Sent.to_string(), "sent");
        assert_eq!(EmailStatus::Failed.to_string(), "failed");
        assert_eq!(EmailStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_email_status_transitions() {
        // EmailStatus의 상태 전이 가능 여부 테스트
        // Created 상태에서 가능한 전이
        assert!(EmailStatus::Created.can_transition_to(EmailStatus::Processing));
        assert!(EmailStatus::Created.can_transition_to(EmailStatus::Stopped));
        assert!(!EmailStatus::Created.can_transition_to(EmailStatus::Sent));
        assert!(!EmailStatus::Created.can_transition_to(EmailStatus::Failed));

        // Processing 상태에서 가능한 전이
        assert!(EmailStatus::Processing.can_transition_to(EmailStatus::Sent));
        assert!(EmailStatus::Processing.can_transition_to(EmailStatus::Failed));
        assert!(EmailStatus::Processing.can_transition_to(EmailStatus::Stopped));
        assert!(!EmailStatus::Processing.can_transition_to(EmailStatus::Created));
        assert!(!EmailStatus::Processing.can_transition_to(EmailStatus::Processing));

        // Terminal 상태 테스트
        assert!(EmailStatus::Sent.is_terminal());
        assert!(EmailStatus::Failed.is_terminal());
        assert!(EmailStatus::Stopped.is_terminal());
        assert!(!EmailStatus::Created.is_terminal());
        assert!(!EmailStatus::Processing.is_terminal());
    }

    #[test]
    fn test_email_request_creation() {
        // EmailRequest 생성 테스트
        let topic_id = "test-topic".to_string();
        let to_email = "test@example.com".to_string();
        let content_id = 1;
        let scheduled_at = None;

        let request =
            EmailRequest::new(topic_id.clone(), to_email.clone(), content_id, scheduled_at);

        assert_eq!(request.topic_id, topic_id);
        assert_eq!(request.to_email, to_email);
        assert_eq!(request.content_id, content_id);
        assert_eq!(request.scheduled_at, scheduled_at);
        assert_eq!(request.status, EmailStatus::Created);
        assert_eq!(request.error, None);
    }

    #[test]
    fn test_email_request_status_update() {
        // EmailRequest 상태 업데이트 테스트
        let mut request = EmailRequest::new(
            "test-topic".to_string(),
            "test@example.com".to_string(),
            1,
            None,
        );

        let initial_updated_at = request.updated_at;

        // 상태 업데이트
        request.update_status(EmailStatus::Processing, None);

        assert_eq!(request.status, EmailStatus::Processing);
        assert_eq!(request.error, None);
        assert!(request.updated_at > initial_updated_at);

        // 에러와 함께 상태 업데이트
        request.update_status(EmailStatus::Failed, Some("Connection timeout".to_string()));

        assert_eq!(request.status, EmailStatus::Failed);
        assert_eq!(request.error, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_email_request_ready_to_send() {
        // EmailRequest 발송 준비 여부 테스트
        let now = Utc::now();

        // 스케줄이 없는 요청 - 바로 발송 가능해야 함
        let request_without_schedule = EmailRequest::new(
            "test-topic".to_string(),
            "test@example.com".to_string(),
            1,
            None,
        );
        assert!(request_without_schedule.is_ready_to_send(now));

        // 과거에 스케줄된 요청 - 발송 가능해야 함
        let request_past_schedule = EmailRequest::new(
            "test-topic".to_string(),
            "test@example.com".to_string(),
            1,
            Some(now - Duration::hours(1)),
        );
        assert!(request_past_schedule.is_ready_to_send(now));

        // 미래에 스케줄된 요청 - 발송 불가능해야 함
        let request_future_schedule = EmailRequest::new(
            "test-topic".to_string(),
            "test@example.com".to_string(),
            1,
            Some(now + Duration::hours(1)),
        );
        assert!(!request_future_schedule.is_ready_to_send(now));

        // Processing 상태의 요청 - 발송 불가능해야 함
        let mut request_processing = EmailRequest::new(
            "test-topic".to_string(),
            "test@example.com".to_string(),
            1,
            None,
        );
        request_processing.update_status(EmailStatus::Processing, None);
        assert!(!request_processing.is_ready_to_send(now));
    }

    #[test]
    fn test_email_request_with_content_tracking_pixel() {
        // EmailRequestWithContent의 추적 픽셀 생성 테스트
        let request = EmailRequestWithContent {
            id: uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            topic_id: "test-topic".to_string(),
            to_email: "test@example.com".to_string(),
            content_id: 1,
            scheduled_at: None,
            status: EmailStatus::Created,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            subject: Some("Test Subject".to_string()),
            content: Some("Test Content".to_string()),
        };

        let server_host = "http://localhost:3000";
        let tracking_pixel = request.generate_tracking_pixel(server_host);

        // 추적 픽셀의 형식이 올바른지 확인
        assert!(tracking_pixel
            .starts_with("<img src=\"http://localhost:3000/v1/events/open?requestId="));
        assert!(tracking_pixel.contains("123e4567-e89b-12d3-a456-426614174000"));
        assert!(tracking_pixel
            .ends_with("\" width=\"1\" height=\"1\" style=\"display:none;\" alt=\"\">"));
    }

    #[test]
    fn test_email_request_with_content_tracking() {
        // EmailRequestWithContent의 추적 픽셀 포함 내용 생성 테스트
        let request = EmailRequestWithContent {
            id: uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            topic_id: "test-topic".to_string(),
            to_email: "test@example.com".to_string(),
            content_id: 1,
            scheduled_at: None,
            status: EmailStatus::Created,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            subject: Some("Test Subject".to_string()),
            content: Some("Test Content".to_string()),
        };

        let server_host = "http://localhost:3000";
        let content_with_tracking = request.content_with_tracking(server_host);

        // 원본 내용과 추적 픽셀이 모두 포함되어 있는지 확인
        assert!(content_with_tracking.starts_with("Test Content"));
        assert!(content_with_tracking
            .contains("<img src=\"http://localhost:3000/v1/events/open?requestId="));
        assert!(content_with_tracking.contains("123e4567-e89b-12d3-a456-426614174000"));
    }

    #[test]
    fn test_email_request_with_content_tracking_empty_content() {
        // EmailRequestWithContent의 추적 픽셀 포함 내용 생성 테스트 (빈 내용)
        let request = EmailRequestWithContent {
            id: uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            topic_id: "test-topic".to_string(),
            to_email: "test@example.com".to_string(),
            content_id: 1,
            scheduled_at: None,
            status: EmailStatus::Created,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            subject: Some("Test Subject".to_string()),
            content: Some("".to_string()),
        };

        let server_host = "http://localhost:3000";
        let content_with_tracking = request.content_with_tracking(server_host);

        // 빈 내용에 추적 픽셀만 추가되는지 확인
        assert!(content_with_tracking
            .starts_with("<img src=\"http://localhost:3000/v1/events/open?requestId="));
        assert!(content_with_tracking.contains("123e4567-e89b-12d3-a456-426614174000"));
    }

    #[test]
    fn test_email_request_with_content_tracking_none_content() {
        // EmailRequestWithContent의 추적 픽셀 포함 내용 생성 테스트 (내용이 None)
        let request = EmailRequestWithContent {
            id: uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            topic_id: "test-topic".to_string(),
            to_email: "test@example.com".to_string(),
            content_id: 1,
            scheduled_at: None,
            status: EmailStatus::Created,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            subject: Some("Test Subject".to_string()),
            content: None,
        };

        let server_host = "http://localhost:3000";
        let content_with_tracking = request.content_with_tracking(server_host);

        // 내용이 없을 때 추적 픽셀만 생성되는지 확인
        assert!(content_with_tracking
            .starts_with("<img src=\"http://localhost:3000/v1/events/open?requestId="));
        assert!(content_with_tracking.contains("123e4567-e89b-12d3-a456-426614174000"));
    }
}
