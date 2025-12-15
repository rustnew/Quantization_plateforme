

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Statut d'un job de quantification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus::Queued
    }
}

/// Méthode de quantification supportée
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum QuantizationMethod {
    Int8,
    Int4,
    Gptq,
    Awq,
    Dynamic,
}

impl Default for QuantizationMethod {
    fn default() -> Self {
        QuantizationMethod::Int8
    }
}

/// Représente un job de quantification
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Job {
    /// Identifiant unique du job
    pub id: Uuid,
    /// ID de l'utilisateur propriétaire
    pub user_id: Uuid,
    /// Nom du modèle (pour l'affichage)
    pub model_name: String,
    /// Nom du fichier original
    pub file_name: String,
    /// Chemin du fichier d'entrée
    pub input_path: String,
    /// Chemin du fichier de sortie après quantification
    pub output_path: String,
    /// Taille originale en bytes
    pub original_size_bytes: i64,
    /// Taille après quantification en bytes
    pub quantized_size_bytes: Option<i64>,
    /// Méthode de quantification utilisée
    pub quantization_method: QuantizationMethod,
    /// Statut actuel du job
    pub status: JobStatus,
    /// Message d'erreur si le job a échoué
    pub error_message: Option<String>,
    /// Pourcentage de réduction de taille
    pub reduction_percent: Option<f32>,
    /// Token sécurisé pour le téléchargement du résultat
    pub download_token: String,
    /// URL sécurisée pour le téléchargement du résultat
    pub download_url: Option<String>,
    /// Date de création du job
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour
    pub updated_at: DateTime<Utc>,
}

/// Données requises pour créer un nouveau job
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewJob {
    pub user_id: Uuid,
    pub model_name: String,
    pub file_name: String,
    pub original_size_bytes: i64,
    pub quantization_method: QuantizationMethod,
}

impl Job {
    /// Crée un nouveau job avec un token de téléchargement unique
    pub fn new(user_id: Uuid, model_name: String, file_name: String, original_size_bytes: i64, method: QuantizationMethod) -> Self {
        let download_token = Uuid::new_v4().to_string() + &chrono::Utc::now().timestamp().to_string();
        
        Self {
            id: Uuid::new_v4(),
            user_id,
            model_name,
            file_name,
            input_path: format!("/tmp/input/{}", Uuid::new_v4()),
            output_path: format!("/tmp/output/{}", Uuid::new_v4()),
            original_size_bytes,
            quantized_size_bytes: None,
            quantization_method: method,
            status: JobStatus::Queued,
            error_message: None,
            reduction_percent: None,
            download_token,
            download_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Met à jour le statut du job
    pub fn update_status(&mut self, new_status: JobStatus) {
        self.status = new_status;
        self.updated_at = Utc::now();
    }

    /// Marque le job comme complété avec les résultats
    pub fn complete(&mut self, quantized_size_bytes: i64, output_path: String, download_url: String) {
        self.quantized_size_bytes = Some(quantized_size_bytes);
        self.output_path = output_path;
        self.download_url = Some(download_url);
        self.reduction_percent = Some(self.calculate_reduction());
        self.update_status(JobStatus::Completed);
    }

    /// Marque le job comme échoué avec un message d'erreur
    pub fn fail(&mut self, error_message: String) {
        self.error_message = Some(error_message);
        self.update_status(JobStatus::Failed);
    }

    /// Annule le job
    pub fn cancel(&mut self) {
        self.update_status(JobStatus::Cancelled);
    }

    /// Calcule le pourcentage de réduction de taille
    fn calculate_reduction(&self) -> f32 {
        if let Some(quantized_size) = self.quantized_size_bytes {
            if self.original_size_bytes > 0 {
                let reduction = (self.original_size_bytes as f64 - quantized_size as f64) 
                               / self.original_size_bytes as f64;
                (reduction * 100.0) as f32
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Génère l'URL de téléchargement sécurisée
    pub fn download_url(&self, base_url: &str) -> String {
        format!("{}/api/v1/jobs/{}/download?token={}", base_url, self.id, self.download_token)
    }

    /// Vérifie si le token de téléchargement est valide
    pub fn validate_download_token(&self, token: &str) -> bool {
        self.download_token == token
    }

    /// Retourne le nom du fichier quantifié
    pub fn quantized_filename(&self) -> String {
        let extension = match self.quantization_method {
            QuantizationMethod::Int8 => "int8",
            QuantizationMethod::Int4 | QuantizationMethod::Gptq | QuantizationMethod::Awq => "int4",
            QuantizationMethod::Dynamic => "dynamic",
        };
        
        format!("{}_{}.onnx", 
                self.file_name.trim_end_matches(".onnx"),
                extension)
    }
}
