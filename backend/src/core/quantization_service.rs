// core/quantization_service.rs
use crate::models::{QuantizationMethod, ModelFormat};
use crate::utils::error::{AppError, Result};
use crate::services::python::PythonClient;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;

pub struct QuantizationService {
    python_client: Arc<PythonClient>,
    gpu_enabled: bool,
    timeout_seconds: u64,
    max_retries: u32,
    work_dir: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl QuantizationService {
    pub fn new(
        python_client: Arc<PythonClient>,
        gpu_enabled: bool,
        timeout_seconds: u64,
        max_retries: u32,
        work_dir: PathBuf,
        max_concurrent: usize,
    ) -> Self {
        Self {
            python_client,
            gpu_enabled,
            timeout_seconds,
            max_retries,
            work_dir,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Quantifier un modèle
    pub async fn quantize(
        &self,
        input_path: &str,
        method: &QuantizationMethod,
        output_format: &ModelFormat,
        job_id: Uuid,
    ) -> Result<String> {
        // Acquérir un permis pour limiter la concurrence
        let _permit = self.semaphore.acquire().await
            .map_err(|_| AppError::ResourceBusy)?;

        // Créer un répertoire de travail pour ce job
        let job_dir = self.work_dir.join(job_id.to_string());
        tokio::fs::create_dir_all(&job_dir).await?;

        // Copier le fichier d'entrée dans le répertoire de travail
        let input_filename = Path::new(input_path)
            .file_name()
            .ok_or(AppError::InvalidPath)?
            .to_string_lossy()
            .to_string();
        
        let job_input_path = job_dir.join(&input_filename);
        tokio::fs::copy(input_path, &job_input_path).await?;

        // Exécuter la quantification
        let output_path = self.execute_quantization(
            &job_input_path,
            method,
            output_format,
            &job_dir,
        ).await?;

        Ok(output_path)
    }

    /// Exécuter la quantification selon la méthode
    async fn execute_quantization(
        &self,
        input_path: &Path,
        method: &QuantizationMethod,
        output_format: &ModelFormat,
        output_dir: &Path,
    ) -> Result<String> {
        let input_path_str = input_path.to_string_lossy();
        let output_dir_str = output_dir.to_string_lossy();

        match method {
            QuantizationMethod::Int8 => {
                // Quantification INT8 pour ONNX
                self.python_client.call_script(
                    "quantize_int8.py",
                    &[
                        "--input", &input_path_str,
                        "--output-dir", &output_dir_str,
                        "--bits", "8",
                    ],
                ).await
            }
            QuantizationMethod::Gptq => {
                if !self.gpu_enabled {
                    return Err(AppError::GpuRequired);
                }
                
                // Quantification GPTQ 4-bit
                self.python_client.call_script(
                    "quantize_gptq.py",
                    &[
                        "--input", &input_path_str,
                        "--output-dir", &output_dir_str,
                        "--bits", "4",
                        "--group-size", "128",
                        "--damp-percent", "0.1",
                        "--act-order",
                    ],
                ).await
            }
            QuantizationMethod::Awq => {
                if !self.gpu_enabled {
                    return Err(AppError::GpuRequired);
                }
                
                // Quantification AWQ 4-bit
                self.python_client.call_script(
                    "quantize_awq.py",
                    &[
                        "--input", &input_path_str,
                        "--output-dir", &output_dir_str,
                        "--bits", "4",
                        "--group-size", "128",
                        "--zero-point",
                    ],
                ).await
            }
            QuantizationMethod::GgufQ4_0 => {
                // Conversion en GGUF Q4_0
                self.convert_to_gguf(&input_path_str, output_dir, "q4_0").await
            }
            QuantizationMethod::GgufQ5_0 => {
                // Conversion en GGUF Q5_0
                self.convert_to_gguf(&input_path_str, output_dir, "q5_0").await
            }
        }
    }

    /// Convertir en format GGUF
    async fn convert_to_gguf(
        &self,
        input_path: &str,
        output_dir: &Path,
        quantization: &str,
    ) -> Result<String> {
        let output_path = output_dir.join("model.gguf");
        let output_path_str = output_path.to_string_lossy();

        // Utiliser llama.cpp ou un script Python
        self.python_client.call_script(
            "convert_gguf.py",
            &[
                "--input", input_path,
                "--output", &output_path_str,
                "--quantization", quantization,
            ],
        ).await?;

        Ok(output_path_str.to_string())
    }

    /// Analyser un modèle pour extraire des métadonnées
    pub async fn analyze_model(&self, model_path: &str) -> Result<ModelAnalysis> {
        let result = self.python_client.call_script(
            "analyze_model.py",
            &["--model", model_path],
        ).await?;

        // Parser le résultat JSON
        let analysis: ModelAnalysis = serde_json::from_str(&result)
            .map_err(|e| AppError::ParseError(e.to_string()))?;

        Ok(analysis)
    }

    /// Vérifier la santé du service Python
    pub async fn health_check(&self) -> Result<()> {
        // Vérifier que Python est accessible
        let output = Command::new("python3")
            .arg("--version")
            .output()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if !output.status.success() {
            return Err(AppError::ExternalService("Python non disponible".to_string()));
        }

        // Vérifier les dépendances
        let deps_ok = self.check_python_dependencies().await?;
        if !deps_ok {
            return Err(AppError::ExternalService("Dépendances Python manquantes".to_string()));
        }

        Ok(())
    }

    /// Vérifier les dépendances Python
    async fn check_python_dependencies(&self) -> Result<bool> {
        let scripts = [
            "quantize_int8.py",
            "quantize_gptq.py",
            "quantize_awq.py",
            "convert_gguf.py",
            "analyze_model.py",
        ];

        for script in &scripts {
            let script_path = self.python_client.scripts_dir.join(script);
            if !script_path.exists() {
                eprintln!("Script manquant: {}", script);
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Nettoyer les fichiers temporaires
    pub async fn cleanup_old_files(&self, max_age_days: i64) -> Result<u64> {
        let mut deleted = 0;
        
        if let Ok(entries) = tokio::fs::read_dir(&self.work_dir).await {
            let mut entries = tokio_stream::wrappers::ReadDirStream::new(entries);
            
            while let Some(entry) = entries.next().await {
                if let Ok(entry) = entry {
                    let metadata = entry.metadata().await.ok();
                    let is_old = metadata
                        .and_then(|m| m.modified().ok())
                        .map(|modified| {
                            let age = std::time::SystemTime::now()
                                .duration_since(modified)
                                .unwrap_or_default();
                            age.as_secs() > (max_age_days as u64 * 24 * 60 * 60)
                        })
                        .unwrap_or(false);

                    if is_old {
                        let _ = tokio::fs::remove_dir_all(entry.path()).await;
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }
}

impl Clone for QuantizationService {
    fn clone(&self) -> Self {
        Self {
            python_client: self.python_client.clone(),
            gpu_enabled: self.gpu_enabled,
            timeout_seconds: self.timeout_seconds,
            max_retries: self.max_retries,
            work_dir: self.work_dir.clone(),
            semaphore: self.semaphore.clone(),
        }
    }
}

/// Analyse d'un modèle
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ModelAnalysis {
    pub model_type: String,
    pub architecture: String,
    pub parameter_count: f64, // en milliards
    pub quantization_bits: Option<i32>,
    pub layers: i32,
    pub vocab_size: Option<i32>,
    pub context_length: Option<i32>,
    pub file_size_bytes: u64,
    pub supported_quantizations: Vec<String>,
}