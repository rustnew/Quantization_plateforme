// Modèle: user.rs
pub mod user;
pub use user::{
    User, NewUser, UserLogin, GoogleAuth, 
    AuthToken, UserProfile
};

// Modèle: job.rs
pub mod job;
pub use job::{
    Job, JobStatus, QuantizationMethod, ModelFormat,
    NewJob, JobProgress, JobResult
};

// Modèle: file.rs
pub mod file;
pub use file::{
    ModelFile, FileUpload, FileDownload,
    FileMetadata, ModelMetadata
};

// Modèle: billing.rs
pub mod billing;
pub use billing::{
    Subscription, SubscriptionPlan, SubscriptionStatus,
    CreditInfo, CreditTransaction, PlanInfo
};

// Modèle: system.rs
pub mod system;
pub use system::{
    AuditLog, HealthStatus, ServiceHealth,
    SystemMetrics, AppConfig
};

// Types communs
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Réponse paginée standard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

/// Réponse d'erreur standard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    pub details: Option<serde_json::Value>,
}

/// Réponse de succès standard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessResponse<T> {
    pub success: bool,
    pub data: T,
    pub message: Option<String>,
}

impl<T> SuccessResponse<T> {
    /// Crée une réponse de succès
    pub fn new(data: T) -> Self {
        Self {
            success: true,
            data,
            message: None,
        }
    }
    
    /// Avec un message
    pub fn with_message(data: T, message: &str) -> Self {
        Self {
            success: true,
            data,
            message: Some(message.to_string()),
        }
    }
}