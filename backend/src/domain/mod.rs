//! # Domain Models Module
//! 
//! Ce module contient tous les modèles de données principaux de l'application.
//! Ces modèles représentent les entités métier et sont utilisés à travers
//! toute l'application (API, services, base de données).
//! 
//! ## Structure
//! - `user.rs`: Modèle pour les utilisateurs authentifiés
//! - `job.rs`: Modèle pour les jobs de quantification  
//! - `model.rs`: Modèle pour les métadonnées des modèles IA
//! 
//! ## Conventions
//! - Tous les modèles implémentent `serde::Serialize` et `serde::Deserialize`
//! - Les champs sensibles sont exclus de la sérialisation JSON
//! - Les identifiants utilisent `uuid::Uuid` pour éviter les conflits
//! - Les timestamps utilisent `chrono::DateTime<chrono::Utc>` pour l'uniformité
//! - Les énumérations utilisent des variants explicites pour la sécurité

pub mod user;
pub mod jobs;    // Note: Le fichier s'appelle jobs.rs mais représente un seul job
pub mod model;

// Ré-export des types principaux pour une utilisation facile
pub use user::User;
pub use jobs::Job;
pub use model::ModelMetadata;