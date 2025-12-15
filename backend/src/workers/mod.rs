//! # Workers Module
//! 
//! Ce module contient tous les workers background qui s'exécutent
//! de manière asynchrone pour traiter des tâches longues :
//! - `quantization_worker.rs`: Traitement des jobs de quantification
//! - `cleanup_worker.rs`: Nettoyage des fichiers temporaires et anciens jobs
//! 
//! ## Architecture
//! Les workers utilisent le pattern Actor avec :
//! - Une boucle infinie avec polling
//! - Gestion robuste des erreurs
//! - Heartbeat pour détection des pannes
//! - Resource limits par worker
//! 
//! ## Scalabilité
//! - Configuration du nombre de workers par type
//! - Priorisation des jobs importants
//! - Auto-scaling basé sur la charge
//! - Distribution sur plusieurs machines
//! 
//! ## Monitoring
//! - Logging structuré pour chaque étape
//! - Métriques Prometheus pour le monitoring
//! - Alertes pour les jobs en retard
//! - Rapports de performance journaliers

pub mod quantization_worker;
pub mod cleanup_worker;

pub use quantization_worker::QuantizationWorker;
pub use cleanup_worker::CleanupWorker;