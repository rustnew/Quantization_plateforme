

use fred::prelude::*;
use fred::types::RedisConfig;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing::{info, warn, error};

use crate::infrastructure::error::{AppError, AppResult};
use crate::domain::jobs::{Job, JobStatus};

/// Configuration Redis
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub max_connections: u32,
    pub command_timeout: u64, // ms
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            max_connections: 5,
            command_timeout: 3000,
        }
    }
}

/// Service de queue Redis
#[derive(Clone)]
pub struct RedisQueue {
    client: RedisClient,
    config: RedisConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueJob {
    pub job_id: Uuid,
    pub user_id: Uuid,
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

impl RedisQueue {
    /// Crée une nouvelle instance du service de queue
    pub async fn new(redis_url: &str) -> AppResult<Self> {
        info!("🔧 Initialisation du service de queue Redis...");
        
        let config = RedisConfig {
            url: redis_url.to_string(),
            ..Default::default()
        };
        
        let redis_config = RedisConfig::from_url(redis_url)?;
        let client = RedisClient::new(redis_config, None, None, None);
        
        // Se connecter à Redis
        client.connect();
        client.wait_for_connect().await.map_err(|e| {
            AppError::ConnectionError(format!("Impossible de se connecter à Redis: {}", e))
        })?;
        
        info!("✅ Service de queue Redis initialisé");
        
        Ok(Self {
            client,
            config,
        })
    }

    /// Enqueue un job dans la queue
    pub async fn enqueue_job(&self, job: &Job) -> AppResult<()> {
        let queue_job = QueueJob {
            job_id: job.id,
            user_id: job.user_id,
            priority: match job.quantization_method {
                crate::domain::jobs::QuantizationMethod::Int8 => 1,
                _ => 2, // INT4/GPTQ/AWQ ont plus de priorité
            },
            created_at: Utc::now(),
            payload: serde_json::json!({
                "job_id": job.id,
                "user_id": job.user_id,
                "model_name": job.model_name,
                "quantization_method": format!("{:?}", job.quantization_method),
                "original_size_bytes": job.original_size_bytes,
            }),
        };
        
        let queue_key = if queue_job.priority > 1 { 
            "jobs:priority" 
        } else { 
            "jobs:default" 
        };
        
        let job_json = serde_json::to_string(&queue_job)?;
        
        // Ajouter à la queue avec priorité
        self.client.zadd(
            queue_key, 
            &[(job_json.as_str(), queue_job.priority as f64)], 
            false, 
            false, 
            false
        ).await?;
        
        info!("✅ Job {} enqueued dans {}", job.id, queue_key);
        Ok(())
    }

    /// Dequeue le prochain job disponible
    pub async fn dequeue_job(&self) -> AppResult<Option<QueueJob>> {
        // Essayer d'abord la queue prioritaire
        let result = self.client.bzpopmin("jobs:priority", 1.0).await?;
        
        let (queue_key, job_data) = if let Some((_, member)) = result {
            ("jobs:priority", member)
        } else {
            // Si pas de jobs prioritaires, essayer la queue normale
            let result = self.client.bzpopmin("jobs:default", 1.0).await?;
            if let Some((_, member)) = result {
                ("jobs:default", member)
            } else {
                return Ok(None);
            }
        };
        
        let queue_job: QueueJob = serde_json::from_str(&job_data)?;
        info!("🔄 Job {} dequeued depuis {}", queue_job.job_id, queue_key);
        
        Ok(Some(queue_job))
    }

    /// Vérifier si un job est en cours de traitement
    pub async fn is_job_processing(&self, job_id: &Uuid) -> AppResult<bool> {
        let result = self.client.get(format!("jobs:processing:{}", job_id)).await?;
        Ok(result.is_some())
    }

    /// Marquer un job comme en cours de traitement
    pub async fn mark_job_processing(&self, job_id: &Uuid) -> AppResult<()> {
        let key = format!("jobs:processing:{}", job_id);
        self.client.set(key, "1", Some(3600), None, false).await?; // Expire après 1h
        Ok(())
    }

    /// Marquer un job comme complété
    pub async fn mark_job_completed(&self, job_id: &Uuid) -> AppResult<()> {
        let key = format!("jobs:processing:{}", job_id);
        self.client.del(&[key.as_str()]).await?;
        Ok(())
    }

    /// Obtenir le statut de la queue
    pub async fn get_queue_status(&self) -> AppResult<serde_json::Value> {
        let default_count = self.client.zcount("jobs:default", 0.0, 100.0).await?;
        let priority_count = self.client.zcount("jobs:priority", 0.0, 100.0).await?;
        let processing_count = self.client.keys("jobs:processing:*").await?.len();
        
        Ok(serde_json::json!({
            "default_queue_count": default_count,
            "priority_queue_count": priority_count,
            "processing_count": processing_count,
            "total_jobs": default_count + priority_count + processing_count as i64
        }))
    }

    /// Création mock pour les tests
    #[cfg(test)]
    pub fn new_test() -> Self {
        // Version mock pour les tests
        Self {
            client: RedisClient::new(RedisConfig::default(), None, None, None),
            config: RedisConfig::default(),
        }
    }
}
