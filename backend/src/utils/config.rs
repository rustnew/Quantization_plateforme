// utils/config.rs
use crate::utils::error::{AppError, Result};
use dotenv::dotenv;
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // Environnement et serveur
    pub run_mode: String,
    pub server_host: String,
    pub server_port: u16,
    pub workers: usize,
    pub log_level: String,
    
    // Base de données
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_min_connections: u32,
    pub database_connection_timeout: u32,
    
    // Sécurité
    pub jwt_secret: String,
    pub jwt_access_token_expiry_hours: i64,
    pub jwt_refresh_token_expiry_days: i64,
    pub admin_email: String,
    pub admin_password: String,
    pub password_reset_token_expiry_hours: i64,
    pub api_key_expiry_days: i64,
    
    // Chiffrement
    pub storage_encryption_key: String,
    pub encryption_algorithm: String,
    pub encryption_nonce_size: usize,
    
    // Redis
    pub redis_url: String,
    pub redis_pool_size: u32,
    pub redis_connection_timeout: u64,
    pub redis_queue_prefix: String,
    pub redis_cache_ttl_seconds: u64,
    
    // MinIO/S3
    pub storage_type: String,
    pub minio_endpoint: Option<String>,
    pub minio_access_key: Option<String>,
    pub minio_secret_key: Option<String>,
    pub minio_bucket: String,
    pub minio_region: String,
    pub minio_secure: bool,
    pub minio_connection_timeout: u64,
    pub max_file_size_mb: u64,
    
    // Quantification
    pub quantization_python_path: String,
    pub quantization_max_concurrent_jobs: usize,
    pub quantization_timeout_seconds: u64,
    pub quantization_max_retries: u32,
    pub quantization_gpu_enabled: bool,
    
    // Google OAuth
    pub google_oauth_client_id: Option<String>,
    pub google_oauth_client_secret: Option<String>,
    pub google_oauth_redirect_uri: Option<String>,
    
    // Stripe
    pub stripe_secret_key: Option<String>,
    pub stripe_publishable_key: Option<String>,
    pub stripe_webhook_secret: Option<String>,
    pub stripe_currency: String,
    pub stripe_trial_period_days: i64,
    pub stripe_price_starter: Option<String>,
    pub stripe_price_pro: Option<String>,
    
    // Email
    pub email_provider: String,
    pub email_from: String,
    pub email_from_name: String,
    pub sendgrid_api_key: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_tls: bool,
    
    // Limites et quotas
    pub free_user_credits_per_month: i32,
    pub free_user_max_file_size_mb: u64,
    pub free_user_file_retention_days: i32,
    pub free_user_queue_priority: String,
    
    pub starter_user_credits_per_month: i32,
    pub starter_user_max_file_size_mb: u64,
    pub starter_user_file_retention_days: i32,
    pub starter_user_queue_priority: String,
    
    pub pro_user_max_file_size_mb: u64,
    pub pro_user_file_retention_days: i32,
    pub pro_user_queue_priority: String,
    
    pub rate_limit_requests_per_minute: i32,
    pub rate_limit_requests_per_hour: i32,
    pub max_upload_size_mb: u64,
    pub max_concurrent_uploads_per_user: usize,
    
    // Monitoring
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub logging_format: String,
    
    // Maintenance
    pub cleanup_interval_hours: u64,
    pub delete_expired_files_days: i64,
    pub delete_failed_jobs_days: i64,
    pub delete_inactive_users_days: i64,
    
    // URLs
    pub frontend_url: String,
    pub api_base_url: String,
    pub websocket_url: String,
    pub password_reset_url: String,
    pub email_verification_url: String,
    
    // Feature flags
    pub enable_google_oauth: bool,
    pub enable_stripe_payments: bool,
    pub enable_email_notifications: bool,
    pub enable_file_scanning: bool,
    pub enable_model_analysis: bool,
    pub enable_batch_processing: bool,
    pub enable_admin_dashboard: bool,
}

