use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Log d'audit pour le suivi des actions
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    /// ID unique
    pub id: Uuid,
    
    /// ID de l'utilisateur (optionnel pour actions anonymes)
    pub user_id: Option<Uuid>,
    
    /// Adresse IP
    pub ip_address: Option<String>,
    
    /// User agent
    pub user_agent: Option<String>,
    
    /// Action effectuée
    pub action: String, // "user.login", "job.create", "file.upload", etc.
    
    /// Type de ressource concernée
    pub resource_type: Option<String>, // "user", "job", "file", etc.
    
    /// ID de la ressource concernée
    pub resource_id: Option<Uuid>,
    
    /// Anciennes valeurs (JSON)
    pub old_values: Option<serde_json::Value>,
    
    /// Nouvelles valeurs (JSON)
    pub new_values: Option<serde_json::Value>,
    
    /// Message supplémentaire
    pub message: Option<String>,
    
    /// Date de l'action
    pub created_at: DateTime<Utc>,
}

/// Vérification de santé du système
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String, // "healthy", "degraded", "unhealthy"
    pub timestamp: DateTime<Utc>,
    pub services: Vec<ServiceHealth>,
    pub uptime_seconds: u64,
}

/// Santé d'un service individuel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub service: String,
    pub status: String,
    pub response_time_ms: Option<u64>,
    pub error: Option<String>,
}

/// Métriques système
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: DateTime<Utc>,
    pub active_users: i64,
    pub total_jobs: i64,
    pub jobs_pending: i64,
    pub jobs_processing: i64,
    pub jobs_completed: i64,
    pub jobs_failed: i64,
    pub queue_size: i64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub total_storage_gb: f64,
    pub used_storage_gb: f64,
}

/// Configuration de l'application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub environment: String,
    pub version: String,
    pub api_version: String,
    pub max_file_size_mb: i64,
    pub allowed_formats: Vec<String>,
    pub quantization_methods: Vec<String>,
    pub default_expiry_days: i32,
    pub rate_limit_per_minute: i32,
}

impl AuditLog {
    /// Crée un nouveau log d'audit
    pub fn new(
        user_id: Option<Uuid>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        action: String,
        resource_type: Option<String>,
        resource_id: Option<Uuid>,
        message: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            ip_address,
            user_agent,
            action,
            resource_type,
            resource_id,
            old_values: None,
            new_values: None,
            message,
            created_at: Utc::now(),
        }
    }
    
    /// Ajoute les changements de valeurs
    pub fn with_changes(mut self, old_values: serde_json::Value, new_values: serde_json::Value) -> Self {
        self.old_values = Some(old_values);
        self.new_values = Some(new_values);
        self
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            environment: "development".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            api_version: "v1".to_string(),
            max_file_size_mb: 10 * 1024, // 10GB
            allowed_formats: vec![
                "pytorch".to_string(),
                "safetensors".to_string(),
                "onnx".to_string(),
                "gguf".to_string(),
            ],
            quantization_methods: vec![
                "int8".to_string(),
                "gptq".to_string(),
                "awq".to_string(),
                "gguf_q4_0".to_string(),
                "gguf_q5_0".to_string(),
            ],
            default_expiry_days: 30,
            rate_limit_per_minute: 60,
        }
    }
}

impl HealthStatus {
    /// Crée un statut de santé
    pub fn new(services: Vec<ServiceHealth>, uptime_seconds: u64) -> Self {
        let all_healthy = services.iter().all(|s| s.status == "healthy");
        
        Self {
            status: if all_healthy {
                "healthy".to_string()
            } else {
                "degraded".to_string()
            },
            timestamp: Utc::now(),
            services,
            uptime_seconds,
        }
    }
}

impl SystemMetrics {
    /// Crée des métriques système
    pub fn new(
        active_users: i64,
        total_jobs: i64,
        jobs_pending: i64,
        jobs_processing: i64,
        jobs_completed: i64,
        jobs_failed: i64,
        queue_size: i64,
        memory_usage_mb: f64,
        cpu_usage_percent: f64,
        total_storage_gb: f64,
        used_storage_gb: f64,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            active_users,
            total_jobs,
            jobs_pending,
            jobs_processing,
            jobs_completed,
            jobs_failed,
            queue_size,
            memory_usage_mb,
            cpu_usage_percent,
            total_storage_gb,
            used_storage_gb,
        }
    }
}