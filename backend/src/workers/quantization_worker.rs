

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::task;
use uuid::Uuid;
use tracing::{info, warn, error, debug, instrument};
use serde::{Serialize, Deserialize};
use chrono::Utc;

use crate::{
    infrastructure::database::{
        Database,
        JobsRepository,
        SubscriptionsRepository,
        UserRepository,
    },
    infrastructure::storage::StorageService,
    infrastructure::python::PythonRuntime,
    infrastructure::error::{AppError, AppResult},
    core::quantization::{
        QuantizationPipeline,
        QuantizationConfig,
        QuantizationMethod as CoreQuantizationMethod,
    },
    domain::job::{Job, JobStatus, QuantizationMethod},
};

/// Configuration du worker
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    /// Nombre maximum de jobs simultanés
    pub max_concurrent_jobs: usize,
    /// Intervalle entre les polling de jobs (secondes)
    pub poll_interval_seconds: u64,
    /// Timeout maximum par job (minutes)
    pub job_timeout_minutes: u64,
    /// Nombre maximum de tentatives par job
    pub max_retries: usize,
    /// Répertoire temporaire pour les fichiers
    pub temp_dir: String,
    /// Activer le mode debug (logs verbeux)
    pub debug_mode: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: 2, // Par défaut sur les petites instances
            poll_interval_seconds: 5,
            job_timeout_minutes: 120,
            max_retries: 3,
            temp_dir: "/tmp/quant_worker".to_string(),
            debug_mode: false,
        }
    }
}

/// Worker principal pour le traitement des jobs
pub struct QuantizationWorker {
    config: WorkerConfig,
    db: Database,
    storage: StorageService,
    python_runtime: PythonRuntime,
    // Semaphore pour contrôler les jobs simultanés
    concurrency_limiter: Arc<Semaphore>,
    // Mutex pour éviter les accès concurrents au même job
    active_jobs: Arc<Mutex<std::collections::HashSet<Uuid>>>,
}

