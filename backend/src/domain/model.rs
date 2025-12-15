
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fmt; // Correction 1: Importer le module fmt

/// Framework supportés pour les modèles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum ModelFramework {
    Pytorch,
    Onnx,
    Tensorflow,
}

/// Architecture de modèle supportée
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum ModelArchitecture {
    Llama,
    Mistral,
    Gemma,
    Deepseek,
    Transformer,
    Cnn,
    Other,
}

/// Méthode de quantification supportée (Correction 2: Implémenter Display)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum QuantizationMethod {
    Int8,
    Int4,
    Gptq,
    Awq,
    Dynamic,
}

// Correction 3: Implémenter le trait Display pour QuantizationMethod
impl fmt::Display for QuantizationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuantizationMethod::Int8 => write!(f, "int8"),
            QuantizationMethod::Int4 => write!(f, "int4"),
            QuantizationMethod::Gptq => write!(f, "gptq"),
            QuantizationMethod::Awq => write!(f, "awq"),
            QuantizationMethod::Dynamic => write!(f, "dynamic"),
        }
    }
}

/// Représente les métadonnées d'un modèle IA
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelMetadata {
    /// Identifiant unique des métadonnées
    pub id: Uuid,
    /// ID de l'utilisateur propriétaire
    pub user_id: Uuid,
    /// ID du job associé
    pub job_id: Uuid,
    /// Nom du modèle
    pub name: String,
    /// Version du modèle
    pub version: String,
    /// Description optionnelle
    pub description: Option<String>,
    /// Framework utilisé
    pub framework: ModelFramework,
    /// Architecture du modèle
    pub architecture: ModelArchitecture,
    /// Nombre de paramètres (peut être nul pour les petits modèles)
    pub parameters_count: Option<i64>,
    /// Taille originale en bytes
    pub original_size_bytes: i64,
    /// Taille après quantification en bytes
    pub quantized_size_bytes: i64,
    /// Méthode de quantification utilisée
    pub quantization_method: String,
    /// Tags pour catégorisation
    pub tags: HashMap<String, String>,
    /// Métriques de qualité
    pub quality_metrics: Option<serde_json::Value>,
    /// Date de création
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour
    pub updated_at: DateTime<Utc>,
}

/// Données requises pour créer de nouvelles métadonnées
#[derive(Debug, Clone, Deserialize)]
pub struct NewModelMetadata {
    pub user_id: Uuid,
    pub job_id: Uuid,
    pub name: String,
    pub version: String,
    pub framework: ModelFramework,
    pub architecture: ModelArchitecture,
    pub parameters_count: Option<i64>,
    pub original_size_bytes: i64,
    pub quantized_size_bytes: i64,
    pub quantization_method: String,
}

/// Rapport de quantification détaillé
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationReport {
    pub model_name: String,
    pub original_size_mb: f64,
    pub quantized_size_mb: f64,
    pub reduction_percent: f32,
    pub quality_loss_percent: Option<f32>,
    pub latency_improvement_percent: Option<f32>,
    pub estimated_cost_savings_percent: Option<f32>,
    pub quantization_method: String,
    pub processing_time_seconds: i32,
}

impl ModelMetadata {
    /// Crée de nouvelles métadonnées à partir d'un job complété
    pub fn from_completed_job(
        job: &super::jobs::Job,
        user_id: Uuid,
        framework: ModelFramework,
        architecture: ModelArchitecture,
        parameters_count: Option<i64>,
        quality_metrics: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            job_id: job.id,
            name: job.model_name.clone(),
            version: "1.0".to_string(),
            description: None,
            framework,
            architecture,
            parameters_count,
            original_size_bytes: job.original_size_bytes,
            quantized_size_bytes: job.quantized_size_bytes.unwrap_or(0),
            // Correction 4: Utiliser implémentation Display au lieu de to_string()
            quantization_method: format!("{:?}", job.quantization_method).to_lowercase(),
            tags: HashMap::new(),
            quality_metrics,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Génère un rapport de quantification formaté
    pub fn generate_report(&self) -> QuantizationReport {
        let original_size_mb = self.original_size_bytes as f64 / 1_000_000.0;
        let quantized_size_mb = self.quantized_size_bytes as f64 / 1_000_000.0;
        let reduction_percent = if self.original_size_bytes > 0 {
            ((self.original_size_bytes - self.quantized_size_bytes) as f64 
             / self.original_size_bytes as f64 * 100.0) as f32
        } else {
            0.0
        };

        QuantizationReport {
            model_name: self.name.clone(),
            original_size_mb,
            quantized_size_mb,
            reduction_percent,
            quality_loss_percent: self.extract_quality_metric("quality_loss_percent"),
            latency_improvement_percent: self.extract_quality_metric("latency_improvement_percent"),
            estimated_cost_savings_percent: self.extract_quality_metric("cost_savings_percent"),
            // Correction 5: Utiliser la méthode to_lowercase() correctement
            quantization_method: self.quantization_method.clone(),
            processing_time_seconds: 0, // À remplir par le service de jobs
        }
    }

    /// Extrait une métrique spécifique des quality_metrics
    fn extract_quality_metric(&self, metric_name: &str) -> Option<f32> {
        if let Some(metrics) = &self.quality_metrics {
            if let Some(number) = metrics.get(metric_name).and_then(|v| v.as_f64()) {
                return Some(number as f32);
            }
        }
        None
    }

    /// Ajoute un tag aux métadonnées
    pub fn add_tag(&mut self, key: String, value: String) {
        self.tags.insert(key, value);
    }

    /// Ajoute plusieurs tags d'un coup
    pub fn add_tags(&mut self, tags: HashMap<String, String>) {
        self.tags.extend(tags);
    }
    
    /// Marque les métadonnées comme complétées
    pub fn complete(&mut self, quantized_size_bytes: i64, _output_path: String) { // Correction 6: Ajouter underscore pour variable non utilisée
        self.quantized_size_bytes = quantized_size_bytes;
        self.updated_at = Utc::now();
    }
}
