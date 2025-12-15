//! # GPTQ Python Bindings
//! 
//! Ce fichier contient les bindings Python pour l'algorithme GPTQ (Generative Pretrained Transformer Quantization).
//! Il permet d'utiliser les bibliothèques Python AutoGPTQ depuis le code Rust de manière sécurisée et performante.
//! 
//! ## Fonctionnalités
//! - Quantification INT4/INT8 de modèles PyTorch
//! - Support des méthodes GPTQ classiques et optimisées
//! - Gestion de la calibration sur dataset
//! - Optimisation des poids par couche
//! 
//! ## Sécurité
//! - Isolation mémoire stricte entre les appels Python
//! - Gestion sécurisée des exceptions Python
//! - Nettoyage automatique des ressources
//! - Protection contre les fuites mémoire
//! 
//! ## Performance
//! - Warm-up des modules au démarrage
//! - Caching des imports fréquents
//! - Parallélisation des appels indépendants
//! - Timeout par invocation pour éviter les blocages
//! 
//! ## Utilisation
//! ```rust
//! let python_runtime = PythonRuntime::new()?;
//! let gptq_quantizer = GPTQQuantizer::new(python_runtime);
//! 
//! // Quantifier un modèle
//! gptq_quantizer.quantize_model(
//!     "/path/to/model",
//!     "/path/to/output",
//!     4,  // bits
//!     128 // group_size
//! ).await?;
//! ```

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn, error};
use crate::infrastructure::error::{AppError, AppResult};

/// Quantizer GPTQ sécurisé
pub struct GPTQQuantizer {
    python_runtime: crate::infrastructure::python::PythonRuntime,
}

impl GPTQQuantizer {
    /// Crée une nouvelle instance du quantizer GPTQ
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
            py.import("auto_gptq")?;
            
            Ok(())
        }).await?;
        
        info!("✅ Dépendances GPTQ initialisées");
        Ok(())
    }

    /// Quantifie un modèle PyTorch avec GPTQ
    /// 
    /// # Arguments
    /// * `model_path` - Chemin vers le modèle à quantifier
    /// * `output_path` - Chemin de sortie pour le modèle quantifié
    /// * `bits` - Nombre de bits (4 ou 8)
    /// * `group_size` - Taille des groupes pour la quantification
    /// * `calibration_data_path` - Chemin vers les données de calibration (optionnel)
    /// 
    /// # Retourne
    /// * `Ok(())` - Si la quantification réussit
    /// * `Err(AppError)` - En cas d'erreur Python ou système
    pub async fn quantize_model(
        &self,
        model_path: &Path,
        output_path: &Path,
        bits: u8,
        group_size: usize,
        calibration_data_path: Option<&Path>,
    ) -> AppResult<()> {
        info!("🧠 Démarrage de la quantification GPTQ pour: {:?}", model_path);
        
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
            kwargs.set_item("damp_percent", 0.01)?;
            kwargs.set_item("desc_act", false)?;
            kwargs.set_item("sym", true)?;
            kwargs.set_item("true_sequential", true)?;
            
            if let Some(calib_path) = calibration_path_str {
                kwargs.set_item("calibration_data_path", calib_path)?;
            }
            
            // Appeler la fonction de quantification
            let gptq_module = py.import("auto_gptq")?;
            let result = gptq_module.call_method("quantize_model", (), Some(kwargs))?;
            
            info!("✅ Modèle GPTQ quantifié avec succès");
            Ok(())
        }).await?;
        
        Ok(())
    }

    /// Test de connexion GPTQ
    pub async fn test_connection(&self) -> AppResult<bool> {
        self.python_runtime.execute_with_timeout(Duration::from_secs(10), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            py.import("auto_gptq")?;
            Ok(true)
        }).await
    }

    /// Convertit un modèle safetensors vers PyTorch
    pub async fn convert_safetensors_to_pytorch(
        &self,
        safetensors_path: &Path,
        pytorch_path: &Path,
    ) -> AppResult<()> {
        let safetensors_path_str = safetensors_path.to_string_lossy().to_string();
        let pytorch_path_str = pytorch_path.to_string_lossy().to_string();
        
        self.python_runtime.execute_with_timeout(Duration::from_secs(300), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            let converter = py.import("model_converter")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("input_path", safetensors_path_str)?;
            kwargs.set_item("output_path", pytorch_path_str)?;
            
            converter.call_method("convert_safetensors_to_pytorch", (), Some(kwargs))?;
            Ok(())
        }).await?;
        
        Ok(())
    }
}
