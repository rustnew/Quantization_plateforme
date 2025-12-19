// services/queue.rs
use crate::utils::error::{AppError, Result};
use redis::{AsyncCommands, Client};
use uuid::Uuid;
use std::sync::Arc;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;

pub struct JobQueue {
    client: Arc<Client>,
    prefix: String,
}

impl JobQueue {
    /// Créer une nouvelle queue Redis
    pub async fn new(redis_url: &str, prefix: Option<&str>) -> Result<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| AppError::RedisError(e.to_string()))?;
        
        let conn = client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;
        
        // Tester la connexion
        let _: () = redis::cmd("PING")
            .query_async(&mut conn.into())
            .await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(Self {
            client: Arc::new(client),
            prefix: prefix.unwrap_or("quant:").to_string(),
        })
    }

    /// Ajouter un job à la queue
    pub async fn enqueue(&self, job_id: Uuid, priority: i32) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let job_data = JobData {
            id: job_id,
            enqueued_at: chrono::Utc::now(),
            priority,
        };

        let data = serde_json::to_string(&job_data)
            .map_err(|e| AppError::SerializeError(e.to_string()))?;

        // Choisir la queue selon la priorité
        let queue_name = match priority {
            3 => self.key("queue:high"),
            2 => self.key("queue:normal"),
            _ => self.key("queue:low"),
        };

        conn.lpush(&queue_name, data).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Récupérer le prochain job de la queue
    pub async fn dequeue(&self) -> Result<Option<Uuid>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        // Essayer dans l'ordre: high -> normal -> low
        let queues = [
            self.key("queue:high"),
            self.key("queue:normal"), 
            self.key("queue:low"),
        ];

        for queue in &queues {
            let data: Option<String> = conn.rpop(queue, None).await
                .map_err(|e| AppError::RedisError(e.to_string()))?;

            if let Some(data_str) = data {
                let job_data: JobData = serde_json::from_str(&data_str)
                    .map_err(|e| AppError::ParseError(e.to_string()))?;

                return Ok(Some(job_data.id));
            }
        }

        Ok(None)
    }

    /// Obtenir la taille de la queue
    pub async fn queue_size(&self, priority: Option<i32>) -> Result<u64> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        match priority {
            Some(3) => conn.llen(self.key("queue:high")).await,
            Some(2) => conn.llen(self.key("queue:normal")).await,
            Some(1) => conn.llen(self.key("queue:low")).await,
            None => {
                let high: u64 = conn.llen(self.key("queue:high")).await?;
                let normal: u64 = conn.llen(self.key("queue:normal")).await?;
                let low: u64 = conn.llen(self.key("queue:low")).await?;
                Ok(high + normal + low)
            }
        }
        .map_err(|e| AppError::RedisError(e.to_string()))
    }

    /// Publier un événement de progression
    pub async fn publish_progress(&self, job_id: Uuid, progress: i32, status: &str) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let event = ProgressEvent {
            job_id,
            progress,
            status: status.to_string(),
            timestamp: chrono::Utc::now(),
        };

        let channel = self.key(&format!("progress:{}", job_id));
        let message = serde_json::to_string(&event)
            .map_err(|e| AppError::SerializeError(e.to_string()))?;

        conn.publish(&channel, message).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// S'abonner aux événements de progression d'un job
    pub async fn subscribe_progress(&self, job_id: Uuid) -> Result<tokio::sync::mpsc::Receiver<ProgressEvent>> {
        let mut pubsub = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?
            .into_pubsub();

        let channel = self.key(&format!("progress:{}", job_id));
        pubsub.subscribe(&channel).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let mut conn = pubsub.into_on_message();
            
            while let Some(msg) = conn.next().await {
                if let Ok(payload) = msg.get_payload::<String>() {
                    if let Ok(event) = serde_json::from_str::<ProgressEvent>(&payload) {
                        let _ = tx.send(event).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Stocker un résultat temporaire
    pub async fn store_result(&self, job_id: Uuid, result: &JobResult, ttl_seconds: u64) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let key = self.key(&format!("result:{}", job_id));
        let value = serde_json::to_string(result)
            .map_err(|e| AppError::SerializeError(e.to_string()))?;

        conn.set_ex(&key, value, ttl_seconds).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Récupérer un résultat
    pub async fn get_result(&self, job_id: Uuid) -> Result<Option<JobResult>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let key = self.key(&format!("result:{}", job_id));
        let value: Option<String> = conn.get(&key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        match value {
            Some(json) => {
                let result: JobResult = serde_json::from_str(&json)
                    .map_err(|e| AppError::ParseError(e.to_string()))?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// Nettoyer les anciens résultats
    pub async fn cleanup_old_results(&self, max_age_hours: u64) -> Result<u64> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let pattern = self.key("result:*");
        let keys: Vec<String> = conn.keys(&pattern).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let mut deleted = 0;
        for key in keys {
            let ttl: i64 = conn.ttl(&key).await
                .map_err(|e| AppError::RedisError(e.to_string()))?;

            if ttl > 0 && (ttl as u64) > max_age_hours * 3600 {
                conn.del(&key).await
                    .map_err(|e| AppError::RedisError(e.to_string()))?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Vérifier la santé de Redis
    pub async fn health_check(&self) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let _: () = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Helper pour ajouter le préfixe aux clés
    fn key(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }
}

impl Clone for JobQueue {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            prefix: self.prefix.clone(),
        }
    }
}

/// Données d'un job dans la queue
#[derive(Debug, Serialize, Deserialize)]
struct JobData {
    id: Uuid,
    enqueued_at: chrono::DateTime<chrono::Utc>,
    priority: i32,
}

/// Événement de progression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub job_id: Uuid,
    pub progress: i32,
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Résultat d'un job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: Uuid,
    pub status: String,
    pub output_file_id: Option<Uuid>,
    pub error_message: Option<String>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}