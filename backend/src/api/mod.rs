// api/mod.rs
pub mod auth;
pub mod user;
pub mod job;
pub mod file;
pub mod billing;
pub mod admin;

use actix_web::{web, HttpResponse};

/// Configure toutes les routes API
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            // Authentification
            .configure(auth::configure_routes)
            // Utilisateurs
            .configure(user::configure_routes)
            // Jobs
            .configure(job::configure_routes)
            // Fichiers
            .configure(file::configure_routes)
            // Facturation
            .configure(billing::configure_routes)
            // Admin (nécessite authentification admin)
            .configure(admin::configure_routes),
    );
}

/// Middleware pour extraire l'utilisateur authentifié
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: uuid::Uuid,
    pub email: String,
}

/// Type de résultat standard pour les handlers
pub type ApiResult<T> = Result<T, actix_web::Error>;