impl Config {
    /// Charger la configuration depuis les variables d'environnement
    pub fn from_env() -> Result<Self> {
        // Charger le fichier .env si présent
        let _ = dotenv().ok();
        
        // Variables requises
        let required_vars = [
            "DATABASE_URL",
            "JWT_SECRET",
            "REDIS_URL",
            "MINIO_BUCKET",
        ];
        
        for var in &required_vars {
            if env::var(var).is_err() {
                return Err(AppError::Validation(format!(
                    "Variable d'environnement requise manquante: {}", var
                )));
            }
        }
        
        let config = Config {
            // Environnement et serveur
            run_mode: env::var("RUN_MODE").unwrap_or_else(|_| "development".to_string()),
            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PORT must be a number".to_string()))?,
            workers: env::var("WORKERS")
                .unwrap_or_else(|_| "4".to_string())
                .parse()
                .map_err(|_| AppError::Validation("WORKERS must be a number".to_string()))?,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            
            // Base de données
            database_url: env::var("DATABASE_URL")?,
            database_max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DATABASE_MAX_CONNECTIONS must be a number".to_string()))?,
            database_min_connections: env::var("DATABASE_MIN_CONNECTIONS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DATABASE_MIN_CONNECTIONS must be a number".to_string()))?,
            database_connection_timeout: env::var("DATABASE_CONNECTION_TIMEOUT")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DATABASE_CONNECTION_TIMEOUT must be a number".to_string()))?,
            
            // Sécurité
            jwt_secret: env::var("JWT_SECRET")?,
            jwt_access_token_expiry_hours: env::var("JWT_ACCESS_TOKEN_EXPIRY_HOURS")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .map_err(|_| AppError::Validation("JWT_ACCESS_TOKEN_EXPIRY_HOURS must be a number".to_string()))?,
            jwt_refresh_token_expiry_days: env::var("JWT_REFRESH_TOKEN_EXPIRY_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .map_err(|_| AppError::Validation("JWT_REFRESH_TOKEN_EXPIRY_DAYS must be a number".to_string()))?,
            admin_email: env::var("ADMIN_EMAIL").unwrap_or_else(|_| "admin@example.com".to_string()),
            admin_password: env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin123".to_string()),
            password_reset_token_expiry_hours: env::var("PASSWORD_RESET_TOKEN_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PASSWORD_RESET_TOKEN_EXPIRY_HOURS must be a number".to_string()))?,
            api_key_expiry_days: env::var("API_KEY_EXPIRY_DAYS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .map_err(|_| AppError::Validation("API_KEY_EXPIRY_DAYS must be a number".to_string()))?,
            
            // Chiffrement
            storage_encryption_key: env::var("STORAGE_ENCRYPTION_KEY").unwrap_or_else(|_| "".to_string()),
            encryption_algorithm: env::var("ENCRYPTION_ALGORITHM").unwrap_or_else(|_| "AES-256-GCM".to_string()),
            encryption_nonce_size: env::var("ENCRYPTION_NONCE_SIZE")
                .unwrap_or_else(|_| "12".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENCRYPTION_NONCE_SIZE must be a number".to_string()))?,
            
            // Redis
            redis_url: env::var("REDIS_URL")?,
            redis_pool_size: env::var("REDIS_POOL_SIZE")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .map_err(|_| AppError::Validation("REDIS_POOL_SIZE must be a number".to_string()))?,
            redis_connection_timeout: env::var("REDIS_CONNECTION_TIMEOUT")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|_| AppError::Validation("REDIS_CONNECTION_TIMEOUT must be a number".to_string()))?,
            redis_queue_prefix: env::var("REDIS_QUEUE_PREFIX").unwrap_or_else(|_| "quant:".to_string()),
            redis_cache_ttl_seconds: env::var("REDIS_CACHE_TTL_SECONDS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .map_err(|_| AppError::Validation("REDIS_CACHE_TTL_SECONDS must be a number".to_string()))?,
            
            // MinIO/S3
            storage_type: env::var("STORAGE_TYPE").unwrap_or_else(|_| "minio".to_string()),
            minio_endpoint: env::var("MINIO_ENDPOINT").ok(),
            minio_access_key: env::var("MINIO_ACCESS_KEY").ok(),
            minio_secret_key: env::var("MINIO_SECRET_KEY").ok(),
            minio_bucket: env::var("MINIO_BUCKET")?,
            minio_region: env::var("MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            minio_secure: env::var("MINIO_SECURE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .map_err(|_| AppError::Validation("MINIO_SECURE must be a boolean".to_string()))?,
            minio_connection_timeout: env::var("MINIO_CONNECTION_TIMEOUT")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .map_err(|_| AppError::Validation("MINIO_CONNECTION_TIMEOUT must be a number".to_string()))?,
            max_file_size_mb: env::var("MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "10240".to_string())
                .parse()
                .map_err(|_| AppError::Validation("MAX_FILE_SIZE_MB must be a number".to_string()))?,
            
            // Quantification
            quantization_python_path: env::var("QUANTIZATION_PYTHON_PATH").unwrap_or_else(|_| "./python".to_string()),
            quantization_max_concurrent_jobs: env::var("QUANTIZATION_MAX_CONCURRENT_JOBS")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .map_err(|_| AppError::Validation("QUANTIZATION_MAX_CONCURRENT_JOBS must be a number".to_string()))?,
            quantization_timeout_seconds: env::var("QUANTIZATION_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .map_err(|_| AppError::Validation("QUANTIZATION_TIMEOUT_SECONDS must be a number".to_string()))?,
            quantization_max_retries: env::var("QUANTIZATION_MAX_RETRIES")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .map_err(|_| AppError::Validation("QUANTIZATION_MAX_RETRIES must be a number".to_string()))?,
            quantization_gpu_enabled: env::var("QUANTIZATION_GPU_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .map_err(|_| AppError::Validation("QUANTIZATION_GPU_ENABLED must be a boolean".to_string()))?,
            
            // Google OAuth
            google_oauth_client_id: env::var("GOOGLE_OAUTH_CLIENT_ID").ok(),
            google_oauth_client_secret: env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok(),
            google_oauth_redirect_uri: env::var("GOOGLE_OAUTH_REDIRECT_URI").ok(),
            
            // Stripe
            stripe_secret_key: env::var("STRIPE_SECRET_KEY").ok(),
            stripe_publishable_key: env::var("STRIPE_PUBLISHABLE_KEY").ok(),
            stripe_webhook_secret: env::var("STRIPE_WEBHOOK_SECRET").ok(),
            stripe_currency: env::var("STRIPE_CURRENCY").unwrap_or_else(|_| "eur".to_string()),
            stripe_trial_period_days: env::var("STRIPE_TRIAL_PERIOD_DAYS")
                .unwrap_or_else(|_| "14".to_string())
                .parse()
                .map_err(|_| AppError::Validation("STRIPE_TRIAL_PERIOD_DAYS must be a number".to_string()))?,
            stripe_price_starter: env::var("STRIPE_PRICE_STARTER").ok(),
            stripe_price_pro: env::var("STRIPE_PRICE_PRO").ok(),
            
            // Email
            email_provider: env::var("EMAIL_PROVIDER").unwrap_or_else(|_| "log".to_string()),
            email_from: env::var("EMAIL_FROM").unwrap_or_else(|_| "noreply@quantization.io".to_string()),
            email_from_name: env::var("EMAIL_FROM_NAME").unwrap_or_else(|_| "Quantization Platform".to_string()),
            sendgrid_api_key: env::var("SENDGRID_API_KEY").ok(),
            smtp_host: env::var("SMTP_HOST").ok(),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok()),
            smtp_username: env::var("SMTP_USERNAME").ok(),
            smtp_password: env::var("SMTP_PASSWORD").ok(),
            smtp_tls: env::var("SMTP_TLS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("SMTP_TLS must be a boolean".to_string()))?,
            
            // Limites et quotas
            free_user_credits_per_month: env::var("FREE_USER_CREDITS_PER_MONTH")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .map_err(|_| AppError::Validation("FREE_USER_CREDITS_PER_MONTH must be a number".to_string()))?,
            free_user_max_file_size_mb: env::var("FREE_USER_MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .map_err(|_| AppError::Validation("FREE_USER_MAX_FILE_SIZE_MB must be a number".to_string()))?,
            free_user_file_retention_days: env::var("FREE_USER_FILE_RETENTION_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .map_err(|_| AppError::Validation("FREE_USER_FILE_RETENTION_DAYS must be a number".to_string()))?,
            free_user_queue_priority: env::var("FREE_USER_QUEUE_PRIORITY").unwrap_or_else(|_| "low".to_string()),
            
            starter_user_credits_per_month: env::var("STARTER_USER_CREDITS_PER_MONTH")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .map_err(|_| AppError::Validation("STARTER_USER_CREDITS_PER_MONTH must be a number".to_string()))?,
            starter_user_max_file_size_mb: env::var("STARTER_USER_MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "10240".to_string())
                .parse()
                .map_err(|_| AppError::Validation("STARTER_USER_MAX_FILE_SIZE_MB must be a number".to_string()))?,
            starter_user_file_retention_days: env::var("STARTER_USER_FILE_RETENTION_DAYS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .map_err(|_| AppError::Validation("STARTER_USER_FILE_RETENTION_DAYS must be a number".to_string()))?,
            starter_user_queue_priority: env::var("STARTER_USER_QUEUE_PRIORITY").unwrap_or_else(|_| "medium".to_string()),
            
            pro_user_max_file_size_mb: env::var("PRO_USER_MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "20480".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PRO_USER_MAX_FILE_SIZE_MB must be a number".to_string()))?,
            pro_user_file_retention_days: env::var("PRO_USER_FILE_RETENTION_DAYS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PRO_USER_FILE_RETENTION_DAYS must be a number".to_string()))?,
            pro_user_queue_priority: env::var("PRO_USER_QUEUE_PRIORITY").unwrap_or_else(|_| "high".to_string()),
            
            rate_limit_requests_per_minute: env::var("RATE_LIMIT_REQUESTS_PER_MINUTE")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .map_err(|_| AppError::Validation("RATE_LIMIT_REQUESTS_PER_MINUTE must be a number".to_string()))?,
            rate_limit_requests_per_hour: env::var("RATE_LIMIT_REQUESTS_PER_HOUR")
                .unwrap_or_else(|_| "1000".to_string())
                .parse()
                .map_err(|_| AppError::Validation("RATE_LIMIT_REQUESTS_PER_HOUR must be a number".to_string()))?,
            max_upload_size_mb: env::var("MAX_UPLOAD_SIZE_MB")
                .unwrap_or_else(|_| "10240".to_string())
                .parse()
                .map_err(|_| AppError::Validation("MAX_UPLOAD_SIZE_MB must be a number".to_string()))?,
            max_concurrent_uploads_per_user: env::var("MAX_CONCURRENT_UPLOADS_PER_USER")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .map_err(|_| AppError::Validation("MAX_CONCURRENT_UPLOADS_PER_USER must be a number".to_string()))?,
            
            // Monitoring
            prometheus_enabled: env::var("PROMETHEUS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PROMETHEUS_ENABLED must be a boolean".to_string()))?,
            prometheus_port: env::var("PROMETHEUS_PORT")
                .unwrap_or_else(|_| "9090".to_string())
                .parse()
                .map_err(|_| AppError::Validation("PROMETHEUS_PORT must be a number".to_string()))?,
            otel_exporter_otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            logging_format: env::var("LOGGING_FORMAT").unwrap_or_else(|_| "json".to_string()),
            
            // Maintenance
            cleanup_interval_hours: env::var("CLEANUP_INTERVAL_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .map_err(|_| AppError::Validation("CLEANUP_INTERVAL_HOURS must be a number".to_string()))?,
            delete_expired_files_days: env::var("DELETE_EXPIRED_FILES_DAYS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DELETE_EXPIRED_FILES_DAYS must be a number".to_string()))?,
            delete_failed_jobs_days: env::var("DELETE_FAILED_JOBS_DAYS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DELETE_FAILED_JOBS_DAYS must be a number".to_string()))?,
            delete_inactive_users_days: env::var("DELETE_INACTIVE_USERS_DAYS")
                .unwrap_or_else(|_| "180".to_string())
                .parse()
                .map_err(|_| AppError::Validation("DELETE_INACTIVE_USERS_DAYS must be a number".to_string()))?,
            
            // URLs
            frontend_url: env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            api_base_url: env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string()),
            websocket_url: env::var("WEBSOCKET_URL").unwrap_or_else(|_| "ws://localhost:8080/ws".to_string()),
            password_reset_url: env::var("PASSWORD_RESET_URL").unwrap_or_else(|_| "http://localhost:3000/reset-password".to_string()),
            email_verification_url: env::var("EMAIL_VERIFICATION_URL").unwrap_or_else(|_| "http://localhost:3000/verify-email".to_string()),
            
            // Feature flags
            enable_google_oauth: env::var("ENABLE_GOOGLE_OAUTH")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_GOOGLE_OAUTH must be a boolean".to_string()))?,
            enable_stripe_payments: env::var("ENABLE_STRIPE_PAYMENTS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_STRIPE_PAYMENTS must be a boolean".to_string()))?,
            enable_email_notifications: env::var("ENABLE_EMAIL_NOTIFICATIONS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_EMAIL_NOTIFICATIONS must be a boolean".to_string()))?,
            enable_file_scanning: env::var("ENABLE_FILE_SCANNING")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_FILE_SCANNING must be a boolean".to_string()))?,
            enable_model_analysis: env::var("ENABLE_MODEL_ANALYSIS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_MODEL_ANALYSIS must be a boolean".to_string()))?,
            enable_batch_processing: env::var("ENABLE_BATCH_PROCESSING")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_BATCH_PROCESSING must be a boolean".to_string()))?,
            enable_admin_dashboard: env::var("ENABLE_ADMIN_DASHBOARD")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .map_err(|_| AppError::Validation("ENABLE_ADMIN_DASHBOARD must be a boolean".to_string()))?,
        };
        
        Ok(config)
    }
    
    /// Vérifier si on est en production
    pub fn is_production(&self) -> bool {
        self.run_mode == "production"
    }
    
    /// Vérifier si on est en développement
    pub fn is_development(&self) -> bool {
        self.run_mode == "development"
    }
    
    /// Vérifier si on est en staging
    pub fn is_staging(&self) -> bool {
        self.run_mode == "staging"
    }
}