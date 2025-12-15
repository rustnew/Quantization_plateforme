

use std::time::Duration;
use tracing::{info, warn, error};

use crate::infrastructure::database::{Database, JobsRepository};
use crate::infrastructure::storage::StorageService;
use crate::infrastructure::error::AppResult;

/// Configuration du worker de nettoyage
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// Durée de rétention des fichiers temporaires (heures)
    pub temp_files_retention_hours: u32,
    /// Durée de rétention des jobs (jours)
    pub jobs_retention_days: u32,
    /// Taille maximale du cache (Go)
    pub max_cache_size_gb: u32,
    /// Intervalle entre les cycles de nettoyage (secondes)
    pub interval_seconds: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            temp_files_retention_hours: 24,
            jobs_retention_days: 30,
            max_cache_size_gb: 100,
            interval_seconds: 300, // 5 minutes
        }
    }
}

/// Worker de nettoyage background
pub struct CleanupWorker {
    config: CleanupConfig,
    db: Database,
    storage: StorageService,
}

impl CleanupWorker {
    /// Crée une nouvelle instance du worker
    pub fn new(
        config: CleanupConfig,
        db: Database,
        storage: StorageService,
    ) -> Self {
        Self {
            config,
            db,
            storage,
        }
    }

    /// Démarre le worker en boucle infinie
    pub async fn start(mut self) -> ! {
        info!("🔧 Worker de nettoyage démarré avec config: {:?}", self.config);
        
        loop {
            match self.run_cleanup_cycle().await {
                Ok(_) => {
                    info!("✅ Cycle de nettoyage terminé avec succès");
                },
                Err(e) => {
                    error!("❌ Erreur lors du cycle de nettoyage: {}", e);
                }
            }
            
            // Attendre avant le prochain cycle
            tokio::time::sleep(Duration::from_secs(self.config.interval_seconds)).await;
        }
    }

    /// Exécute un cycle complet de nettoyage
    async fn run_cleanup_cycle(&self) -> AppResult<()> {
        info!("🔄 Démarrage du cycle de nettoyage...");
        
        // 1. Nettoyer les fichiers temporaires anciens
        self.cleanup_temp_files().await?;
        
        // 2. Nettoyer les jobs anciens
        self.cleanup_old_jobs().await?;
        
        // 3. Nettoyer le cache
        self.cleanup_cache().await?;
        
        // 4. Vérifier l'espace disque
        self.check_disk_space().await?;
        
        info!("✅ Cycle de nettoyage terminé");
        Ok(())
    }

    /// Nettoyer les fichiers temporaires anciens
    async fn cleanup_temp_files(&self) -> AppResult<()> {
        let temp_dir = std::env::var("TEMP_DIR").unwrap_or_else(|_| "/tmp/quant_worker".to_string());
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(self.config.temp_files_retention_hours as i64);
        
        info!("🧹 Nettoyage des fichiers temporaires plus anciens que {} heures", self.config.temp_files_retention_hours);
        
        let mut cleaned_count = 0;
        let mut total_size = 0;
        
        for entry in std::fs::read_dir(&temp_dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            
            if metadata.is_file() {
                let modified = metadata.modified()?;
                if modified < cutoff_time {
                    let size = metadata.len();
                    std::fs::remove_file(&path)?;
                    cleaned_count += 1;
                    total_size += size;
                }
            }
        }
        
        info!("✅ {} fichiers temporaires nettoyés ({} Mo libérés)", 
              cleaned_count, total_size as f64 / 1_000_000.0);
        
        Ok(())
    }

    /// Nettoyer les jobs anciens
    async fn cleanup_old_jobs(&self) -> AppResult<()> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(self.config.jobs_retention_days as i64);
        
        info!("🧹 Nettoyage des jobs terminés plus anciens que {} jours", self.config.jobs_retention_days);
        
        let jobs_repo = JobsRepository::new(self.db.pool.clone());
        let deleted_count = jobs_repo.cleanup_old_jobs(cutoff_date).await?;
        
