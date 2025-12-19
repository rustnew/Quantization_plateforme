-- migrations/0001_initial.sql

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255),
    stripe_customer_id VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    
    CONSTRAINT users_email_check CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$')
);

-- API Keys table
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(100) NOT NULL,
    permissions JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    
    INDEX idx_api_keys_user_id (user_id),
    INDEX idx_api_keys_key (key)
);

-- Model files table
CREATE TABLE model_files (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    original_filename VARCHAR(255) NOT NULL,
    storage_filename VARCHAR(255) NOT NULL,
    file_size BIGINT NOT NULL,
    checksum_sha256 VARCHAR(64) NOT NULL,
    format VARCHAR(50) NOT NULL,
    model_type VARCHAR(100),
    architecture VARCHAR(100),
    parameter_count DECIMAL(10, 2),
    storage_bucket VARCHAR(255) NOT NULL,
    storage_path VARCHAR(1024) NOT NULL,
    download_token VARCHAR(64),
    download_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    
    INDEX idx_model_files_user_id (user_id),
    INDEX idx_model_files_checksum (checksum_sha256),
    INDEX idx_model_files_expires (expires_at)
);

-- Job status enum
CREATE TYPE job_status AS ENUM (
    'pending',
    'processing', 
    'completed',
    'failed',
    'cancelled'
);

-- Quantization method enum
CREATE TYPE quantization_method AS ENUM (
    'int8',
    'gptq',
    'awq',
    'gguf_q4_0',
    'gguf_q5_0'
);

-- Model format enum
CREATE TYPE model_format AS ENUM (
    'pytorch',
    'safetensors',
    'onnx',
    'gguf'
);

-- Jobs table
CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    status job_status NOT NULL DEFAULT 'pending',
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress >= 0 AND progress <= 100),
    quantization_method quantization_method NOT NULL,
    input_format model_format NOT NULL,
    output_format model_format NOT NULL,
    input_file_id UUID NOT NULL REFERENCES model_files(id),
    output_file_id UUID REFERENCES model_files(id),
    error_message TEXT,
    original_size BIGINT,
    quantized_size BIGINT,
    processing_time INTEGER,
    credits_used INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    
    INDEX idx_jobs_user_id (user_id),
    INDEX idx_jobs_status (status),
    INDEX idx_jobs_created_at (created_at),
    INDEX idx_jobs_input_file (input_file_id),
    INDEX idx_jobs_output_file (output_file_id)
);

-- Subscription plan enum
CREATE TYPE subscription_plan AS ENUM (
    'free',
    'starter',
    'pro'
);

-- Subscription status enum
CREATE TYPE subscription_status AS ENUM (
    'active',
    'past_due',
    'cancelled',
    'trialing'
);

-- Subscriptions table
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID UNIQUE NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan subscription_plan NOT NULL DEFAULT 'free',
    status subscription_status NOT NULL DEFAULT 'active',
    current_period_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    current_period_end TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '30 days',
    stripe_subscription_id VARCHAR(255),
    stripe_price_id VARCHAR(255),
    cancelled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    INDEX idx_subscriptions_user_id (user_id),
    INDEX idx_subscriptions_status (status)
);

-- Credit transactions table
CREATE TABLE credit_transactions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    transaction_type VARCHAR(50) NOT NULL,
    amount INTEGER NOT NULL,
    balance_after INTEGER NOT NULL,
    job_id UUID REFERENCES jobs(id),
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    INDEX idx_credit_transactions_user_id (user_id),
    INDEX idx_credit_transactions_created_at (created_at)
);

-- Audit logs table
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id),
    api_key_id VARCHAR(64),
    ip_address INET,
    user_agent TEXT,
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(50),
    resource_id UUID,
    old_values JSONB,
    new_values JSONB,
    message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    INDEX idx_audit_logs_user_id (user_id),
    INDEX idx_audit_logs_action (action),
    INDEX idx_audit_logs_created_at (created_at DESC)
);

-- System metrics table
CREATE TABLE system_metrics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    active_users BIGINT NOT NULL DEFAULT 0,
    total_jobs BIGINT NOT NULL DEFAULT 0,
    jobs_pending BIGINT NOT NULL DEFAULT 0,
    jobs_processing BIGINT NOT NULL DEFAULT 0,
    jobs_completed BIGINT NOT NULL DEFAULT 0,
    jobs_failed BIGINT NOT NULL DEFAULT 0,
    queue_size BIGINT NOT NULL DEFAULT 0,
    memory_usage_mb DECIMAL(10, 2) NOT NULL DEFAULT 0,
    cpu_usage_percent DECIMAL(5, 2) NOT NULL DEFAULT 0,
    total_storage_gb DECIMAL(10, 2) NOT NULL DEFAULT 0,
    used_storage_gb DECIMAL(10, 2) NOT NULL DEFAULT 0,
    
    INDEX idx_system_metrics_timestamp (timestamp DESC)
);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers for updated_at
CREATE TRIGGER update_jobs_updated_at 
    BEFORE UPDATE ON jobs 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_subscriptions_updated_at 
    BEFORE UPDATE ON subscriptions 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Function to cleanup expired files
CREATE OR REPLACE FUNCTION cleanup_expired_files()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM model_files 
    WHERE expires_at IS NOT NULL 
    AND expires_at < NOW()
    RETURNING COUNT(*) INTO deleted_count;
    
    RETURN deleted_count;
END;
$$ language 'plpgsql';

-- Create admin user (password will be set by application)
INSERT INTO users (id, email, password_hash, created_at) 
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'admin@quantization.io',
    NULL,
    NOW()
) ON CONFLICT DO NOTHING;

-- Create free subscription for admin
INSERT INTO subscriptions (id, user_id, plan, status, created_at, updated_at)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    '00000000-0000-0000-0000-000000000000',
    'free',
    'active',
    NOW(),
    NOW()
) ON CONFLICT DO NOTHING;