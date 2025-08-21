use crate::{api::handlers::AppState, error::AppError};
use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let expected_key = &state.config.security.api_key;

    // 타이밍 공격을 방지하기 위해 상수 시간 비교 사용
    if api_key.len() == expected_key.len()
        && api_key.as_bytes().ct_eq(expected_key.as_bytes()).into()
    {
        Ok(next.run(request).await)
    } else {
        tracing::warn!("🔒 권한 없는 API 접근 시도");
        Err(AppError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtle::Choice;

    #[test]
    fn test_constant_time_comparison() {
        // 상수 시간 비교 함수 테스트
        let key1 = "test-key-123";
        let key2 = "test-key-123";
        let key3 = "test-key-124";

        // 같은 키는 true 반환
        let choice: Choice = key1.as_bytes().ct_eq(key2.as_bytes());
        assert!(bool::from(choice));

        // 다른 키는 false 반환
        let choice: Choice = key1.as_bytes().ct_eq(key3.as_bytes());
        assert!(!bool::from(choice));

        // 길이가 다른 키는 false 반환
        let choice: Choice = key1.as_bytes().ct_eq("short".as_bytes());
        assert!(!bool::from(choice));

        let choice: Choice = key1.as_bytes().ct_eq("very-long-key-456".as_bytes());
        assert!(!bool::from(choice));
    }
}
