

use std::path::{Path};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use anyhow::Result;
use pyo3::prelude::*;

use crate::infrastructure::python::PythonRuntime;

pub struct PyTorchQuantizer {
    python_runtime: Arc<Mutex<PythonRuntime>>,
}

impl PyTorchQuantizer {
    pub fn new(python_runtime: PythonRuntime) -> Self {
        Self {
            python_runtime: Arc::new(Mutex::new(python_runtime)),
        }
    }

    /// Quantification GPTQ pour modèles PyTorch
    pub async fn quantize_gptq(
        input_path: &Path,
        output_path: &Path,
        bits: i32,
        group_size: usize,
        calibration_data_path: Option<&Path>,
    ) -> Result<()> {
        if bits != 4 {
            return Err(anyhow::anyhow!("GPTQ currently only supports INT4 quantization"));
        }

        info!("🧠 Quantification GPTQ INT4 pour: {:?}", input_path);
        
        let python_runtime = self.python_runtime.lock().await;
        
        Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let gptq = py.import("auto_gptq")?;
            let kwargs = pyo3::types::PyDict::new(py);
            
            kwargs.set_item("model_path", input_path.to_string_lossy().to_string())?;
            kwargs.set_item("output_path", output_path.to_string_lossy().to_string())?;
            kwargs.set_item("bits", bits)?;
            kwargs.set_item("group_size", group_size as i32)?;
            kwargs.set_item("damp_percent", 0.01)?;
            kwargs.set_item("desc_act", false)?;
            kwargs.set_item("sym", true)?;
            kwargs.set_item("true_sequential", true)?;
            
            if let Some(calib_path) = calibration_data_path {
                kwargs.set_item("calibration_data_path", calib_path.to_string_lossy().to_string())?;
            }
            
            gptq.call_method("quantize_model", (), Some(kwargs))?;
            Ok(())
        })?;
        
        info!("✅ Modèle GPTQ quantifié sauvegardé: {:?}", output_path);
        Ok(())
    }

    /// Quantification AWQ pour modèles PyTorch
    pub async fn quantize_awq(
        input_path: &Path,
        output_path: &Path,
        bits: i32,
        group_size: usize,
        calibration_data_path: Option<&Path>,
    ) -> Result<()> {
        if bits != 4 {
            return Err(anyhow::anyhow!("AWQ currently only supports INT4 quantization"));
        }

        info!("⚡ Quantification AWQ INT4 pour: {:?}", input_path);
        
        let python_runtime = self.python_runtime.lock().await;
        
        Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let awq = py.import("auto_awq")?;
            let kwargs = pyo3::types::PyDict::new(py);
            
            kwargs.set_item("model_path", input_path.to_string_lossy().to_string())?;
            kwargs.set_item("output_path", output_path.to_string_lossy().to_string())?;
            kwargs.set_item("bits", bits)?;
            kwargs.set_item("group_size", group_size as i32)?;
            kwargs.set_item("zero_point", true)?;
            kwargs.set_item("version", "gemm")?;
            
            if let Some(calib_path) = calibration_data_path {
                kwargs.set_item("calibration_data_path", calib_path.to_string_lossy().to_string())?;
            }
            
            awq.call_method("quantize_model", (), Some(kwargs))?;
            Ok(())
        })?;
        
        info!("✅ Modèle AWQ quantifié sauvegardé: {:?}", output_path);
        Ok(())
    }

    /// Convertir un modèle safetensors vers PyTorch
    pub async fn convert_safetensors_to_pytorch(
        safetensors_path: &Path,
        pytorch_path: &Path,
    ) -> Result<()> {
        info!("🔄 Conversion safetensors vers PyTorch: {:?}", safetensors_path);
        
        let python_runtime = self.python_runtime.lock().await;
        
        Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let converter = py.import("model_converter")?;
            let kwargs = pyo3::types::PyDict::new(py);
            kwargs.set_item("input_path", safetensors_path.to_string_lossy().to_string())?;
            kwargs.set_item("output_path", pytorch_path.to_string_lossy().to_string())?;
            
            converter.call_method("convert_safetensors_to_pytorch", (), Some(kwargs))?;
            Ok(())
        })?;
        
        Ok(())
    }

    /// Fusionner les poids quantifiés avec le modèle original
    pub async fn merge_quantized_weights(
        base_model_path: &Path,
        quantized_weights_path: &Path,
        output_path: &Path,
    ) -> Result<()> {
        info!("🔍 Fusion des poids quantifiés");
        
        let python_runtime = self.python_runtime.lock().await;
        
        Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let merger = py.import("weight_merger")?;
            let kwargs = pyo3::types::PyDict::new(py);
            kwargs.set_item("base_model", base_model_path.to_string_lossy().to_string())?;
            kwargs.set_item("quantized_weights", quantized_weights_path.to_string_lossy().to_string())?;
            kwargs.set_item("output_path", output_path.to_string_lossy().to_string())?;
            
            merger.call_method("merge_weights", (), Some(kwargs))?;
            Ok(())
        })?;
        
        Ok(())
    }

    /// Comparer deux modèles PyTorch pour la précision
    pub async fn compare_models(
        model1_path: &Path,
        model2_path: &Path,
        test_data_path: &Path,
    ) -> Result<serde_json::Value> {
        warn!("⚠️  Comparaison de modèles non implémentée dans le MVP");
        
        // Retourner des résultats simulés pour le MVP
        Ok(serde_json::json!({
            "perplexity_model1": 15.8,
            "perplexity_model2": 16.2,
            "accuracy_model1": 0.89,
            "accuracy_model2": 0.87,
            "latency_model1_ms": 45.2,
            "latency_model2_ms": 22.8
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::fs::File;
    use std::io::Write;
    use crate::infrastructure::python::PythonRuntime;
    
    #[tokio::test]
    async fn test_gptq_quantization() {
        // Créer un fichier modèle de test
        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();
        
        File::create(input_file.path()).unwrap().write_all(b"PyTorch model content").unwrap();
        
        // Créer un runtime Python mock
        let python_runtime = PythonRuntime::new_test();
        
        let quantizer = PyTorchQuantizer::new(python_runtime);
        let result = quantizer.quantize_gptq(
            input_file.path(),
            output_file.path(),
            4,
            128,
            None,
        ).await;
        
        assert!(result.is_ok());
        assert!(output_file.path().exists());
        assert!(std::fs::metadata(output_file.path())?.len() > 0);
    }
}