//! # Model Analysis
//! 
//! Ce module analyse les modèles IA avant quantification pour déterminer:
//! - L'architecture et le nombre de paramètres
//! - La compatibilité avec les méthodes de quantification
//! - Les caractéristiques d'activation pour AWQ vs GPTQ
//! - Les recommandations de configuration optimale
//! 
//! ## Méthodes d'analyse
//! - **ONNX**: Lecture des métadonnées et analyse de la structure du graphe
//! - **PyTorch**: Chargement du modèle et inspection des couches
//! - **Statistiques**: Distribution des poids, sparsité, etc.
//! 
//! ## Résultats fournis
//! - `parameter_count`: Nombre total de paramètres
//! - `size_mb`: Taille du modèle en Mo
//! - `architecture`: Type d'architecture (Llama, Mistral, etc.)
//! - `supports_quantization`: Compatibilité avec la quantification
//! - `activation_sparsity`: Sparsité des activations (pour AWQ)
//! - `recommended_bits`: Nombre de bits recommandé
//! - `recommended_method`: Méthode recommandée (GPTQ vs AWQ)

use std::path::Path;
use ort::{Session, GraphOptimizationLevel};
use tracing::info;
use pyo3::prelude::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelAnalysis {
    pub parameter_count: u64,
    pub size_mb: f32,
    pub architecture: String,
    pub supports_quantization: bool,
    pub activation_sparsity: f32,
    pub recommended_bits: u8,
    pub recommended_method: String,
    pub layer_types: Vec<String>,
    pub input_shapes: Vec<Vec<usize>>,
    pub output_shapes: Vec<Vec<usize>>,
}

pub struct ModelAnalyzer;

impl ModelAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyse un modèle ONNX
    pub async fn analyze_onnx(model_path: &Path) -> Result<ModelAnalysis, anyhow::Error> {
        info!("🔍 Analyse du modèle ONNX: {:?}", model_path);
        
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_model_from_file(model_path)?;
        
        let metadata = session.metadata()?;
        let input_count = session.inputs.len();
        let output_count = session.outputs.len();
        
        // Estimation du nombre de paramètres
        let parameter_count = Self::estimate_parameters_from_session(&session)?;
        let file_size = std::fs::metadata(model_path)?.len() as f32 / 1_000_000.0;
        
        // Détection de l'architecture
        let architecture = Self::detect_architecture_from_metadata(&metadata)?;
        
        Ok(ModelAnalysis {
            parameter_count,
            size_mb: file_size,
            architecture,
            supports_quantization: true,
            activation_sparsity: 0.0,
            recommended_bits: 8,
            recommended_method: "dynamic".to_string(),
            layer_types: Self::get_layer_types(&session)?,
            input_shapes: session.inputs.iter()
                .map(|input| input.shape().iter().map(|dim| dim as usize).collect())
                .collect(),
            output_shapes: session.outputs.iter()
                .map(|output| output.shape().iter().map(|dim| dim as usize).collect())
                .collect(),
        })
    }

    /// Analyse un modèle PyTorch
    pub async fn analyze_pytorch(model_path: &Path) -> Result<ModelAnalysis, anyhow::Error> {
        info!("🔍 Analyse du modèle PyTorch: {:?}", model_path);
        
        Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let analyzer = py.import("model_analyzer")?;
            let kwargs = pyo3::types::PyDict::new(py);
            kwargs.set_item("model_path", model_path.to_string_lossy().to_string())?;
            kwargs.set_item("device", "cpu")?;
            
            let result: PyObject = analyzer.call_method("analyze_model", (), Some(kwargs))?;
            let analysis: ModelAnalysis = result.extract(py)?;
            
            Ok(analysis)
        })
    }

    /// Estime le nombre de paramètres à partir d'une session ONNX
    fn estimate_parameters_from_session(session: &Session) -> Result<u64, anyhow::Error> {
        // Implémentation simplifiée pour le MVP
        // Dans la vraie version, on analyserait les poids et biais
        let input_size: usize = session.inputs.iter()
            .map(|input| input.shape().iter().product::<usize>())
            .sum();
        
        let output_size: usize = session.outputs.iter()
            .map(|output| output.shape().iter().product::<usize>())
            .sum();
        
        // Heuristique grossière: 10x la taille d'entrée+sortie
        Ok((input_size + output_size) as u64 * 10)
    }

    /// Détecte l'architecture à partir des métadonnées
    fn detect_architecture_from_metadata(metadata: &ort::SessionMetadata) -> Result<String, anyhow::Error> {
        let model_name = metadata.name.clone().unwrap_or_default().to_lowercase();
        
        if model_name.contains("llama") || model_name.contains("mistral") {
            Ok("llama".to_string())
        } else if model_name.contains("gemma") {
            Ok("gemma".to_string())
        } else if model_name.contains("deepseek") {
            Ok("deepseek".to_string())
        } else {
            Ok("unknown".to_string())
        }
    }

    /// Récupère les types de couches
    fn get_layer_types(session: &Session) -> Result<Vec<String>, anyhow::Error> {
        // Pour le MVP, on retourne des types génériques
        Ok(vec!["Linear".to_string(), "LayerNorm".to_string(), "Attention".to_string()])
    }

    /// Analyse les statistiques des poids
    pub fn analyze_weight_statistics(weights: &[f32]) -> (f32, f32, f32) {
        if weights.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        
        let min = weights.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = weights.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let mean = weights.iter().sum::<f32>() / weights.len() as f32;
        
        (min, max, mean)
    }

    /// Calcule la sparsité des activations
    pub fn calculate_activation_sparsity(activations: &[f32]) -> f32 {
        if activations.is_empty() {
            return 0.0;
        }
        
        let zero_count = activations.iter().filter(|&&x| x.abs() < 1e-6).count();
        zero_count as f32 / activations.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::fs::File;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_onnx_analysis() {
        // Créer un fichier ONNX de test (simulé)
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        
        File::create(&path).unwrap().write_all(b"ONNX model content").unwrap();
        
        let analysis = ModelAnalyzer::analyze_onnx(&path).await;
        assert!(analysis.is_ok());
        
        let result = analysis.unwrap();
        assert!(result.supports_quantization);
        assert_eq!(result.recommended_bits, 8);
    }
}