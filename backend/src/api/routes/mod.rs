

use actix_web::web;

pub mod auth;
pub mod jobs;
pub mod subscriptions;
pub mod models;
pub mod middleware;
pub mod upload;
pub mod 

fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            // Routes d'authentification
            .service(auth::login)
            .service(auth::register)
            .service(auth::google_callback)
            .service(auth::refresh_token)
            .service(auth::logout)
            
            // Routes des jobs de quantification
            .service(jobs::create_job)
            .service(jobs::get_job)
            .service(jobs::list_jobs)
            .service(jobs::download_result)
            .service(jobs::check_status)
            
            // Routes des abonnements
            .service(subscriptions::get_subscription)
            .service(subscriptions::upgrade_plan)
            .service(subscriptions::cancel_subscription)
            .service(subscriptions::get_payment_history)
            
            // Routes des modèles
            .service(models::upload_model)
            .service(models::list_models)
            .service(models::get_model)
            .service(models::get_report)
            
            // Routes admin (protégées)
            .service(web::scope("/admin")
                .wrap(middleware::require_admin)
                .service(jobs::list_all_jobs)
                .service(subscriptions::list_all_subscriptions)
                .service(models::list_all_models)
            )
    );
    
    // Routes publiques
    cfg.service(web::resource("/health").route(web::get().to(health_check)));
}

/// Endpoint de santé pour les probes Kubernetes/Docker
async fn health_check() -> impl actix_web::Responder {
    actix_web::HttpResponse::Ok().json({
        serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime": format!("{} seconds", std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    })
}
