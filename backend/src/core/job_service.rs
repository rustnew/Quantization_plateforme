// core/job_service.rs
use crate::models::{
    Job, JobStatus, QuantizationMethod, ModelFormat,
    NewJob, JobResult, FileMetadata
};
use crate::services::{
    database::Database,
    queue::JobQueue,
    storage::FileStorage,
};
use crate::utils::error::{AppError, Result};
use crate::core::quantization_service::QuantizationService;
use uuid::Uuid;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct JobService {
    db: Arc<Database>,
    queue: Arc<JobQueue>,
    storage: Arc<FileStorage>,
    quantizer: Arc<QuantizationService>,
    max_concurrent_jobs: usize,
    active_jobs: RwLock<Vec<Uuid>>,
}

impl JobService {
    pub fn new(
        db: Arc<Database>,
        queue: Arc<JobQueue>,
        storage: Arc<FileStorage>,
        quantizer: Arc<QuantizationService>,
        max_concurrent_jobs: usize,
    ) -> Self {
        Self {
            db,
            queue,
            storage,
            quantizer,
            max_concurrent_jobs,
            active_jobs: RwLock::new(Vec::new()),
        }
    }

    /// Créer un nouveau job de quantification
    pub async fn create_job(
        &self,
        user_id: Uuid,
        input_file_id: Uuid,
        name: String,
        quantization_method: QuantizationMethod,
        output_format: ModelFormat,
    ) -> Result<Job> {
        // Récupérer les métadonnées du fichier
        let file_metadata = self.storage.get_file_metadata(input_file_id).await?;
        
        // Vérifier que le fichier appartient à l'utilisateur
        if file_metadata.user_id != user_id {
            return Err(AppError::Unauthorized);
        }

        // Vérifier la compatibilité format/méthode
        if !self.is_compatible(&file_metadata.format, &quantization_method, &output_format) {
            return Err(AppError::InvalidCombination);
        }

        // Calculer le coût en crédits
        let credits_cost = self.calculate_job_cost(
            user_id,
            &quantization_method,
            &file_metadata,
        ).await?;

        // Créer le job en base
        let job = Job::new(
            user_id,
            name,
            quantization_method,
            file_metadata.format,
            output_format,
            input_file_id,
            credits_cost,
        );

        let job = self.db.create_job(&job).await?;

        // Ajouter à la queue avec priorité selon le plan
        let subscription = self.db.get_user_subscription(user_id).await?;
        let priority = subscription.plan.queue_priority();
        
        self.queue.enqueue(job.id, priority).await?;

        Ok(job)
    }