impl QuantizationWorker {
    /// Crée une nouvelle instance du worker
    pub fn new(
        config: WorkerConfig,
        db: Database,
        storage: StorageService,
        python_runtime: PythonRuntime,
    ) -> Self {
        Self {
            config: config.clone(),
            db,
            storage,
            python_runtime,
            concurrency_limiter: Arc::new(Semaphore::new(config.max_concurrent_jobs)),
            active_jobs: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// Démarre le worker en boucle infinie
    pub async fn start(mut self) -> ! {
        info!("🚀 Worker de quantification démarré avec config: {:?}", self.config);
        info!("⚡ {} jobs simultanés maximum", self.config.max_concurrent_jobs);
        info!("⏱️  Intervalle de polling: {} secondes", self.config.poll_interval_seconds);
        
        // Créer le répertoire temporaire si nécessaire
        let temp_path = PathBuf::from(&self.config.temp_dir);
        if !temp_path.exists() {
            if let Err(e) = std::fs::create_dir_all(&temp_path) {
                error!("❌ Impossible de créer le répertoire temporaire {}: {}", self.config.temp_dir, e);
            } else {
                info!("✅ Répertoire temporaire créé: {}", self.config.temp_dir);
            }
        }
        
        let mut last_heartbeat = Instant::now();
        
        loop {
            // Heartbeat toutes les 30 secondes pour vérifier que le worker est actif
            if last_heartbeat.elapsed() > Duration::from_secs(30) {
                info!("💓 Worker heartbeat - {} jobs actifs", self.active_jobs.lock().await.len());
                last_heartbeat = Instant::now();
                
                // Vérifier les jobs potentiellement bloqués
                self.check_stuck_jobs().await;
            }
            
            // Poller les jobs en attente
            match self.poll_and_process_jobs().await {
                Ok(processed) if processed > 0 => {
                    debug!("✅ {} jobs traités lors de ce cycle", processed);
                },
                Ok(_) => {
                    // Aucun job à traiter - attente avec backoff
                    tokio::time::sleep(Duration::from_secs(self.config.poll_interval_seconds)).await;
                },
                Err(e) => {
                    error!("❌ Erreur lors du polling des jobs: {}", e);
                    // Attente plus longue en cas d'erreur pour éviter la saturation
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }
        }
    }

    /// Poll la base de données pour les jobs en attente et les traite
    async fn poll_and_process_jobs(&self) -> AppResult<usize> {
        // Récupérer les jobs en attente (status = 'queued')
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        let queued_jobs = jobs_repo.get_by_status(JobStatus::Queued, 10, 0).await?;
        
        if queued_jobs.is_empty() {
            return Ok(0);
        }
        
        debug!("🔍 {} jobs en attente trouvés", queued_jobs.len());
        
        let mut processed_count = 0;
        
        // Traiter chaque job avec un système de priorité
        for job in queued_jobs {
            // Vérifier si le job est déjà en cours de traitement
            let mut active_jobs = self.active_jobs.lock().await;
            if active_jobs.contains(&job.id) {
                debug!("⏸️  Job {} déjà en cours de traitement", job.id);
                continue;
            }
            
            // Ajouter le job à la liste des jobs actifs
            active_jobs.insert(job.id);
            drop(active_jobs); // Libérer le lock
            
            // Obtenir un permis pour exécuter le job
            let permit = match self.concurrency_limiter.clone().acquire().await {
                Ok(permit) => permit,
                Err(e) => {
                    warn!("⚠️  Impossible d'obtenir un permis de concurrence: {}", e);
                    // Remettre le job dans la liste des actifs
                    let mut active_jobs = self.active_jobs.lock().await;
                    active_jobs.insert(job.id);
                    continue;
                }
            };
            
            processed_count += 1;
            
            // Traiter le job dans une tâche séparée
            let worker_clone = self.clone();
            let job_clone = job.clone();
            
            task::spawn(async move {
                // Démarrer le traitement avec timeout
                let timeout = Duration::from_secs(worker_clone.config.job_timeout_minutes * 60);
                
                match tokio::time::timeout(timeout, worker_clone.process_single_job(job_clone)).await {
                    Ok(result) => {
                        match result {
                            Ok(_) => {
                                debug!("✅ Job {} traité avec succès", job_clone.id);
                            },
                            Err(e) => {
                                error!("❌ Échec du job {}: {}", job_clone.id, e);
                            }
                        }
                    },
                    Err(_) => {
                        error!("⏰ Job {} a expiré après {} minutes", job_clone.id, worker_clone.config.job_timeout_minutes);
                        // Marquer le job comme échoué
                        let jobs_repo = JobsRepository::new(worker_clone.db.pool.clone());
                        let _ = jobs_repo.fail_job(&job_clone.id, format!(
                            "Job expiré après {} minutes", worker_clone.config.job_timeout_minutes
                        )).await;
                    }
                }
                
                // Libérer les ressources
                let mut active_jobs = worker_clone.active_jobs.lock().await;
                active_jobs.remove(&job_clone.id);
                drop(active_jobs);
                drop(permit);
            });
        }
        
        Ok(processed_count)
    }

    /// Traite un seul job de quantification
    #[instrument(skip_all, fields(job_id = %job.id, user_id = %job.user_id, model_name = %job.model_name))]
    async fn process_single_job(&self, job: Job) -> AppResult<()> {
        info!("🔄 Traitement du job {} pour l'utilisateur {}", job.id, job.user_id);
        let start_time = Instant::now();
        
        // 1. Mettre à jour le statut en "processing"
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        jobs_repo.update_status(&job.id, JobStatus::Processing).await?;
        
        // 2. Télécharger le modèle depuis le stockage
        let input_path = self.storage.download_file(&job.input_path).await?;
        info!("📥 Modèle téléchargé: {:?}, taille: {} Mo", input_path, 
              std::fs::metadata(&input_path)?.len() as f64 / 1_000_000.0);
        
        // 3. Créer le répertoire de sortie
        let output_dir = PathBuf::from(&self.config.temp_dir).join(job.id.to_string());
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir)?;
        }
        std::fs::create_dir_all(&output_dir)?;
        
        // 4. Configurer la quantification
        let quant_method = match job.quantization_method {
            QuantizationMethod::Int8 => CoreQuantizationMethod::Int8,
            QuantizationMethod::Int4 => CoreQuantizationMethod::Int4,
            QuantizationMethod::Gptq => CoreQuantizationMethod::Gptq,
            QuantizationMethod::Awq => CoreQuantizationMethod::Awq,
            _ => CoreQuantizationMethod::Int8,
        };
        
        let config = QuantizationConfig {
            method: quant_method.clone(),
            bits: match quant_method {
                CoreQuantizationMethod::Int8 => 8,
                _ => 4,
            },
            group_size: 128,
            use_calibration: true,
            calibration_data_path: Some("/app/data/calibration_data".to_string()),
            output_formats: vec!["onnx".to_string(), "gguf".to_string()],
        };
        
        // 5. Exécuter le pipeline de quantification
        info!("⚙️  Démarrage de la quantification {}...", quant_method);
        
        match quant_method {
            CoreQuantizationMethod::Int8 => {
                self.quantize_onnx(&job, &input_path, &output_dir, &config).await?;
            },
            CoreQuantizationMethod::Int4 | 
            CoreQuantizationMethod::Gptq | 
            CoreQuantizationMethod::Awq => {
                self.quantize_pytorch(&job, &input_path, &output_dir, &config).await?;
            },
            _ => {
                return Err(AppError::BadRequest("Méthode de quantification non supportée".to_string()));
            }
        }
        
        // 6. Calculer le temps de traitement
        let processing_time = start_time.elapsed().as_secs() as i32;
        let processing_time_str = format!("{:.1} minutes", processing_time as f32 / 60.0);
        info!("✅ Job {} complété en {}", job.id, processing_time_str);
        
        // 7. Générer le rapport détaillé
        self.generate_and_save_report(&job, processing_time).await?;
        
        // 8. Nettoyer les fichiers temporaires
        self.cleanup_temp_files(&input_path, &output_dir).await?;
        
        Ok(())
    }

    /// Quantifie un modèle ONNX
    async fn quantize_onnx(
        &self,
        job: &Job,
        input_path: &PathBuf,
        output_dir: &PathBuf,
        config: &QuantizationConfig,
    ) -> AppResult<()> {
        // Créer le pipeline de quantification
        let pipeline = QuantizationPipeline::new(
            self.db.clone(),
            self.storage.clone(),
            self.python_runtime.clone(),
        );
        
        // Exécuter la quantification
        let result = pipeline.quantize_onnx(input_path, output_dir, config).await?;
        
        // Upload du résultat
        let output_path = PathBuf::from(&result.quantized_path);
        let download_url = self.storage.upload_file(&output_path).await?;
        
        // Mettre à jour le job
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        jobs_repo.complete_job(
            &job.id,
            result.quantized_size_bytes as i64,
            download_url,
        ).await?;
        
        info!("📤 Résultat ONNX uploadé: {}", result.quantized_path);
        
        Ok(())
    }

    /// Quantifie un modèle PyTorch
    async fn quantize_pytorch(
        &self,
        job: &Job,
        input_path: &PathBuf,
        output_dir: &PathBuf,
        config: &QuantizationConfig,
    ) -> AppResult<()> {
        // Créer le pipeline de quantification
        let pipeline = QuantizationPipeline::new(
            self.db.clone(),
            self.storage.clone(),
            self.python_runtime.clone(),
        );
        
        // Exécuter la quantification
        let result = pipeline.quantize_pytorch(input_path, output_dir, config).await?;
        
        // Upload du résultat
        let output_path = PathBuf::from(&result.quantized_path);
        let download_url = self.storage.upload_file(&output_path).await?;
        
        // Mettre à jour le job
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        jobs_repo.complete_job(
            &job.id,
            result.quantized_size_bytes as i64,
            download_url,
        ).await?;
        
        info!("📤 Résultat PyTorch uploadé: {}", result.quantized_path);
        
        Ok(())
    }

    /// Génère et sauvegarde le rapport de quantification
    async fn generate_and_save_report(&self, job: &Job, processing_time: i32) -> AppResult<()> {
        // Récupérer le job mis à jour
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        let completed_job = jobs_repo.get_by_id(&job.id).await?;
        
        // Générer le rapport
        let reduction_percent = completed_job.reduction_percent.unwrap_or(0.0);
        let original_size_gb = completed_job.original_size_bytes as f64 / 1_073_741_824.0;
        let quantized_size_gb = completed_job.quantized_size_bytes.unwrap_or(0) as f64 / 1_073_741_824.0;
        
        // Estimer les économies de coûts
        let cost_savings_percent = if completed_job.quantization_method == QuantizationMethod::Int8 {
            40.0
        } else {
            70.0
        };
        
        let report = serde_json::json!({
            "job_id": job.id,
            "user_id": job.user_id,
            "model_name": job.model_name,
            "quantization_method": format!("{:?}", completed_job.quantization_method),
            "original_size_gb": original_size_gb,
            "quantized_size_gb": quantized_size_gb,
            "reduction_percent": reduction_percent,
            "processing_time_seconds": processing_time,
            "quality_loss_percent": 0.8, // Valeur temporaire - à remplacer par une validation réelle
            "latency_improvement_percent": 65.0, // Valeur temporaire
            "estimated_cost_savings_percent": cost_savings_percent,
            "download_url": completed_job.download_url,
            "created_at": Utc::now(),
            "hardware_recommendations": {
                "minimum_ram_gb": if reduction_percent > 70.0 { 8 } else { 16 },
                "recommended_gpu": if completed_job.quantization_method == QuantizationMethod::Int8 { 
                    "RTX 3060" 
                } else { 
                    "RTX 3090 ou supérieur" 
                }
            }
        });
        
        // Sauvegarder dans la base de données
        let query = sqlx::query!(
            r#"
            INSERT INTO quantization_reports (
                job_id, original_perplexity, quantized_perplexity, 
                quality_loss_percent, latency_improvement_percent, cost_savings_percent
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            job.id,
            15.8, // perplexity originale temporaire
            16.2, // perplexity quantifiée temporaire
            0.8,  // perte de qualité temporaire
            65.0, // amélioration latence temporaire
            cost_savings_percent
        );
        
        query.execute(&*self.db.pool).await?;
        
        // Logger le rapport
        info!("📊 Rapport de quantification généré pour le job {}:\n{}", job.id, serde_json::to_string_pretty(&report)?);
        
        Ok(())
    }

    /// Vérifie les jobs potentiellement bloqués
    async fn check_stuck_jobs(&self) {
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        
        // Récupérer les jobs en traitement depuis plus de 30 minutes
        let cutoff_time = Utc::now() - chrono::Duration::minutes(30);
        
        match sqlx::query_as!(
            Job,
            r#"
            SELECT 
                id, user_id, model_name, original_size_bytes, quantized_size_bytes,
                quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
                status::VARCHAR as "status: JobStatus",
                error_message, reduction_percent, download_url,
                created_at, updated_at
            FROM jobs
            WHERE status = $1 AND updated_at < $2
            "#,
            JobStatus::Processing.to_string(),
            cutoff_time
        )
        .fetch_all(&self.db.pool)
        .await
        {
            Ok(stuck_jobs) if !stuck_jobs.is_empty() => {
                warn!("⚠️  {} jobs potentiellement bloqués détectés", stuck_jobs.len());
                
                for job in stuck_jobs {
                    error!("🔍 Investigating stuck job: {}", job.id);
                    
                    // Essayer de marquer comme échoué
                    let _ = jobs_repo.fail_job(&job.id, "Job bloqué - timeout détection".to_string()).await;
                    
                    // Nettoyer les ressources
                    let temp_dir = PathBuf::from(&self.config.temp_dir).join(job.id.to_string());
                    if temp_dir.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
                            warn!("⚠️  Impossible de nettoyer {} pour le job {}: {}", temp_dir.display(), job.id, e);
                        }
                    }
                }
            },
            Ok(_) => {
                // Aucun job bloqué
            },
            Err(e) => {
                error!("❌ Erreur lors de la vérification des jobs bloqués: {}", e);
            }
        }
    }

