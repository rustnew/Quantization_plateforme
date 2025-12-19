// api/admin.rs
use crate::models::{SystemMetrics, HealthStatus, PaginatedResponse};
use crate::api::AuthenticatedUser;
use crate::core::system_service::SystemService;
use actix_web::{web, HttpResponse, Responder};

/// Middleware pour vérifier les permissions admin
fn require_admin(user: &AuthenticatedUser) -> Result<(), actix_web::Error> {
    // Dans le MVP, on peut avoir une liste d'admins en dur
    // En production, on utiliserait un système de rôles
    let admin_emails = vec![
        "admin@quantization.com",
        // Ajouter d'autres emails admin
    ];
    
    if admin_emails.contains(&user.email.as_str()) {
        Ok(())
    } else {
        Err(actix_web::error::ErrorForbidden("Accès admin requis"))
    }
}

/// Configure les routes admin
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/admin")
            .wrap(crate::api::auth_middleware::require_auth())
            // Santé du système
            .route("/health", web::get().to(get_health))
            // Métriques système
            .route("/metrics", web::get().to(get_metrics))
            // Statistiques
            .route("/stats", web::get().to(get_stats))
            // Utilisateurs (admin)
            .route("/users", web::get().to(list_users))
            .route("/users/{user_id}", web::get().to(get_user))
            .route("/users/{user_id}", web::delete().to(delete_user))
            // Jobs (admin)
            .route("/jobs", web::get().to(list_all_jobs))
            .route("/jobs/{job_id}", web::get().to(get_job_details))
            .route("/jobs/{job_id}/retry", web::post().to(retry_job))
            // Logs d'audit
            .route("/audit-logs", web::get().to(get_audit_logs)),
    );
}

/// Obtenir la santé du système
async fn get_health(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_system_health().await {
        Ok(health_status) => HttpResponse::Ok().json(health_status),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir les métriques système
async fn get_metrics(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_system_metrics().await {
        Ok(metrics) => HttpResponse::Ok().json(metrics),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir les statistiques
async fn get_stats(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_system_stats().await {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Lister tous les utilisateurs (admin)
async fn list_users(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    query: web::Query<AdminListQuery>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.list_users(
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(50),
        query.search.as_deref(),
    ).await {
        Ok(users) => {
            let total = users.len() as i64;
            let response = PaginatedResponse {
                items: users,
                total,
                page: query.page.unwrap_or(1),
                per_page: query.per_page.unwrap_or(50),
                total_pages: (total as f64 / query.per_page.unwrap_or(50) as f64).ceil() as i64,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir les détails d'un utilisateur (admin)
async fn get_user(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    user_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_user_details(*user_id).await {
        Ok(user_details) => HttpResponse::Ok().json(user_details),
        Err(e) => {
            match e {
                crate::utils::error::AppError::UserNotFound => {
                    HttpResponse::NotFound().json("Utilisateur non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Supprimer un utilisateur (admin)
async fn delete_user(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    user_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    // Empêcher l'auto-suppression
    if user.id == *user_id {
        return HttpResponse::BadRequest().json("Vous ne pouvez pas supprimer votre propre compte");
    }
    
    match system_service.delete_user(*user_id).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            match e {
                crate::utils::error::AppError::UserNotFound => {
                    HttpResponse::NotFound().json("Utilisateur non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Lister tous les jobs (admin)
async fn list_all_jobs(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    query: web::Query<AdminJobQuery>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.list_all_jobs(
        query.status.as_deref(),
        query.user_id,
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(50),
    ).await {
        Ok(jobs) => {
            let total = jobs.len() as i64;
            let response = PaginatedResponse {
                items: jobs,
                total,
                page: query.page.unwrap_or(1),
                per_page: query.per_page.unwrap_or(50),
                total_pages: (total as f64 / query.per_page.unwrap_or(50) as f64).ceil() as i64,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir les détails d'un job (admin)
async fn get_job_details(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_job_details(*job_id).await {
        Ok(job_details) => HttpResponse::Ok().json(job_details),
        Err(e) => {
            match e {
                crate::utils::error::AppError::JobNotFound => {
                    HttpResponse::NotFound().json("Job non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Réessayer un job échoué (admin)
async fn retry_job(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.retry_job(*job_id).await {
        Ok(job) => HttpResponse::Ok().json(job),
        Err(e) => {
            match e {
                crate::utils::error::AppError::JobNotFound => {
                    HttpResponse::NotFound().json("Job non trouvé")
                }
                crate::utils::error::AppError::JobCannotBeRetried => {
                    HttpResponse::BadRequest().json("Ce job ne peut pas être réessayé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Obtenir les logs d'audit (admin)
async fn get_audit_logs(
    user: AuthenticatedUser,
    system_service: web::Data<SystemService>,
    query: web::Query<AuditLogQuery>,
) -> impl Responder {
    // Vérifier les permissions admin
    if let Err(e) = require_admin(&user) {
        return e.into();
    }
    
    match system_service.get_audit_logs(
        query.action.as_deref(),
        query.user_id,
        query.resource_type.as_deref(),
        query.start_date,
        query.end_date,
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(100),
    ).await {
        Ok(logs) => {
            let total = logs.len() as i64;
            let response = PaginatedResponse {
                items: logs,
                total,
                page: query.page.unwrap_or(1),
                per_page: query.per_page.unwrap_or(100),
                total_pages: (total as f64 / query.per_page.unwrap_or(100) as f64).ceil() as i64,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

// Structures de requête pour les queries admin
#[derive(Debug, serde::Deserialize)]
struct AdminListQuery {
    page: Option<i64>,
    per_page: Option<i64>,
    search: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AdminJobQuery {
    status: Option<String>,
    user_id: Option<uuid::Uuid>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct AuditLogQuery {
    action: Option<String>,
    user_id: Option<uuid::Uuid>,
    resource_type: Option<String>,
    start_date: Option<chrono::DateTime<chrono::Utc>>,
    end_date: Option<chrono::DateTime<chrono::Utc>>,
    page: Option<i64>,
    per_page: Option<i64>,
}