    /// Traiter un job depuis la queue
    pub async fn process_next_job(&self) -> Result<()> {
        // Vérifier le nombre maximum de jobs simultanés
        let active_count = self.active_jobs.read().await.len();
        if active_count >= self.max_concurrent_jobs {
            return Ok(());
        }

        // Récupérer un job de la queue
        let job_id = match self.queue.dequeue().await? {
            Some(id) => id,
            None => return Ok(()), // Pas de job en attente
        };

        // Marquer comme actif
        self.active_jobs.write().await.push(job_id);

        // Traiter le job en arrière-plan
        let self_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone.process_job(job_id).await {
                eprintln!("Erreur lors du traitement du job {}: {}", job_id, e);
            }
            
            // Retirer du tableau des jobs actifs
            self_clone.active_jobs.write().await.retain(|&id| id != job_id);
        });

        Ok(())
    }

    /// Traiter un job spécifique
    async fn process_job(&self, job_id: Uuid) -> Result<()> {
        // Récupérer le job
        let mut job = self.db.get_job(job_id).await?;

        // Mettre à jour le statut
        job.start();
        self.db.update_job_status(job.id, &job.status, job.progress).await?;

        // Récupérer le fichier source
        let input_file = self.storage.get_file_metadata(job.input_file_id).await?;
        
        // Télécharger le fichier source
        let input_path = self.storage.download_file(job.input_file_id).await?;

        // Quantifier le modèle
        let output_path = match self.quantizer.quantize(
            &input_path,
            &job.quantization_method,
            &job.output_format,
            job.id,
        ).await {
            Ok(path) => path,
            Err(e) => {
                job.fail(e.to_string());
                self.db.update_job_status(job.id, &job.status, job.progress).await?;
                return Err(e);
            }
        };

        // Uploader le résultat
        let output_filename = format!("{}_{}.bin", job.name, job.id);
        let output_file_id = self.storage.upload_result(
            job.user_id,
            &output_filename,
            &output_path,
            job.output_format.clone(),
        ).await?;

        // Mettre à jour le job avec succès
        let file_size = std::fs::metadata(&output_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        
        job.complete(output_file_id, file_size);
        self.db.update_job_completion(job.id, &job).await?;

        // Nettoyer les fichiers temporaires
        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);

        Ok(())
    }

    /// Obtenir un job par ID
    pub async fn get_job(&self, job_id: Uuid) -> Result<Job> {
        self.db.get_job(job_id).await
    }

    /// Lister les jobs d'un utilisateur
    pub async fn list_user_jobs(
        &self,
        user_id: Uuid,
        status_filter: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Job>> {
        self.db.list_user_jobs(user_id, status_filter, page, per_page).await
    }

    /// Annuler un job
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.db.get_job(job_id).await?;
        
        if !job.can_be_cancelled() {
            return Err(AppError::JobCannotBeCancelled);
        }

        job.cancel();
        self.db.update_job_status(job.id, &job.status, job.progress).await?;

        // TODO: Si le job est en cours d'exécution, l'annuler

        Ok(())
    }

    /// Vérifier la compatibilité format/méthode
    fn is_compatible(
        &self,
        input_format: &ModelFormat,
        quantization_method: &QuantizationMethod,
        output_format: &ModelFormat,
    ) -> bool {
        match quantization_method {
            QuantizationMethod::Int8 => {
                matches!(input_format, ModelFormat::Onnx) &&
                matches!(output_format, ModelFormat::Onnx)
            }
            QuantizationMethod::Gptq | QuantizationMethod::Awq => {
                matches!(input_format, ModelFormat::PyTorch | ModelFormat::Safetensors) &&
                matches!(output_format, ModelFormat::PyTorch | ModelFormat::Safetensors)
            }
            QuantizationMethod::GgufQ4_0 | QuantizationMethod::GgufQ5_0 => {
                matches!(input_format, ModelFormat::PyTorch | ModelFormat::Safetensors) &&
                matches!(output_format, ModelFormat::Gguf)
            }
        }
    }

    /// Calculer le coût en crédits d'un job
    async fn calculate_job_cost(
        &self,
        user_id: Uuid,
        method: &QuantizationMethod,
        file_metadata: &FileMetadata,
    ) -> Result<i32> {
        // Obtenir l'abonnement de l'utilisateur
        let subscription = self.db.get_user_subscription(user_id).await?;
        
        let base_cost = match method {
            QuantizationMethod::Int8 => 1,
            QuantizationMethod::Gptq => 2,
            QuantizationMethod::Awq => 2,
            QuantizationMethod::GgufQ4_0 | QuantizationMethod::GgufQ5_0 => 1,
        };

        // Ajuster selon la taille du modèle
        let size_factor = match file_metadata.parameter_count {
            Some(params) if params > 70.0 => 3, // Modèles très larges
            Some(params) if params > 13.0 => 2, // Modèles larges
            _ => 1, // Modèles standards
        };

        let total_cost = base_cost * size_factor;

        // Vérifier les crédits disponibles
        let credits = self.db.get_user_credits(user_id).await?;
        if credits < total_cost {
            return Err(AppError::InsufficientCredits);
        }

        Ok(total_cost)
    }

    /// Obtenir les statistiques des jobs
    pub async fn get_job_stats(&self, user_id: Option<Uuid>) -> Result<JobStats> {
        self.db.get_job_stats(user_id).await
    }

    /// Démarrer le worker de traitement des jobs
    pub async fn start_worker(&self, interval_seconds: u64) {
        let interval = tokio::time::Duration::from_secs(interval_seconds);
        
        loop {
            if let Err(e) = self.process_next_job().await {
                eprintln!("Erreur dans le worker: {}", e);
            }
            
            tokio::time::sleep(interval).await;
        }
    }
}

impl Clone for JobService {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            queue: self.queue.clone(),
            storage: self.storage.clone(),
            quantizer: self.quantizer.clone(),
            max_concurrent_jobs: self.max_concurrent_jobs,
            active_jobs: RwLock::new(Vec::new()),
        }
    }
}

/// Statistiques des jobs
pub struct JobStats {
    pub total: i64,
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub average_duration_seconds: f64,
}