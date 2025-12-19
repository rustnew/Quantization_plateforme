// backend/src/main.rs
mod models;
mod api;
mod core;
mod services;
mod utils;

use crate::utils::config::Config;
use crate::utils::error::Result;
use crate::services::{
    Database, Cache, JobQueue, FileStorage, 
    GoogleAuthClient, SendGridClient, PythonClient
};
use crate::core::{
    UserService, JobService, QuantizationService,
    BillingService, NotificationService, LogEmailProvider
};
use actix_web::{web, App, HttpServer};
use std::sync::Arc;
use std::path::Path;
use tracing_subscriber::{fmt, EnvFilter};

#[actix_web::main]
async fn main() -> Result<()> {
    // 1. Charger la configuration
    let config = Config::from_env()?;
    
    // 2. Initialiser le logging
    init_logging(&config)?;
    
    // 3. Initialiser les services d'infrastructure
    let (db, cache, queue, storage) = init_infrastructure(&config).await?;
    
    // 4. Initialiser les services externes
    let (google_client, email_provider, python_client) = init_external_services(&config);
    
    // 5. Initialiser les services métier
    let (user_service, job_service, quant_service, billing_service, notification_service) = 
        init_business_services(
            &config, 
            db, cache, queue.clone(), storage.clone(), 
            google_client, email_provider, python_client
        ).await?;
    
    // 6. Démarrer les workers background
    start_background_workers(
        job_service.clone(), 
        quant_service.clone(), 
        &config
    );
    
    // 7. Lancer le serveur HTTP
    start_http_server(
        config, 
        user_service, job_service, billing_service, notification_service,
        queue, storage,
    ).await?;
    
    Ok(())
}

/// Initialiser le système de logging
fn init_logging(config: &Config) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    
    if config.logging_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }
    
    log::info!("Logging initialisé avec niveau: {}", config.log_level);
    Ok(())
}

/// Initialiser l'infrastructure (DB, Cache, Queue, Storage)
async fn init_infrastructure(
    config: &Config,
) -> Result<(
    Arc<Database>,
    Arc<Cache>,
    Arc<JobQueue>,
    Arc<FileStorage>,
)> {
    log::info!("Initialisation de l'infrastructure...");
    
    // Base de données
    let db = Arc::new(Database::new(&config.database_url).await?);
    log::info!("✅ Base de données connectée");
    
    // Exécuter les migrations
    db.run_migrations().await?;
    log::info!("✅ Migrations exécutées");
    
    // Cache Redis
    let cache = Arc::new(
        Cache::new(
            &config.redis_url,
            Some(&config.redis_queue_prefix),
            config.redis_cache_ttl_seconds,
        ).await?
    );
    log::info!("✅ Cache Redis initialisé");
    
    // Queue Redis
    let queue = Arc::new(
        JobQueue::new(
            &config.redis_url,
            Some(&config.redis_queue_prefix),
        ).await?
    );
    log::info!("✅ Queue Redis initialisée");
    
    // Stockage fichiers
    let storage = Arc::new(FileStorage::new(
        config.minio_endpoint.as_deref(),
        config.minio_access_key.as_deref(),
        config.minio_secret_key.as_deref(),
        &config.minio_bucket,
        Some(Path::new("./storage")),
        if config.storage_encryption_key.is_empty() {
            None
        } else {
            Some(&config.storage_encryption_key)
        },
        config.max_file_size_mb,
    ));
    log::info!("✅ Stockage initialisé (type: {})", config.storage_type);
    
    Ok((db, cache, queue, storage))
}

/// Initialiser les services externes
fn init_external_services(
    config: &Config,
) -> (
    Option<Arc<GoogleAuthClient>>,
    Arc<dyn crate::core::notification_service::EmailProvider + Send + Sync>,
    Arc<PythonClient>,
) {
    log::info!("Initialisation des services externes...");
    
    // Client Google OAuth
    let google_client = if config.enable_google_oauth {
        config.google_oauth_client_id.as_ref().and_then(|client_id| {
            config.google_oauth_client_secret.as_ref().map(|client_secret| {
                Arc::new(GoogleAuthClient::new(
                    client_id.clone(),
                    client_secret.clone(),
                    config.google_oauth_redirect_uri
                        .clone()
                        .unwrap_or_else(|| "http://localhost:8080/api/auth/google/callback".to_string()),
                ))
            })
        })
    } else {
        None
    };
    
    if google_client.is_some() {
        log::info!("✅ Google OAuth activé");
    }
    
    // Fournisseur d'emails
    let email_provider: Arc<dyn crate::core::notification_service::EmailProvider + Send + Sync> = 
        if config.enable_email_notifications && config.email_provider == "sendgrid" {
            if let Some(api_key) = &config.sendgrid_api_key {
                Arc::new(SendGridClient::new(
                    api_key.clone(),
                    config.email_from.clone(),
                    config.email_from_name.clone(),
                ))
            } else {
                log::warn!("SendGrid configuré mais SENDGRID_API_KEY manquant, utilisation du logger");
                Arc::new(LogEmailProvider)
            }
        } else {
            log::info!("📧 Emails en mode log (développement)");
            Arc::new(LogEmailProvider)
        };
    
    // Client Python pour la quantification
    let python_client = Arc::new(PythonClient::new(
        &config.quantization_python_path,
        Some("python3"),
        config.quantization_timeout_seconds,
    ));
    log::info!("✅ Client Python initialisé");
    
    (google_client, email_provider, python_client)
}

