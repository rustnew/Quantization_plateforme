

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use std::env;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};


mod api;
mod core;
mod domain;
mod infrastructure;
mod workers;

use infrastructure::{
    database::Database,
    python::PythonRuntime,
    storage::StorageService,
    queue::RedisQueue,
};
use workers::quantization_worker::{QuantizationWorker, WorkerConfig, start_worker_background};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Database,
    pub storage: StorageService,
    pub queue: RedisQueue,
    pub python_runtime: PythonRuntime,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialisation du logging
    setup_tracing();
    info!("🚀 Démarrage de Quantization Platform Backend");

    // Chargement de la configuration
    let config = load_configuration().expect("❌ Impossible de charger la configuration");
    info!("✅ Configuration chargée avec succès");
    info!("🔧 Mode: {}", config.server.run_mode);
    
    // Validation des variables d'environnement critiques
    validate_environment_variables().expect("❌ Variables d'environnement manquantes");

    // Initialisation des services
    let db = Database::new(&config.database.url)
        .await
        .expect("❌ Impossible de se connecter à la base de données");
    
    let storage = StorageService::new(
        &config.storage.endpoint,
        &config.storage.access_key,
        &config.storage.secret_key,
        &config.storage.bucket,
    )
    .expect("❌ Impossible d'initialiser le stockage");
    
    let queue = RedisQueue::new(&config.redis.url)
        .await
        .expect("❌ Impossible de se connecter à Redis");
    
    let python_runtime = PythonRuntime::new()
        .expect("❌ Impossible d'initialiser le runtime Python");

    // Vérification des dépendances critiques
    verify_dependencies(&python_runtime).await;

    // Création de l'état de l'application
    let app_state = web::Data::new(AppState {
        db: db.clone(),
        storage: storage.clone(),
        queue: queue.clone(),
        python_runtime: python_runtime.clone(),
    });

    // Démarrage des workers background
    let worker_config = WorkerConfig::default();
    start_worker_background(
        worker_config,
        db.clone(),
        storage.clone(),
        python_runtime.clone(),
    ).await.expect("❌ Impossible de démarrer le worker background");

    // Configuration du serveur Actix-Web
    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .app_data(app_state.clone())
            .configure(api::config)
            .service(actix_files::Files::new("/static", "./static").show_files_listing())
            .default_service(web::route().to(|| async { "🚀 Quantization Platform Backend est en cours d'exécution!" }))
    })
    .bind(format!("{}:{}", config.server.host, config.server.port))?
    .workers(config.server.workers)
    .shutdown_timeout(10);

    info!("✅ Backend démarré avec succès!");
    info!("🔗 API disponible sur http://{}:{}", config.server.host, config.server.port);
    info!("📊 Documentation Swagger: http://{}:{}/api/docs", config.server.host, config.server.port);

    server.run().await
}

/// Configure le tracing pour le logging structuré
fn setup_tracing() {
    let log_level = env::var("LOG_LEVEL")
        .unwrap_or_else(|_| "info".into())
        .parse()
        .unwrap_or(tracing::Level::INFO);

    let log_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "json".into());

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(log_level.into()),
        )
        .with(if log_format == "json" {
            Box::new(
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true)
                    .with_span_list(true),
            ) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>
        } else {
            Box::new(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_line_number(true)
                    .with_file(true),
            ) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>
        });

    subscriber.init();
}

/// Charge la configuration depuis les fichiers et variables d'environnement
fn load_configuration() -> anyhow::Result<config::Config> {
    let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
    
    let mut settings = config::Config::default();
    
    // Ajout des sources de configuration
    settings
        .merge(config::File::with_name("config/base"))?
        .merge(config::File::with_name(&format!("config/{}", run_mode)))?
        .merge(config::Environment::with_prefix("APP"))?;

    // Validation des paramètres critiques
    validate_configuration(&settings)?;

    Ok(settings)
}

/// Valide les paramètres de configuration critiques
fn validate_configuration(settings: &config::Config) -> anyhow::Result<()> {
    // Validation du port
    let port: u16 = settings.get("server.port")?;
    if port == 0 || port > 65535 {
        return Err(anyhow::anyhow!("Port invalide: {}", port));
    }

    // Validation de l'URL de base de données
    let _db_url: String = settings.get("database.url")?;

    // Validation de la clé JWT
    let jwt_secret: String = settings.get("security.jwt_secret")?;
    if jwt_secret.len() < 32 {
        warn!("⚠️  JWT_SECRET trop court (< 32 caractères) - risque de sécurité");
    }

    // Validation de la clé de chiffrement
    let _encryption_key: String = settings.get("storage.encryption_key")?;

    Ok(())
}

/// Valide les variables d'environnement requises
fn validate_environment_variables() -> anyhow::Result<()> {
    let required_vars = vec![
        "DATABASE_URL",
        "REDIS_URL",
        "MINIO_ENDPOINT",
        "MINIO_ACCESS_KEY",
        "MINIO_SECRET_KEY",
        "JWT_SECRET",
        "STORAGE_ENCRYPTION_KEY"
    ];

    for var in required_vars {
        if env::var(var).is_err() {
            error!("❌ Variable d'environnement manquante: {}", var);
            return Err(anyhow::anyhow!("Variable d'environnement manquante: {}", var));
        }
    }

    Ok(())
}

/// Vérifie les dépendances critiques avant le démarrage
async fn verify_dependencies(python_runtime: &PythonRuntime) {
    info!("🔍 Vérification des dépendances...");

    // Vérification ONNX Runtime
    match ort::Environment::builder().build() {
        Ok(_) => info!("✅ ONNX Runtime: prêt"),
        Err(e) => warn!("⚠️  ONNX Runtime: {}", e),
    }

    // Vérification bindings Python
    match python_runtime.test_gptq_connection().await {
        Ok(_) => info!("✅ Python runtime (GPTQ): prêt"),
        Err(e) => warn!("⚠️  Python runtime (GPTQ): {}", e),
    }

    info!("✅ Toutes les dépendances vérifiées!");
}