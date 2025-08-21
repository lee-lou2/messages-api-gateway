-- Create email_contents table
CREATE TABLE email_contents (
    id SERIAL PRIMARY KEY,
    subject VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create email_requests table
CREATE TABLE email_requests (
    id UUID PRIMARY KEY,
    topic_id VARCHAR(50) NOT NULL DEFAULT '',
    to_email VARCHAR(255) NOT NULL,
    content_id INTEGER NOT NULL REFERENCES email_contents(id),
    scheduled_at TIMESTAMPTZ,
    status SMALLINT NOT NULL DEFAULT 0,
    error VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create email_results table
CREATE TABLE email_results (
    id SERIAL PRIMARY KEY,
    request_id UUID NOT NULL REFERENCES email_requests(id),
    status VARCHAR(50) NOT NULL,
    raw JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for performance optimization
CREATE INDEX IF NOT EXISTS idx_email_requests_status_scheduled 
ON email_requests(status, scheduled_at NULLS FIRST, created_at) 
WHERE status = 0;

CREATE INDEX IF NOT EXISTS idx_email_requests_topic_status 
ON email_requests(topic_id, status);

CREATE INDEX IF NOT EXISTS idx_email_requests_updated_status 
ON email_requests(updated_at, status)
WHERE status IN (2, 3); -- sent, failed

CREATE INDEX IF NOT EXISTS idx_email_results_request_status 
ON email_results(request_id, status);

CREATE INDEX IF NOT EXISTS idx_email_requests_topic_id 
ON email_requests(topic_id);

CREATE INDEX IF NOT EXISTS idx_email_requests_content_id 
ON email_requests(content_id);

-- Add unique constraint to prevent duplicate open events
CREATE UNIQUE INDEX IF NOT EXISTS idx_email_results_request_status_unique
ON email_results(request_id, status)
WHERE status = 'Open';

-- Add partial index for processing status to help with stuck request cleanup
CREATE INDEX IF NOT EXISTS idx_email_requests_processing_timeout
ON email_requests(updated_at)
WHERE status = 1; -- processing