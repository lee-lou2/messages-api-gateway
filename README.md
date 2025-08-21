# Messages API Gateway 🦀

Rust로 구축된 고성능 이메일 API 게이트웨이 서비스로, 확장 가능한 이메일 발송과 고급 트래킹·분석 기능을 제공합니다.

## ✨ 주요 기능

- **🚀 고성능**: Rust 기반으로 최대 처리량과 최소 자원 사용을 지향
- **📧 배치 처리**: 구성 가능한 배치 크기로 대량 이메일 요청을 효율적으로 처리
- **⏰ 예약 발송**: 미래 시점으로 정확한 예약 발송 지원
- **📊 이메일 트래킹**: 오픈/참여 지표 수집을 위한 픽셀 트래킹 지원
- **🔗 AWS SES 연동**: SNS 웹훅을 통한 AWS SES와의 매끄러운 연동
- **🏷️ 토픽별 구성**: 캠페인 관리에 유리한 토픽 단위 그룹화
- **📈 실시간 분석**: 종합 통계와 전송 인사이트 제공
- **🛡️ 타입 안정성**: 컴파일 타임 보장으로 런타임 오류 예방
- **🔒 보안**: 상수 시간 API 키 비교 및 입력 값 검증
- **🔄 우아한 종료**: 적절한 정리 및 커넥션 처리

## 🛠️ 기술 스택

- **언어**: Rust
- **웹 프레임워크**: Axum
- **데이터베이스**: PostgreSQL + SQLx
- **메시지 큐**: NATS JetStream
- **비동기 런타임**: Tokio
- **검증**: 정규식을 활용한 포괄적 입력 검증
- **로깅**: tracing 기반의 구조화 로깅

## 빠른 시작

### 사전 준비물

- Rust 1.75+
- PostgreSQL
- JetStream이 활성화된 NATS 서버

### 설치

1. 레포지토리 클론:
```bash
git clone <repository-url>
cd messages-api-gateway
```

2. 환경 변수 파일 복사:
```bash
cp .env.example .env
```

3. `.env`에 환경 값을 설정:
```env
DATABASE_URL=postgresql://postgres:password@localhost:5432/messages
NATS_URL=nats://127.0.0.1:4222
API_KEY=your-secret-api-key-here
```

4. 데이터베이스 마이그레이션 실행:
```bash
# Install sqlx-cli if not already installed
cargo install sqlx-cli --no-default-features --features postgres

# Run migrations
sqlx migrate run
```

5. 빌드 및 실행:
```bash
# Development
cargo run

# Production build
cargo build --release
./target/release/messages-api-gateway
```

### 도커

```bash
# Build image
docker build -t messages-api-gateway .

# Run container
docker run --env-file .env -p 3000:3000 messages-api-gateway
```

## API 엔드포인트

### 인증
보호된 엔드포인트는 모두 `x-api-key` 헤더에 API 키를 포함해야 합니다.

### 메시지 생성
```http
POST /v1/messages
Content-Type: application/json
x-api-key: your-api-key

{
  "messages": [
    {
      "topic_id": "newsletter-2024",
      "emails": ["user@example.com"],
      "subject": "Welcome!",
      "content": "<h1>Hello World</h1>",
      "scheduled_at": "2024-12-25T10:00:00"
    }
  ]
}
```

### 토픽 통계 조회
```http
GET /v1/topics/{topicId}
x-api-key: your-api-key
```

### 발송 수 조회
```http
GET /v1/events/counts/sent?hours=24
x-api-key: your-api-key
```

### 상태 점검
```http
GET /health
```

## 구성

| 환경 변수 | 기본값 | 설명 |
|---------------------|---------|-------------|
| `SERVER_PORT` | `3000` | HTTP 서버 포트 |
| `DATABASE_URL` | - | PostgreSQL 연결 문자열 |
| `NATS_URL` | `nats://127.0.0.1:4222` | NATS 서버 URL |
| `API_KEY` | - | API 인증 키 |
| `SERVER_HOST` | `http://localhost:3000` | 트래킹 픽셀용 서버 호스트 |
| `BATCH_SIZE` | `1000` | 이메일 처리 배치 크기 |
| `SCHEDULER_INTERVAL` | `60` | 스케줄러 실행 주기(초) |
| `NATS_STREAM` | `messages` | NATS 스트림 이름 |
| `NATS_SUBJECT` | `messages.email` | 이메일 메시지용 NATS 서브젝트 |
| `RUST_LOG` | `info` | 로그 레벨 (error, warn, info, debug, trace) |

## 아키텍처

본 서비스는 프로듀서-컨슈머 패턴을 따릅니다:

1. **HTTP API**가 이메일 요청을 수신하고 PostgreSQL에 저장
2. **백그라운드 스케줄러**가 대기 중 이메일을 처리하여 NATS로 퍼블리시
3. **외부 이메일 발송기**가 NATS에서 소비하여 AWS SES로 전송
4. **전송 결과**는 SNS 웹훅으로 수신되어 분석용으로 저장

## 개발

### 테스트 실행
```bash
cargo test
```

### 데이터베이스 마이그레이션
```bash
# Create new migration
sqlx migrate add <migration_name>

# Run migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert
```

### 로깅
로그 레벨은 `RUST_LOG` 환경 변수로 제어합니다:
```bash
RUST_LOG=debug cargo run
```

## Go에서의 마이그레이션

이 Rust 버전은 기존 Go 구현과의 API 호환성을 유지하면서 다음 이점을 제공합니다:

- **더 나은 성능**: Rust의 제로-코스트 추상화와 메모리 안전성
- **타입 안정성**: 컴파일 타임 보장으로 많은 런타임 오류 예방
- **모던 비동기**: 효율적 비동기 I/O를 위한 Tokio 기반
- **SQLx 통합**: 컴파일 타임 검증되는 SQL 쿼리
