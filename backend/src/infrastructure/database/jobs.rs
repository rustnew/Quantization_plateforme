

use sqlx::{Pool, Postgres, Error as SqlxError, query_as, query_scalar, query};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use validator::Validate;

use crate::{
    domain::jobs::{Job, JobStatus, QuantizationMethod, NewJob},
    infrastructure::error::{AppError, AppResult},
};

/// Repository pour les opérations sur les jobs de quantification
#[derive(Clone)]
pub struct JobsRepository {
    pool: Pool<Postgres>,
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("Job non trouvé")]
    NotFound,
    #[error("Job déjà complété ou échoué")]
    AlreadyCompleted,
    #[error("Méthode de quantification non supportée")]
    InvalidQuantizationMethod,
    #[error("Statut de job invalide")]
    InvalidStatus,
    #[error("Validation échouée: {0}")]
    ValidationError(#[from] validator::ValidationErrors),
    #[error("Erreur de base de données: {0}")]
    DatabaseError(#[from] SqlxError),
}

impl From<JobError> for AppError {
    fn from(error: JobError) -> Self {
        match error {
            JobError::NotFound => AppError::NotFound("Job".to_string()),
            JobError::AlreadyCompleted => AppError::Conflict("Job déjà terminé".to_string()),
            JobError::InvalidQuantizationMethod => AppError::ValidationError(
                validator::ValidationError::new("quantization_method").into()
            ),
            JobError::InvalidStatus => AppError::ValidationError(
                validator::ValidationError::new("status").into()
            ),
            JobError::ValidationError(errors) => AppError::ValidationError(errors),
            JobError::DatabaseError(e) => AppError::DatabaseError(e),
        }
    }
}

impl JobsRepository {
    /// Crée une nouvelle instance du repository
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }


    pub async fn create(&self, new_job: &NewJob) -> AppResult<Job> {
        // Validation des données d'entrée
        new_job.validate().map_err(JobError::ValidationError)?;
        
        // Vérification de la méthode de quantification
        if !Self::is_valid_quantization_method(&new_job.quantization_method) {
            return Err(JobError::InvalidQuantizationMethod.into());
        }

        let job_id = Uuid::new_v4();
        let download_token = Self::generate_download_token();
        let now = Utc::now();

        // Création du job dans la base de données
        let job = query_as!(
            Job,
            r#"
            INSERT INTO jobs (
                id, user_id, model_name, original_size_bytes, quantization_method,
                status, download_token, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            "#,
            job_id,
            new_job.user_id,
            new_job.model_name,
            new_job.original_size_bytes as i64,
            new_job.quantization_method.to_string(),
            JobStatus::Queued.to_string(),
            download_token,
            now,
            now
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(job)
    }

    /// Récupère un job par son ID
    /// 
    /// # Arguments
    /// * `job_id` - L'identifiant du job
    /// 
    /// # Retourne
    /// * `Ok(Job)` - Le job trouvé
    /// * `Err(AppError)` - Si le job n'existe pas ou erreur de base de données
    pub async fn get_by_id(&self, job_id: &Uuid) -> AppResult<Job> {
        let job = query_as!(
            Job,
            r#"
            SELECT 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            FROM jobs
            WHERE id = $1
            "#,
            job_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(JobError::NotFound)?;

        Ok(job)
    }

    /// Récupère tous les jobs d'un utilisateur avec pagination
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// * `limit` - Nombre maximum de jobs à retourner
    /// * `offset` - Offset pour la pagination
    /// 
    /// # Retourne
    /// * `Ok(Vec<Job>)` - Liste des jobs de l'utilisateur
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn get_by_user(&self, user_id: &Uuid, limit: i64, offset: i64) -> AppResult<Vec<Job>> {
        let jobs = query_as!(
            Job,
            r#"
            SELECT 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            FROM jobs
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            limit as i64,
            offset as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }

    /// Récupère les jobs par statut avec pagination
    /// 
    /// # Arguments
    /// * `status` - Le statut des jobs à récupérer
    /// * `limit` - Nombre maximum de jobs à retourner
    /// * `offset` - Offset pour la pagination
    /// 
    /// # Retourne
    /// * `Ok(Vec<Job>)` - Liste des jobs avec le statut spécifié
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn get_by_status(&self, status: JobStatus, limit: i64, offset: i64) -> AppResult<Vec<Job>> {
        let jobs = query_as!(
            Job,
            r#"
            SELECT 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            FROM jobs
            WHERE status = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
            status.to_string(),
            limit as i64,
            offset as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }

    /// Récupère les jobs par statut et utilisateur
    pub async fn get_by_status_and_user(&self, user_id: &Uuid, status: JobStatus, limit: i64, offset: i64) -> AppResult<Vec<Job>> {
        let jobs = query_as!(
            Job,
            r#"
            SELECT 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            FROM jobs
            WHERE user_id = $1 AND status = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            user_id,
            status.to_string(),
            limit as i64,
            offset as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }


    pub async fn update_status(&self, job_id: &Uuid, new_status: JobStatus) -> AppResult<Job> {
        // Vérifier que le job existe et n'est pas déjà terminé
        let current_job = self.get_by_id(job_id).await?;
        
        if current_job.status == JobStatus::Completed || current_job.status == JobStatus::Failed {
            return Err(JobError::AlreadyCompleted.into());
        }

        let now = Utc::now();
        let updated_job = query_as!(
            Job,
            r#"
            UPDATE jobs
            SET status = $1, updated_at = $2
            WHERE id = $3
            RETURNING 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            "#,
            new_status.to_string(),
            now,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_job)
    }

    pub async fn complete_job(&self, job_id: &Uuid, quantized_size_bytes: i64, download_url: String) -> AppResult<Job> {
        let current_job = self.get_by_id(job_id).await?;
        
        if current_job.status == JobStatus::Completed || current_job.status == JobStatus::Failed {
            return Err(JobError::AlreadyCompleted.into());
        }

        // Calculer le pourcentage de réduction
        let reduction_percent = if current_job.original_size_bytes > 0 {
            let original = current_job.original_size_bytes as f64;
            let quantized = quantized_size_bytes as f64;
            ((original - quantized) / original * 100.0) as f32
        } else {
            0.0
        };

        let now = Utc::now();
        let completed_job = query_as!(
            Job,
            r#"
            UPDATE jobs
            SET 
                quantized_size_bytes = $1,
                reduction_percent = $2,
                download_url = $3,
                status = $4,
                updated_at = $5
            WHERE id = $6
            RETURNING 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            "#,
            quantized_size_bytes,
            reduction_percent,
            download_url,
            JobStatus::Completed.to_string(),
            now,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(completed_job)
    }

   
    pub async fn fail_job(&self, job_id: &Uuid, error_message: String) -> AppResult<Job> {
        let current_job = self.get_by_id(job_id).await?;
        
        if current_job.status == JobStatus::Completed || current_job.status == JobStatus::Failed {
            return Err(JobError::AlreadyCompleted.into());
        }

        let now = Utc::now();
        let failed_job = query_as!(
            Job,
            r#"
            UPDATE jobs
            SET 
                error_message = $1,
                status = $2,
                updated_at = $3
            WHERE id = $4
            RETURNING 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_token,
                created_at, updated_at
            "#,
            error_message,
            JobStatus::Failed.to_string(),
            now,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(failed_job)
    }

   
    pub async fn verify_download_token(&self, job_id: &Uuid, token: &str) -> AppResult<bool> {
        let job = self.get_by_id(job_id).await?;
        Ok(job.download_token == token)
    }

    /// Génère un token de téléchargement sécurisé unique
    fn generate_download_token() -> String {
        let uuid_part = Uuid::new_v4().to_string();
        let timestamp_part = Utc::now().timestamp().to_string();
        let random_part: u32 = rand::random();
        
        format!("{}_{}_{}", uuid_part, timestamp_part, random_part)
    }

    /// Vérifie si une méthode de quantification est valide
    fn is_valid_quantization_method(method: &QuantizationMethod) -> bool {
        matches!(method, 
            QuantizationMethod::Int8 | 
            QuantizationMethod::Int4 | 
            QuantizationMethod::Gptq | 
            QuantizationMethod::Awq
        )
    }

    
    pub async fn count_by_user(&self, user_id: &Uuid) -> AppResult<i64> {
        let count = query_scalar!(
            r#"
            SELECT COUNT(*) as count
            FROM jobs
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.pool)
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }

   
    pub async fn cleanup_old_jobs(&self, cutoff_date: DateTime<Utc>) -> AppResult<i64> {
        let result = query!(
            r#"
            DELETE FROM jobs
            WHERE created_at < $1
            "#,
            cutoff_date
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }
}
