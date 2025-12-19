// utils/error.rs
use actix_web::{HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum AppError {
    // Erreurs d'authentification
    #[error("Authentication failed")]
    Unauthorized,
    
    #[error("Invalid token")]
    InvalidToken,
    
    #[error("Token expired")]
    TokenExpired,
    
    // Erreurs utilisateur
    #[error("User not found")]
    UserNotFound,
    
    #[error("User already exists")]
    UserAlreadyExists,
    
    #[error("Invalid credentials")]
    InvalidCredentials,
    
    // Erreurs de données
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Serialize error: {0}")]
    SerializeError(String),
    
    // Erreurs de ressources
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Resource already exists")]
    AlreadyExists,
    
    #[error("Insufficient credits")]
    InsufficientCredits,
    
    #[error("Job not found")]
    JobNotFound,
    
    #[error("File not found")]
    FileNotFound,
    
    #[error("File too large")]
    FileTooLarge,
    
    #[error("Invalid file format")]
    InvalidFileFormat,
    
    // Erreurs de traitement
    #[error("Job cannot be cancelled")]
    JobCannotBeCancelled,
    
    #[error("Job cannot be retried")]
    JobCannotBeRetried,
    
    #[error("Invalid combination of parameters")]
    InvalidCombination,
    
    #[error("GPU required for this operation")]
    GpuRequired,
    
    // Erreurs de paiement
    #[error("Invalid plan")]
    InvalidPlan,
    
    #[error("No active subscription")]
    NoSubscription,
    
    #[error("Payment failed")]
    PaymentFailed,
    
    // Erreurs externes
    #[error("External service error: {0}")]
    ExternalService(String),
    
    #[error("Stripe error: {0}")]
    StripeError(String),
    
    // Erreurs de base de données
    #[error("Database error: {0}")]
    Database(String),
    
    // Erreurs de stockage
    #[error("Storage error: {0}")]
    StorageError(String),
    
    // Erreurs Redis
    #[error("Redis error: {0}")]
    RedisError(String),
    
    // Erreurs de chiffrement
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    // Erreurs système
    #[error("Resource busy")]
    ResourceBusy,
    
    #[error("Invalid path")]
    InvalidPath,
    
    #[error("Notification error: {0}")]
    NotificationError(String),
    
    #[error("Internal server error")]
    Internal,
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            // 400 - Bad Request
            AppError::Validation(_)
            | AppError::InvalidCombination
            | AppError::InvalidPlan
            | AppError::InvalidPath => {
                HttpResponse::BadRequest().json(json!({
                    "error": self.to_string(),
                    "code": "BAD_REQUEST"
                }))
            }
            
            // 401 - Unauthorized
            AppError::Unauthorized
            | AppError::InvalidToken
            | AppError::TokenExpired
            | AppError::InvalidCredentials => {
                HttpResponse::Unauthorized().json(json!({
                    "error": self.to_string(),
                    "code": "UNAUTHORIZED"
                }))
            }
            
            // 403 - Forbidden
            AppError::GpuRequired => {
                HttpResponse::Forbidden().json(json!({
                    "error": self.to_string(),
                    "code": "FORBIDDEN"
                }))
            }
            
            // 404 - Not Found
            AppError::NotFound(_)
            | AppError::UserNotFound
            | AppError::JobNotFound
            | AppError::FileNotFound
            | AppError::NoSubscription => {
                HttpResponse::NotFound().json(json!({
                    "error": self.to_string(),
                    "code": "NOT_FOUND"
                }))
            }
            
            // 409 - Conflict
            AppError::UserAlreadyExists
            | AppError::AlreadyExists => {
                HttpResponse::Conflict().json(json!({
                    "error": self.to_string(),
                    "code": "CONFLICT"
                }))
            }
            
            // 412 - Precondition Failed
            AppError::JobCannotBeCancelled
            | AppError::JobCannotBeRetried => {
                HttpResponse::PreconditionFailed().json(json!({
                    "error": self.to_string(),
                    "code": "PRECONDITION_FAILED"
                }))
            }
            
            // 413 - Payload Too Large
            AppError::FileTooLarge => {
                HttpResponse::PayloadTooLarge().json(json!({
                    "error": self.to_string(),
                    "code": "PAYLOAD_TOO_LARGE"
                }))
            }
            
            // 422 - Unprocessable Entity
            AppError::InvalidFileFormat => {
                HttpResponse::UnprocessableEntity().json(json!({
                    "error": self.to_string(),
                    "code": "UNPROCESSABLE_ENTITY"
                }))
            }
            
            // 429 - Too Many Requests
            AppError::ResourceBusy => {
                HttpResponse::TooManyRequests().json(json!({
                    "error": self.to_string(),
                    "code": "TOO_MANY_REQUESTS"
                }))
            }
            
            // 402 - Payment Required
            AppError::InsufficientCredits => {
                HttpResponse::PaymentRequired().json(json!({
                    "error": self.to_string(),
                    "code": "PAYMENT_REQUIRED"
                }))
            }
            
            // 500 - Internal Server Error
            _ => {
                log::error!("Internal server error: {}", self);
                HttpResponse::InternalServerError().json(json!({
                    "error": "Internal server error",
                    "code": "INTERNAL_ERROR"
                }))
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Resource not found".to_string()),
            _ => AppError::Database(err.to_string()),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::SerializeError(err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::StorageError(err.to_string())
    }
}

impl From<uuid::Error> for AppError {
    fn from(err: uuid::Error) -> Self {
        AppError::Validation(err.to_string())
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        let messages: Vec<String> = err
            .field_errors()
            .iter()
            .map(|(field, errors)| {
                let error_messages: Vec<String> = errors
                    .iter()
                    .filter_map(|e| e.message.as_ref().map(|m| m.to_string()))
                    .collect();
                format!("{}: {}", field, error_messages.join(", "))
            })
            .collect();
        
        AppError::Validation(messages.join("; "))
    }
}

// Type de résultat standard
pub type Result<T> = std::result::Result<T, AppError>;