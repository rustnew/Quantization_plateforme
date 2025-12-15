

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn, error};
use crate::infrastructure::error::{AppError, AppResult};

/// Quantizer AWQ sécurisé
pub struct AWQQuantizer {
    python_runtime: crate::infrastructure::python::PythonRuntime,
}

impl AWQQuantizer {
    /// Crée une nouvelle instance du quantizer AWQ
    pub fn new(python_runtime: crate::infrastructure::python::PythonRuntime) -> Self {
        Self { python_runtime }
    }

    /// Initialise les dépendances Python nécessaires
    async fn initialize_dependencies(&self) -> AppResult<()> {
        self.python_runtime.execute_with_timeout(Duration::from_secs(10), |py| {
            // Ajouter le chemin des libs Python au sys.path
            let sys = py.import("sys")?;
            let path = sys.getattr("path")?;
            path.call_method1("append", ("./python/libs",))?;
            
            // Importer les dépendances
            py.import("torch")?;
            py.import("transformers")?;
            py.import("auto_awq")?;
            
            Ok(())
        }).await?;
        
        info!("✅ Dépendances AWQ initialisées");
        Ok(())
    }

    
    pub async fn quantize_model(
        &self,
        model_path: &Path,
        output_path: &Path,
        bits: u8,
        group_size: usize,
        calibration_data_path: Option<&Path>,
    ) -> AppResult<()> {
        info!("⚡ Démarrage de la quantification AWQ pour: {:?}", model_path);
        
        // Initialiser les dépendances
        self.initialize_dependencies().await?;
        
        let model_path_str = model_path.to_string_lossy().to_string();
        let output_path_str = output_path.to_string_lossy().to_string();
        let calibration_path_str = calibration_data_path.map(|p| p.to_string_lossy().to_string());
        
        self.python_runtime.execute_with_timeout(Duration::from_secs(3600), |py| {
            // Créer un dictionnaire de configuration
            let kwargs = PyDict::new(py);
            kwargs.set_item("model_path", model_path_str)?;
            kwargs.set_item("output_path", output_path_str)?;
            kwargs.set_item("bits", bits as i32)?;
            kwargs.set_item("group_size", group_size as i32)?;
            kwargs.set_item("zero_point", true)?;
            kwargs.set_item("version", "gemm")?;
            
            if let Some(calib_path) = calibration_path_str {
                kwargs.set_item("calibration_data_path", calib_path)?;
            }
            
            // Appeler la fonction de quantification
            let awq_module = py.import("auto_awq")?;
            let result = awq_module.call_method("quantize_model", (), Some(kwargs))?;
            
            info!("✅ Modèle AWQ quantifié avec succès");
            Ok(())
        }).await?;
        
        Ok(())
    }

    /// Test de connexion AWQ
    pub async fn test_connection(&self) -> AppResult<bool> {
        self.python_runtime.execute_with_timeout(Duration::from_secs(10), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            py.import("auto_awq")?;
            Ok(true)
        }).await
    }

    /// Compare deux modèles pour la précision
    pub async fn compare_models(
        &self,
        model1_path: &Path,
        model2_path: &Path,
        test_data_path: &Path,
    ) -> AppResult<serde_json::Value> {
        let model1_path_str = model1_path.to_string_lossy().to_string();
        let model2_path_str = model2_path.to_string_lossy().to_string();
        let test_data_path_str = test_data_path.to_string_lossy().to_string();
        
        let result = self.python_runtime.execute_with_timeout(Duration::from_secs(600), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let comparator = py.import("model_comparator")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("model1_path", model1_path_str)?;
            kwargs.set_item("model2_path", model2_path_str)?;
            kwargs.set_item("test_data_path", test_data_path_str)?;
            
            let result = comparator.call_method("compare_models", (), Some(kwargs))?;
            let json_str: String = result.extract()?;
            let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
            
            Ok(json_value)
        }).await?;
        
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_awq_initialization() {
        let python_runtime = crate::infrastructure::python::PythonRuntime::new_test();
        let awq_quantizer = AWQQuantizer::new(python_runtime);
        
        // Tester la connexion
        let result = awq_quantizer.test_connection().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_model_comparison() {
        let temp_dir = tempdir().unwrap();
        let model1_path = temp_dir.path().join("model1.pt");
        let model2_path = temp_dir.path().join("model2.pt");
        let test_data_path = temp_dir.path().join("test_data.json");
        
        // Créer des fichiers de test
        File::create(&model1_path).unwrap().write_all(b"model1 content").unwrap();
        File::create(&model2_path).unwrap().write_all(b"model2 content").unwrap();
        File::create(&test_data_path).unwrap().write_all(b"test data").unwrap();
        
        let python_runtime = crate::infrastructure::python::PythonRuntime::new_test();
        let awq_quantizer = AWQQuantizer::new(python_runtime);
        
        // Tester la comparaison
        let result = awq_quantizer.compare_models(&model1_path, &model2_path, &test_data_path).await;
        assert!(result.is_ok());
        
        let report = result.unwrap();
        assert!(report.is_object());
    }
}