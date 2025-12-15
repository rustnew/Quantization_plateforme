

pub mod jobs;
pub mod users;
pub mod subscriptions;


use sqlx::{Pool, Postgres, Error as SqlxError};
use std::sync::Arc;
use tracing::info;

/// Gestion de la connexion à la base de données
#[derive(Clone)]
pub struct Database {
    pub pool: Arc<Pool<Postgres>>,
}

impl Database {
    /// Crée une nouvelle connexion à la base de données
    pub async fn new(database_url: &str) -> Result<Self, SqlxError> {
        info!("🔌 Connexion à la base de données PostgreSQL...");
        
        let pool = Pool::connect(database_url).await?;
        info!("✅ Connexion établie avec succès");
        
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// Création pour les tests (utilise une connexion existante)
    #[cfg(test)]
    pub fn new_with_pool(pool: Pool<Postgres>) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Création mock pour les tests
    #[cfg(test)]
    pub fn new_test() -> Self {
        use sqlx::postgres::PgPoolOptions;
        
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://test:test@localhost/test")
            .expect("Impossible de créer le pool de test");
            
        Self {
            pool: Arc::new(pool),
        }
    }
}
