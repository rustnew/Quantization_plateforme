

pub mod gptq;
pub mod awq;


use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn, error};
use std::time::Duration;

use crate::infrastructure::error::{AppError, AppResult};

/// Runtime Python sécurisé
#[derive(Clone)]
pub struct PythonRuntime {
    pool: Arc<Mutex<Vec<Python>>>,
}

impl PythonRuntime {
    /// Crée une nouvelle instance du runtime Python
    pub fn new() -> AppResult<Self> {
        info!("🔧 Initialisation du runtime Python...");
        
        // Préparer l'environnement Python
        pyo3::prepare_freethreaded_python();
        
        // Créer un pool initial de runtimes
        let pool = (0..3).map(|_| Python::with_gil(|py| py.clone())).collect();
        
        info!("✅ Runtime Python initialisé avec 3 workers");
        
        Ok(Self {
            pool: Arc::new(Mutex::new(pool)),
        })
    }

    /// Exécute une fonction Python avec timeout
    pub async fn execute_with_timeout<F, T>(&self, timeout: Duration, func: F) -> AppResult<T>
    where
        F: FnOnce(Python) -> PyResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let pool = self.pool.clone();
        let mut pool_guard = pool.lock().await;
        
        if pool_guard.is_empty() {
            // Recréer un runtime si le pool est vide
            pool_guard.push(Python::with_gil(|py| py.clone()));
        }
        
        let py = pool_guard.pop().unwrap();
        let result = tokio::time::timeout(timeout, async move {
            let result = Python::with_gil(|py| func(py));
            pool_guard.push(py);
            result
        }).await;
        
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(AppError::PythonError(format!("Erreur Python: {}", e))),
            Err(_) => Err(AppError::Timeout("Timeout d'exécution Python".to_string())),
        }
    }

    /// Test de connexion GPTQ
    pub async fn test_gptq_connection(&self) -> AppResult<bool> {
        self.execute_with_timeout(Duration::from_secs(10), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            py.import("auto_gptq")?;
            Ok(true)
        }).await
    }

    /// Test de connexion AWQ
    pub async fn test_awq_connection(&self) -> AppResult<bool> {
        self.execute_with_timeout(Duration::from_secs(10), |py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("append", ("./python/libs",))?;
            
            py.import("auto_awq")?;
            Ok(true)
        }).await
    }

    /// Exécute un script Python personnalisé
    pub async fn execute_script(&self, script: &str, args: Option<&PyDict>) -> AppResult<serde_json::Value> {
        self.execute_with_timeout(Duration::from_secs(30), |py| {
            let globals = PyDict::new(py);
            let locals = PyDict::new(py);
            
            // Exécuter le script
            py.run(script, Some(globals), Some(locals))?;
            
            // Convertir le résultat en JSON
            let result = locals.get_item("result")?;
            if let Some(result) = result {
                let json_str = result.call_method0("json")?;
                Ok(serde_json::from_str(&json_str.to_string())?)
            } else {
                Ok(serde_json::json!({}))
            }
        }).await
    }

    /// Version test pour les tests unitaires
    #[cfg(test)]
    pub fn new_test() -> Self {
        Self {
            pool: Arc::new(Mutex::new(vec![])),
        }
    }
}
