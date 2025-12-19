// services/database.rs
use crate::models::{
    User, Job, ModelFile, Subscription, CreditTransaction,
    JobStatus, QuantizationMethod, ModelFormat,
    SubscriptionPlan, SubscriptionStatus,
};
use crate::utils::error::{AppError, Result};
use sqlx::{PgPool, postgres::PgPoolOptions, Row, FromRow};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;

pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Créer une nouvelle instance de base de données
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .connect_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Self { pool })
    }

    /// Exécuter les migrations
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        
        Ok(())
    }

    // === UTILISATEURS ===

    /// Vérifier si un utilisateur existe par email
    pub async fn user_exists_by_email(&self, email: &str) -> Result<bool> {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)"
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(exists.0)
    }

    /// Créer un nouvel utilisateur
    pub async fn create_user(&self, user: &User) -> Result<User> {
        let row = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, email, password_hash, created_at, last_login_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.created_at)
        .bind(user.last_login_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row)
    }

    /// Récupérer un utilisateur par email
    pub async fn get_user_by_email(&self, email: &str) -> Result<User> {
        let row = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE email = $1 AND deleted_at IS NULL"
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AppError::UserNotFound)?;

        Ok(row)
    }

    /// Récupérer un utilisateur par ID
    pub async fn get_user_by_id(&self, id: Uuid) -> Result<User> {
        let row = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AppError::UserNotFound)?;

        Ok(row)
    }

    /// Mettre à jour la dernière connexion
    pub async fn update_user_last_login(&self, user_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE users SET last_login_at = $1 WHERE id = $2"
        )
        .bind(Utc::now())
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mettre à jour le mot de passe
    pub async fn update_user_password(&self, user_id: Uuid, password_hash: &str) -> Result<()> {
        sqlx::query(
            "UPDATE users SET password_hash = $1, updated_at = $2 WHERE id = $3"
        )
        .bind(password_hash)
        .bind(Utc::now())
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Soft delete d'un utilisateur
    pub async fn soft_delete_user(&self, user_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE users SET deleted_at = $1 WHERE id = $2"
        )
        .bind(Utc::now())
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Obtenir l'ID Stripe d'un utilisateur
    pub async fn get_user_stripe_id(&self, user_id: Uuid) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT stripe_customer_id FROM users WHERE id = $1"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row.and_then(|r| r.0))
    }

    /// Mettre à jour l'ID Stripe
    pub async fn update_user_stripe_id(&self, user_id: Uuid, stripe_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE users SET stripe_customer_id = $1 WHERE id = $2"
        )
        .bind(stripe_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // === JOBS ===

    /// Créer un nouveau job
    pub async fn create_job(&self, job: &Job) -> Result<Job> {
        let row = sqlx::query_as::<_, Job>(
            r#"
            INSERT INTO jobs (
                id, user_id, name, status, progress,
                quantization_method, input_format, output_format,
                input_file_id, credits_used, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#
        )
        .bind(job.id)
        .bind(job.user_id)
        .bind(&job.name)
        .bind(&job.status)
        .bind(job.progress)
        .bind(&job.quantization_method)
        .bind(&job.input_format)
        .bind(&job.output_format)
        .bind(job.input_file_id)
        .bind(job.credits_used)
        .bind(job.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row)
    }

    /// Récupérer un job par ID
    pub async fn get_job(&self, job_id: Uuid) -> Result<Job> {
        let row = sqlx::query_as::<_, Job>(
            "SELECT * FROM jobs WHERE id = $1"
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AppError::JobNotFound)?;

        Ok(row)
    }

    /// Mettre à jour le statut d'un job
    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: &JobStatus,
        progress: i32,
    ) -> Result<()> {
        let now = Utc::now();
        
        let mut query = sqlx::query(
            "UPDATE jobs SET status = $1, progress = $2, updated_at = $3"
        )
        .bind(status)
        .bind(progress)
        .bind(now);

        // Si le job démarre, mettre started_at
        if matches!(status, JobStatus::Processing) {
            query = sqlx::query(
                "UPDATE jobs SET status = $1, progress = $2, updated_at = $3, started_at = $3 WHERE id = $4"
            )
            .bind(status)
            .bind(progress)
            .bind(now)
            .bind(job_id);
        }

        query.execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mettre à jour la complétion d'un job
    pub async fn update_job_completion(&self, job_id: Uuid, job: &Job) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs 
            SET status = $1, progress = $2, output_file_id = $3,
                quantized_size = $4, processing_time = $5,
                completed_at = $6, updated_at = $7
            WHERE id = $8
            "#
        )
        .bind(&job.status)
        .bind(job.progress)
        .bind(job.output_file_id)
        .bind(job.quantized_size)
        .bind(job.processing_time)
        .bind(job.completed_at)
        .bind(Utc::now())
        .bind(job_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Lister les jobs d'un utilisateur
    pub async fn list_user_jobs(
        &self,
        user_id: Uuid,
        status_filter: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Job>> {
        let offset = (page - 1) * per_page;
        
        let mut query = "SELECT * FROM jobs WHERE user_id = $1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<sqlx::Postgres> + Send + Sync + '_>> = vec![
            Box::new(user_id)
        ];

        if let Some(status) = status_filter {
            query.push_str(" AND status::text = $2");
            params.push(Box::new(status));
        }

        query.push_str(" ORDER BY created_at DESC LIMIT $");
        query.push_str(&format!("{} OFFSET ${}", params.len() + 1, params.len() + 2));
        
        params.push(Box::new(per_page));
        params.push(Box::new(offset));

        let rows = sqlx::query_as::<_, Job>(&query)
            .bind(user_id)
            .bind_all(params)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows)
    }

    /// Obtenir les statistiques des jobs
    pub async fn get_job_stats(&self, user_id: Option<Uuid>) -> Result<JobStats> {
        let mut query = "
            SELECT 
                COUNT(*) as total,
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as pending,
                SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) as processing,
                SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed,
                SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END) as cancelled,
                AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_duration
            FROM jobs
        ".to_string();

        if let Some(uid) = user_id {
            query.push_str(" WHERE user_id = $1");
        }

        let row = sqlx::query(&query)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let stats = JobStats {
            total: row.get::<i64, _>("total"),
            pending: row.get::<i64, _>("pending"),
            processing: row.get::<i64, _>("processing"),
            completed: row.get::<i64, _>("completed"),
            failed: row.get::<i64, _>("failed"),
            cancelled: row.get::<i64, _>("cancelled"),
            average_duration_seconds: row.get::<Option<f64>, _>("avg_duration").unwrap_or(0.0),
        };

        Ok(stats)
    }

    // === FICHIERS ===

    /// Créer une entrée de fichier
    pub async fn create_file(&self, file: &ModelFile) -> Result<ModelFile> {
        let row = sqlx::query_as::<_, ModelFile>(
            r#"
            INSERT INTO model_files (
                id, user_id, original_filename, storage_filename,
                file_size, checksum_sha256, format, model_type,
                architecture, parameter_count, storage_bucket,
                storage_path, created_at, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#
        )
        .bind(file.id)
        .bind(file.user_id)
        .bind(&file.original_filename)
        .bind(&file.storage_filename)
        .bind(file.file_size)
        .bind(&file.checksum_sha256)
        .bind(&file.format)
        .bind(&file.model_type)
        .bind(&file.architecture)
        .bind(file.parameter_count)
        .bind(&file.storage_bucket)
        .bind(&file.storage_path)
        .bind(file.created_at)
        .bind(file.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row)
    }

    /// Récupérer un fichier par ID
    pub async fn get_file(&self, file_id: Uuid) -> Result<ModelFile> {
        let row = sqlx::query_as::<_, ModelFile>(
            "SELECT * FROM model_files WHERE id = $1"
        )
        .bind(file_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AppError::FileNotFound)?;

        Ok(row)
    }

    /// Mettre à jour le token de téléchargement
    pub async fn update_file_download_token(
        &self,
        file_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE model_files SET download_token = $1, download_expires_at = $2 WHERE id = $3"
        )
        .bind(token)
        .bind(expires_at)
        .bind(file_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Lister les fichiers d'un utilisateur
    pub async fn list_user_files(
        &self,
        user_id: Uuid,
        format_filter: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<ModelFile>> {
        let offset = (page - 1) * per_page;
        
        let mut query = "SELECT * FROM model_files WHERE user_id = $1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<sqlx::Postgres> + Send + Sync + '_>> = vec![
            Box::new(user_id)
        ];

        if let Some(format) = format_filter {
            query.push_str(" AND format::text = $2");
            params.push(Box::new(format));
        }

        query.push_str(" ORDER BY created_at DESC LIMIT $");
        query.push_str(&format!("{} OFFSET ${}", params.len() + 1, params.len() + 2));
        
        params.push(Box::new(per_page));
        params.push(Box::new(offset));

        let rows = sqlx::query_as::<_, ModelFile>(&query)
            .bind_all(params)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows)
    }

    /// Supprimer un fichier (soft delete)
    pub async fn delete_file(&self, file_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE model_files SET expires_at = $1 WHERE id = $2"
        )
        .bind(Utc::now())
        .bind(file_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // === ABONNEMENTS ===

    /// Créer un abonnement
    pub async fn create_subscription(&self, subscription: &Subscription) -> Result<Subscription> {
        let row = sqlx::query_as::<_, Subscription>(
            r#"
            INSERT INTO subscriptions (
                id, user_id, plan, status,
                current_period_start, current_period_end,
                stripe_subscription_id, stripe_price_id,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#
        )
        .bind(subscription.id)
        .bind(subscription.user_id)
        .bind(&subscription.plan)
        .bind(&subscription.status)
        .bind(subscription.current_period_start)
        .bind(subscription.current_period_end)
        .bind(&subscription.stripe_subscription_id)
        .bind(&subscription.stripe_price_id)
        .bind(subscription.created_at)
        .bind(subscription.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row)
    }

    /// Récupérer l'abonnement d'un utilisateur
    pub async fn get_user_subscription(&self, user_id: Uuid) -> Result<Subscription> {
        let row = sqlx::query_as::<_, Subscription>(
            "SELECT * FROM subscriptions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AppError::NotFound("Abonnement non trouvé".to_string()))?;

        Ok(row)
    }

    /// Mettre à jour un abonnement
    pub async fn update_subscription(&self, subscription: &Subscription) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE subscriptions 
            SET plan = $1, status = $2, current_period_start = $3,
                current_period_end = $4, stripe_subscription_id = $5,
                stripe_price_id = $6, cancelled_at = $7, updated_at = $8
            WHERE id = $9
            "#
        )
        .bind(&subscription.plan)
        .bind(&subscription.status)
        .bind(subscription.current_period_start)
        .bind(subscription.current_period_end)
        .bind(&subscription.stripe_subscription_id)
        .bind(&subscription.stripe_price_id)
        .bind(subscription.cancelled_at)
        .bind(subscription.updated_at)
        .bind(subscription.id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // === CRÉDITS ===

    /// Obtenir le total des crédits d'un utilisateur
    pub async fn get_user_total_credits(&self, user_id: Uuid) -> Result<i32> {
        let row: (i32,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount), 0) FROM credit_transactions WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row.0)
    }

    /// Obtenir les crédits utilisés
    pub async fn get_user_used_credits(&self, user_id: Uuid) -> Result<i32> {
        let row: (i32,) = sqlx::query_as(
            "SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_transactions 
             WHERE user_id = $1 AND amount < 0"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row.0)
    }

    /// Créer une transaction de crédits
    pub async fn create_credit_transaction(
        &self,
        user_id: Uuid,
        transaction_type: &str,
        amount: i32,
        description: &str,
    ) -> Result<()> {
        let total_credits = self.get_user_total_credits(user_id).await?;
        let balance_after = total_credits + amount;

        sqlx::query(
            r#"
            INSERT INTO credit_transactions (
                id, user_id, transaction_type, amount,
                balance_after, description, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(transaction_type)
        .bind(amount)
        .bind(balance_after)
        .bind(description)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Obtenir l'historique des transactions de crédits
    pub async fn get_user_credit_transactions(
        &self,
        user_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<CreditTransaction>> {
        let offset = (page - 1) * per_page;
        
        let rows = sqlx::query_as::<_, CreditTransaction>(
            r#"
            SELECT * FROM credit_transactions 
            WHERE user_id = $1 
            ORDER BY created_at DESC 
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(user_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows)
    }

    /// Réinitialiser les crédits mensuels (cron job)
    pub async fn reset_monthly_credits(&self) -> Result<u64> {
        // Pour les utilisateurs avec abonnement payant
        let result = sqlx::query(
            r#"
            WITH user_credits AS (
                SELECT 
                    s.user_id,
                    CASE 
                        WHEN s.plan = 'starter' THEN 10
                        WHEN s.plan = 'pro' THEN -1 -- illimité
                        ELSE 0
                    END as monthly_credits
                FROM subscriptions s
                WHERE s.status = 'active'
                AND s.current_period_start <= NOW()
                AND s.current_period_end >= NOW()
            )
            INSERT INTO credit_transactions (id, user_id, transaction_type, amount, balance_after, description)
            SELECT 
                gen_random_uuid(),
                uc.user_id,
                'monthly_reset',
                uc.monthly_credits,
                COALESCE((
                    SELECT SUM(amount) 
                    FROM credit_transactions ct 
                    WHERE ct.user_id = uc.user_id
                ), 0) + uc.monthly_credits,
                'Réinitialisation mensuelle des crédits'
            FROM user_credits uc
            WHERE uc.monthly_credits > 0
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(result.rows_affected())
    }

    // === CLÉS API ===

    /// Créer une clé API
    pub async fn create_api_key(
        &self,
        user_id: Uuid,
        api_key: &str,
        name: &str,
        permissions: &[String],
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO api_keys (id, user_id, key, name, permissions, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(api_key)
        .bind(name)
        .bind(permissions)
        .bind(Utc::now())
        .bind(Utc::now() + chrono::Duration::days(90))
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Récupérer les permissions d'une clé API
    pub async fn get_api_key_permissions(
        &self,
        api_key: &str,
    ) -> Result<(Uuid, Vec<String>)> {
        let row: Option<(Uuid, Vec<String>)> = sqlx::query_as(
            "SELECT user_id, permissions FROM api_keys WHERE key = $1 AND expires_at > NOW()"
        )
        .bind(api_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        row.ok_or(AppError::Unauthorized)
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

/// Statistiques des jobs
#[derive(Debug)]
pub struct JobStats {
    pub total: i64,
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub average_duration_seconds: f64,
}