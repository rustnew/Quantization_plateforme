use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;

/// État d'un job de quantification
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "job_status", rename_all = "snake_case")]
pub enum JobStatus {
    Pending,      // En attente dans la queue
    Processing,   // En cours de traitement
    Completed,    // Terminé avec succès
    Failed,       // Échec
    Cancelled,    // Annulé par l'utilisateur
}

/// Méthode de quantification
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "quantization_method", rename_all = "snake_case")]
pub enum QuantizationMethod {
    Int8,        // Quantification 8-bit
    Gptq,        // GPTQ 4-bit
    Awq,         // AWQ 4-bit
    GgufQ4_0,    // GGUF Q4_0
    GgufQ5_0,    // GGUF Q5_0
}

/// Format de modèle
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "model_format", rename_all = "snake_case")]
pub enum ModelFormat {
    PyTorch,
    Onnx,
    Safetensors,
    Gguf,
}

/// Un job de quantification
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Job {
    /// ID unique du job
    pub id: Uuid,
    
    /// ID de l'utilisateur propriétaire
    pub user_id: Uuid,
    
    /// Nom du job (donné par l'utilisateur)
    pub name: String,
    
    /// État actuel du job
    pub status: JobStatus,
    
    /// Progression (0-100)
    pub progress: i32,
    
    /// Méthode de quantification
    pub quantization_method: QuantizationMethod,
    
    /// Format du modèle source
    pub input_format: ModelFormat,
    
    /// Format du modèle de sortie
    pub output_format: ModelFormat,
    
    /// ID du fichier modèle source
    pub input_file_id: Uuid,
    
    /// ID du fichier modèle quantifié (optionnel)
    pub output_file_id: Option<Uuid>,
    
    /// Message d'erreur en cas d'échec
    pub error_message: Option<String>,
    
    /// Taille originale (octets)
    pub original_size: Option<i64>,
    
    /// Taille après quantification (octets)
    pub quantized_size: Option<i64>,
    
    /// Temps de traitement en secondes
    pub processing_time: Option<i32>,
    
    /// Crédits utilisés pour ce job
    pub credits_used: i32,
    
    /// Date de création
    pub created_at: DateTime<Utc>,
    
    /// Date de début de traitement
    pub started_at: Option<DateTime<Utc>>,
    
    /// Date de fin de traitement
    pub completed_at: Option<DateTime<Utc>>,
}

/// Pour créer un nouveau job
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewJob {
    #[validate(length(min = 1, max = 100, message = "Le nom doit faire entre 1 et 100 caractères"))]
    pub name: String,
    
    pub quantization_method: QuantizationMethod,
    pub output_format: ModelFormat,
}

/// Pour mettre à jour la progression d'un job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub progress: i32,
    pub status: JobStatus,
    pub error_message: Option<String>,
}

/// Pour le résultat d'un job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub id: Uuid,
    pub status: JobStatus,
    pub progress: i32,
    pub error_message: Option<String>,
    pub original_size: Option<i64>,
    pub quantized_size: Option<i64>,
    pub compression_ratio: Option<f64>,
    pub download_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Job {
    /// Crée un nouveau job
    pub fn new(
        user_id: Uuid,
        name: String,
        quantization_method: QuantizationMethod,
        input_format: ModelFormat,
        output_format: ModelFormat,
        input_file_id: Uuid,
        credits_used: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            name,
            status: JobStatus::Pending,
            progress: 0,
            quantization_method,
            input_format,
            output_format,
            input_file_id,
            output_file_id: None,
            error_message: None,
            original_size: None,
            quantized_size: None,
            processing_time: None,
            credits_used,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }
    
    /// Met à jour la progression
    pub fn update_progress(&mut self, progress: i32) {
        self.progress = progress.clamp(0, 100);
    }
    
    /// Démarre le traitement
    pub fn start(&mut self) {
        self.status = JobStatus::Processing;
        self.started_at = Some(Utc::now());
        self.progress = 10; // Démarrage
    }
    
    /// Termine avec succès
    pub fn complete(&mut self, output_file_id: Uuid, quantized_size: i64) {
        self.status = JobStatus::Completed;
        self.progress = 100;
        self.output_file_id = Some(output_file_id);
        self.quantized_size = Some(quantized_size);
        self.completed_at = Some(Utc::now());
        
        // Calcul du temps de traitement
        if let Some(started) = self.started_at {
            if let Some(completed) = self.completed_at {
                self.processing_time = Some((completed - started).num_seconds() as i32);
            }
        }
    }
    
    /// Marque comme échoué
    pub fn fail(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.error_message = Some(error);
        self.completed_at = Some(Utc::now());
    }
    
    /// Annule le job
    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
    
    /// Calcule le ratio de compression
    pub fn compression_ratio(&self) -> Option<f64> {
        if let (Some(original), Some(quantized)) = (self.original_size, self.quantized_size) {
            if original > 0 {
                Some(quantized as f64 / original as f64)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Convertit en résultat pour l'API
    pub fn to_result(&self, download_url: Option<String>) -> JobResult {
        JobResult {
            id: self.id,
            status: self.status.clone(),
            progress: self.progress,
            error_message: self.error_message.clone(),
            original_size: self.original_size,
            quantized_size: self.quantized_size,
            compression_ratio: self.compression_ratio(),
            download_url,
            created_at: self.created_at,
            completed_at: self.completed_at,
        }
    }
}