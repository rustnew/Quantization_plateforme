

pub mod pipeline;
pub mod onnx;
pub mod pytorch;
pub mod gguf;
pub mod analysis;
pub mod validation;

pub use pipeline::QuantizationPipeline;
pub use analysis::ModelAnalyzer;
pub use validation::QualityValidator;
pub use onnx::OnnxQuantizer;
pub use pytorch::PyTorchQuantizer;
pub use gguf::GGUFExporter;

use crate::infrastructure::error::{AppError, AppResult};
use crate::domain::jobs::{Job, QuantizationMethod, JobStatus};


/// Configuration de la quantification
#[derive(Debug, Clone)]
pub struct QuantizationConfig {
    /// Méthode de quantification à utiliser
    pub method: QuantizationMethod,
    /// Nombre de bits pour la quantification (4 ou 8)
    pub bits: u8,
    /// Taille de groupe pour GPTQ/AWQ
    pub group_size: usize,
    /// Activation de la calibration statique
    pub use_calibration: bool,
    /// Chemin vers les données de calibration
    pub calibration_data_path: Option<String>,
    /// Formats de sortie demandés
    pub output_formats: Vec<String>,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            method: QuantizationMethod::Int8,
            bits: 8,
            group_size: 128,
            use_calibration: false,
            calibration_data_path: None,
            output_formats: vec!["onnx".to_string()],
        }
    }
}

/// Résultat d'une quantification
#[derive(Debug, Clone)]
pub struct QuantizationResult {
    /// Chemin vers le modèle quantifié
    pub quantized_path: String,
    /// Taille après quantification en bytes
    pub quantized_size_bytes: u64,
    /// Pourcentage de réduction
    pub reduction_percent: f32,
    /// Rapport de qualité
    pub quality_report: Option<serde_json::Value>,
    /// Formats générés
    pub formats: Vec<String>,
}

/// Erreurs de quantification
#[derive(Debug, thiserror::Error)]
pub enum QuantizationError {
    #[error("Modèle non supporté: {0}")]
    UnsupportedModel(String),
    #[error("Erreur ONNX Runtime: {0}")]
    OnnxError(#[from] ort::OrtError),
    #[error("Erreur Python: {0}")]
    PythonError(#[from] pyo3::PyErr),
    #[error("Erreur de fichier: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Erreur de validation: {0}")]
    ValidationError(String),
    #[error("Mémoire insuffisante pour la quantification")]
    OutOfMemory,
    #[error("Timeout de quantification")]
    Timeout,
    #[error("Erreur d'analyse du modèle: {0}")]
    AnalysisError(String),
}

impl From<QuantizationError> for AppError {
    fn from(error: QuantizationError) -> Self {
        match error {
            QuantizationError::UnsupportedModel(msg) => AppError::ValidationError(msg),
            QuantizationError::OnnxError(e) => AppError::InternalError(format!("ONNX error: {}", e)),
            QuantizationError::PythonError(e) => AppError::InternalError(format!("Python error: {}", e)),
            QuantizationError::FileError(e) => AppError::InternalError(format!("File error: {}", e)),
            QuantizationError::ValidationError(msg) => AppError::ValidationError(validator::ValidationError::new(&msg).into()),
            QuantizationError::OutOfMemory => AppError::ResourceExhausted("GPU/CPU memory exhausted".to_string()),
            QuantizationError::Timeout => AppError::Timeout("Quantization timed out".to_string()),
            QuantizationError::AnalysisError(msg) => AppError::ValidationError(msg),
        }
    }
}