/// Initialiser les services métier
async fn init_business_services(
    config: &Config,
    db: Arc<Database>,
    cache: Arc<Cache>,
    queue: Arc<JobQueue>,
    storage: Arc<FileStorage>,
    google_client: Option<Arc<GoogleAuthClient>>,
    email_provider: Arc<dyn crate::core::notification_service::EmailProvider + Send + Sync>,
    python_client: Arc<PythonClient>,
) -> Result<(
    Arc<UserService>,
    Arc<JobService>,
    Arc<QuantizationService>,
    Arc<BillingService>,
    Arc<NotificationService>,
)> {
    log::info!("Initialisation des services métier...");
    
    // Service utilisateur
    let user_service = Arc::new(UserService::new(
        db.clone(),
        cache.clone(),
        config.jwt_secret.clone(),
        config.admin_email.clone(),
        config.admin_password.clone(),
    ));
    log::info!("✅ Service utilisateur initialisé");
    
    // Service de quantification
    let work_dir = Path::new("./work").to_path_buf();
    std::fs::create_dir_all(&work_dir).ok();
    
    let quant_service = Arc::new(QuantizationService::new(
        python_client.clone(),
        config.quantization_gpu_enabled,
        config.quantization_timeout_seconds,
        config.quantization_max_retries,
        work_dir,
        config.quantization_max_concurrent_jobs,
    ));
    log::info!("✅ Service de quantification initialisé");
    
    // Service de jobs
    let job_service = Arc::new(JobService::new(
        db.clone(),
        queue.clone(),
        storage.clone(),
        quant_service.clone(),
        config.quantization_max_concurrent_jobs,
    ));
    log::info!("✅ Service de jobs initialisé");
    
    // Service de facturation
    let billing_service = Arc::new(BillingService::new(
        db.clone(),
        config.stripe_secret_key.clone().unwrap_or_default(),
        config.stripe_webhook_secret.clone().unwrap_or_default(),
        config.stripe_currency.clone(),
        config.stripe_trial_period_days,
    ));
    log::info!("✅ Service de facturation initialisé");
    
    // Service de notifications
    let notification_service = Arc::new(NotificationService::new(
        email_provider,
        None, // Pas de SMS pour le MVP
        config.frontend_url.clone(),
    ));
    log::info!("✅ Service de notifications initialisé");
    
    // Créer l'utilisateur admin si nécessaire
    init_admin_user(&user_service, config).await?;
    
    Ok((user_service, job_service, quant_service, billing_service, notification_service))
}

/// Créer l'utilisateur admin
async fn init_admin_user(user_service: &UserService, config: &Config) -> Result<()> {
    match user_service.register_user(&config.admin_email, &config.admin_password).await {
        Ok(user) => {
            log::info!("✅ Utilisateur admin créé: {}", user.email);
            Ok(())
        }
        Err(AppError::UserAlreadyExists) => {
            log::info!("👤 Utilisateur admin déjà existant");
            Ok(())
        }
        Err(e) => {
            log::error!("❌ Erreur lors de la création de l'utilisateur admin: {}", e);
            Err(e)
        }
    }
}

/// Démarrer les workers background
fn start_background_workers(
    job_service: Arc<JobService>,
    quant_service: Arc<QuantizationService>,
    config: &Config,
) {
    // Worker de traitement des jobs
    let job_service_clone = job_service.clone();
    tokio::spawn(async move {
        log::info!("🚀 Démarrage du worker de jobs...");
        job_service_clone.start_worker(5).await; // Vérifie toutes les 5 secondes
    });
    
    // Worker de nettoyage des fichiers temporaires
    let quant_service_clone = quant_service.clone();
    tokio::spawn(async move {
        let interval = tokio::time::Duration::from_secs(3600); // Toutes les heures
        
        loop {
            tokio::time::sleep(interval).await;
            
            match quant_service_clone.cleanup_old_files(7).await { // 7 jours
                Ok(deleted) if deleted > 0 => {
                    log::info!("🧹 {} fichiers temporaires nettoyés", deleted);
                }
                _ => {}
            }
        }
    });
    
    log::info!("✅ Workers background démarrés");
}

/// Démarrer le serveur HTTP
async fn start_http_server(
    config: Config,
    user_service: Arc<UserService>,
    job_service: Arc<JobService>,
    billing_service: Arc<BillingService>,
    notification_service: Arc<NotificationService>,
    queue: Arc<JobQueue>,
    storage: Arc<FileStorage>,
) -> Result<()> {
    let host = config.server_host.clone();
    let port = config.server_port;
    
    log::info!("🌍 Démarrage du serveur sur {}:{}", host, port);
    log::info!("📊 Mode: {}", config.run_mode);
    log::info!("👷 Workers: {}", config.workers);
    
    HttpServer::new(move || {
        App::new()
            // Données de configuration
            .app_data(web::Data::new(config.clone()))
            
            // Services métier
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(job_service.clone()))
            .app_data(web::Data::new(billing_service.clone()))
            .app_data(web::Data::new(notification_service.clone()))
            
            // Services d'infrastructure
            .app_data(web::Data::new(queue.clone()))
            .app_data(web::Data::new(storage.clone()))
            
            // Middleware
            .wrap(actix_web::middleware::Logger::default())
            .wrap(actix_cors::Cors::default()
                .allow_any_origin()
                .allow_any_method()
                .allow_any_header()
                .max_age(3600))
            .wrap(actix_web::middleware::Compress::default())
            .wrap(actix_web::middleware::NormalizePath::trim())
            
            // Routes API
            .configure(api::configure_routes)
            
            // Health check
            .route("/health", web::get().to(health_check))
            .route("/ready", web::get().to(ready_check))
    })
    .workers(config.workers)
    .bind((host, port))?
    .run()
    .await
    .map_err(|e| AppError::Internal)?;
    
    Ok(())
}

/// Health check endpoint
async fn health_check() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "quantization-platform",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Ready check endpoint
async fn ready_check() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(serde_json::json!({
        "status": "ready",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}