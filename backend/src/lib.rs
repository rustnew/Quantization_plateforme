// backend/src/lib.rs
// Modules principaux
pub mod models;
pub mod api;
pub mod core;
pub mod services;
pub mod utils;

// Ré-exports pour faciliter l'utilisation
pub use models::*;
pub use api::*;
pub use core::*;
pub use services::*;
pub use utils::*;

// Version de l'application
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = "Quantization Platform";

// Types communs
pub use uuid::Uuid;
pub use chrono::{DateTime, Utc};
pub use serde::{Serialize, Deserialize};
pub use validator::Validate;

// Initialisation de l'application
pub async fn init_app() -> Result<()> {
    // Cette fonction peut être utilisée pour les tests
    Ok(())
}

// Configuration par défaut pour les tests
#[cfg(test)]
pub mod test_utils {
    use super::*;
    use std::sync::Once;
    
    static INIT: Once = Once::new();
    
    pub fn init_test_logging() {
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_test_writer()
                .init();
        });
    }
    
    pub async fn create_test_database() -> Result<Database> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://test:test@localhost:5432/test".to_string());
        
        let db = Database::new(&database_url).await?;
        
        // Nettoyer et créer les tables
        // Note: En vrai, on utiliserait des migrations de test
        Ok(db)
    }
}