    /// Nettoie les fichiers temporaires
    async fn cleanup_temp_files(&self, input_path: &PathBuf, output_dir: &PathBuf) -> AppResult<()> {
        if self.config.debug_mode {
            debug!("🔍 Mode debug activé - pas de nettoyage des fichiers temporaires");
            return Ok(());
        }
        
        // Supprimer le fichier d'entrée téléchargé
        if input_path.exists() {
            if let Err(e) = std::fs::remove_file(input_path) {
                warn!("⚠️  Impossible de supprimer {}: {}", input_path.display(), e);
            } else {
                debug!("🗑️  Fichier d'entrée supprimé: {}", input_path.display());
            }
        }
        
        // Supprimer le répertoire de sortie
        if output_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(output_dir) {
                warn!("⚠️  Impossible de supprimer {}: {}", output_dir.display(), e);
            } else {
                debug!("🗑️  Répertoire de sortie supprimé: {}", output_dir.display());
            }
        }
        
        Ok(())
    }
}

/// Initialisation du worker au démarrage de l'application
pub async fn start_worker_background(
    config: WorkerConfig,
    db: Database,
    storage: StorageService,
    python_runtime: PythonRuntime,
) -> AppResult<()> {
    info!("🔧 Initialisation du worker background...");
    
    let worker = QuantizationWorker::new(
        config,
        db,
        storage,
        python_runtime,
    );
    
    // Démarrer dans une tâche Tokio séparée
    tokio::spawn(async move {
        worker.start().await;
    });
    
    info!("✅ Worker background démarré avec succès");
    Ok(())
}
