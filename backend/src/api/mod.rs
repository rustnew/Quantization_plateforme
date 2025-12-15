
pub mod routes;

use actix_web::web;

/// Configure toutes les routes de l'API
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            // Routes publiques
            .service(routes::upload::upload_model)
            .service(routes::auth::login)
            .service(routes::auth::register)
            
            // Routes protégées par authentification
            .wrap(crate::api::routes::middleware::AuthMiddleware)
            .service(routes::jobs::create_job)
            .service(routes::jobs::get_job)
            .service(routes::jobs::list_jobs)
            .service(routes::jobs::download_result)
            .service(routes::models::list_models)
            .service(routes::models::get_model)
            .service(routes::subscriptions::get_subscription)
            .service(routes::subscriptions::upgrade_plan)
            
            // Routes admin protégées
            .wrap(crate::api::routes::middleware::AdminMiddleware)
            .service(routes::jobs::list_all_jobs)
            .service(routes::subscriptions::list_all_subscriptions)
    );
    
    // Endpoint de santé
    cfg.service(web::resource("/health").route(web::get().to(health_check)));
}

/// Endpoint de santé pour monitoring
async fn health_check() -> impl actix_web::Responder {
    actix_web::HttpResponse::Ok().json({
        serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime": format!("{} seconds", std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "environment": std::env::var("RUN_MODE").unwrap_or_else(|_| "production".to_string())
        })
    })
}