        info!("✅ {} jobs anciens nettoyés", deleted_count);
        Ok(())
    }

    /// Nettoyer le cache
    async fn cleanup_cache(&self) -> AppResult<()> {
        let cache_dir = std::env::var("CACHE_DIR").unwrap_or_else(|_| "/tmp/quant_cache".to_string());
        
        info!("🧹 Nettoyage du cache (taille maximale: {} Go)", self.config.max_cache_size_gb);
        
        // Obtenir la taille actuelle du cache
        let current_size = self.get_directory_size(&cache_dir)?;
        let max_size_bytes = (self.config.max_cache_size_gb as u64) * 1_000_000_000;
        
        if current_size > max_size_bytes {
            info!("⚠️  Cache trop volumineux ({} Mo > {} Mo), nettoyage nécessaire", 
                  current_size as f64 / 1_000_000.0, max_size_bytes as f64 / 1_000_000.0);
            
            // Supprimer les fichiers les plus anciens
            let mut files: Vec<_> = std::fs::read_dir(&cache_dir)?
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    let metadata = entry.metadata().ok()?;
                    if metadata.is_file() {
                        let modified = metadata.modified().ok()?;
                        Some((path, modified))
                    } else {
                        None
                    }
                })
                .collect();
            
            // Trier par date de modification (plus ancien en premier)
            files.sort_by_key(|(_, modified)| *modified);
            
            let mut freed_size = 0;
            let mut deleted_count = 0;
            
            for (path, _) in files {
                if current_size - freed_size <= max_size_bytes {
                    break;
                }
                
                let metadata = std::fs::metadata(&path)?;
                let file_size = metadata.len();
                
                std::fs::remove_file(&path)?;
                freed_size += file_size;
                deleted_count += 1;
            }
            
            info!("✅ {} fichiers de cache supprimés ({} Mo libérés)", 
                  deleted_count, freed_size as f64 / 1_000_000.0);
        }
        
        Ok(())
    }

    /// Obtenir la taille d'un répertoire
    fn get_directory_size(&self, path: &str) -> AppResult<u64> {
        let mut total_size = 0;
        
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_dir() {
                total_size += self.get_directory_size(&entry.path().to_string_lossy())?;
            } else if metadata.is_file() {
                total_size += metadata.len();
            }
        }
        
        Ok(total_size)
    }

    /// Vérifier l'espace disque disponible
    async fn check_disk_space(&self) -> AppResult<()> {
        let path = std::env::var("DATA_DIR").unwrap_or_else(|_| "/".to_string());
        let available_space = self.get_available_disk_space(&path)?;
        
        let warning_threshold = 10_000_000_000; // 10 Go
        let critical_threshold = 5_000_000_000;  // 5 Go
        
        if available_space < critical_threshold {
            error!("🚨 Espace disque critique: {} Mo restants", available_space as f64 / 1_000_000.0);
        } else if available_space < warning_threshold {
            warn!("⚠️  Espace disque faible: {} Mo restants", available_space as f64 / 1_000_000.0);
        } else {
            info!("✅ Espace disque disponible: {} Go", available_space as f64 / 1_000_000_000.0);
        }
        
        Ok(())
    }

    /// Obtenir l'espace disque disponible
    fn get_available_disk_space(&self, path: &str) -> AppResult<u64> {
        let metadata = std::fs::metadata(path)?;
        let stat = metadata.st_ctime()?;
        Ok(stat.st_free as u64)
    }
}

/// Démarrage du worker de nettoyage
pub async fn start_cleanup_worker(
    config: CleanupConfig,
    db: Database,
    storage: StorageService,
) -> AppResult<()> {
    info!("🔧 Initialisation du worker de nettoyage...");
    
    let worker = CleanupWorker::new(config, db, storage);
    
    // Démarrer dans une tâche Tokio séparée
    tokio::spawn(async move {
        worker.start().await;
    });
    
    info!("✅ Worker de nettoyage démarré avec succès");
    Ok(())
}
