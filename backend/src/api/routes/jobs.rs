

use actix_web::{post, get, web, HttpResponse, Responder, HttpRequest};
use actix_web::http::header::CONTENT_DISPOSITION;
use serde::{Deserialize, Serialize};
use validator::Validate;
use uuid::Uuid;
use std::fs;
use std::path::PathBuf;
use tokio::task;

use crate::{
    domain::job::{Job, JobStatus, QuantizationMethod, NewJob},
    infrastructure::database::{Database, JobsRepository, UserRepository, SubscriptionsRepository},
    infrastructure::storage::StorageService,
    infrastructure::error::AppResult,
    core::auth::get_current_user,
    core::quantization::pipeline::QuantizationPipeline,
};

/// Requête pour créer un nouveau job de quantification
#[derive(Deserialize, Validate)]
pub struct CreateJobRequest {
    #[validate(length(min = 1, message = "Le nom du modèle est requis"))]
    pub model_name: String,
    #[validate(length(min = 1, message = "Le nom du fichier est requis"))]
    pub file_name: String,
    #[validate(range(min = 1, message = "La taille doit être positive"))]
    pub original_size_bytes: u64,
    #[validate(custom = "validate_quantization_method")]
    pub quantization_method: String,
}

/// Réponse de création de job
#[derive(Serialize)]
pub struct JobCreatedResponse {
    pub job: JobSummary,
    pub message: String,
    pub estimated_time_minutes: f32,
}

/// Résumé d'un job (pour les listes)
#[derive(Serialize)]
pub struct JobSummary {
    pub id: Uuid,
    pub model_name: String,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub quantization_method: QuantizationMethod,
    pub reduction_percent: Option<f32>,
    pub download_url: Option<String>,
}

impl From<Job> for JobSummary {
    fn from(job: Job) -> Self {
        Self {
            id: job.id,
            model_name: job.model_name,
            status: job.status,
            created_at: job.created_at,
            quantization_method: job.quantization_method,
            reduction_percent: job.reduction_percent,
            download_url: if job.status == JobStatus::Completed {
                Some(job.download_url.clone())
            } else {
                None
            },
        }
    }
}

/// Paramètres pour le téléchargement
#[derive(Deserialize)]
pub struct DownloadParams {
    pub token: String,
}

/// Valide la méthode de quantification
fn validate_quantization_method(method: &str) -> Result<(), validator::ValidationError> {
    match method.to_lowercase().as_str() {
        "int8" | "int4" | "gptq" | "awq" => Ok(()),
        _ => {
            let mut err = validator::ValidationError::new("quantization_method");
            err.message = Some("Méthode de quantification non supportée. Utilisez: int8, int4, gptq, awq".into());
            Err(err)
        }
    }
}

/// Endpoint de création de job
#[post("/jobs/create")]
pub async fn create_job(
    req: HttpRequest,
    request: web::Json<CreateJobRequest>,
    db: web::Data<Database>,
    storage: web::Data<StorageService>,
) -> AppResult<HttpResponse> {
    // Validation des inputs
    request.validate()?;
    
    // Récupération de l'utilisateur courant
    let user = get_current_user(&req, db.clone()).await?;
    
    // Vérification des crédits disponibles
    let subs_repo = SubscriptionsRepository::new(db.pool.clone());
    let job_cost = match request.quantization_method.as_str() {
        "int8" => 1,
        _ => 2, // INT4/GPTQ/AWQ coûtent plus cher
    };
    
    let has_credits = subs_repo.has_credits_available(&user.id).await?;
    if !has_credits {
        return Err(crate::infrastructure::error::AppError::PaymentRequired(
            "Pas assez de crédits disponibles. Veuillez mettre à niveau votre abonnement.".to_string()
        ));
    }
    
    // Conversion de la méthode de quantification
    let quant_method = match request.quantization_method.as_str() {
        "int8" => QuantizationMethod::Int8,
        "int4" => QuantizationMethod::Int4,
        "gptq" => QuantizationMethod::Gptq,
        "awq" => QuantizationMethod::Awq,
        _ => QuantizationMethod::Int8,
    };
    
    // Création du nouveau job
    let new_job = NewJob {
        user_id: user.id,
        model_name: request.model_name.clone(),
        file_name: request.file_name.clone(),
        original_size_bytes: request.original_size_bytes as i64,
        quantization_method: quant_method.clone(),
    };
    
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = jobs_repo.create(&new_job).await?;
    
    // Consommer un crédit
    subs_repo.consume_credit(&user.id).await?;
    
    // Démarrer le traitement en arrière-plan
    let job_id = job.id;
    let quant_method_clone = quant_method.clone();
    let storage_clone = storage.clone();
    let db_clone = db.clone();
    
    task::spawn(async move {
        match QuantizationPipeline::process_job(job_id, quant_method_clone, storage_clone, db_clone).await {
            Ok(_) => {
                tracing::info!("✅ Job {} complété avec succès", job_id);
            },
            Err(e) => {
                tracing::error!("❌ Échec du job {}: {}", job_id, e);
                
                // Marquer le job comme échoué
                let jobs_repo = JobsRepository::new(db_clone.pool.clone());
                let _ = jobs_repo.fail_job(&job_id, format!("Erreur de quantification: {}", e)).await;
            }
        }
    });
    
    // Estimation du temps en fonction de la taille et méthode
    let estimated_time = estimate_processing_time(request.original_size_bytes, &quant_method);
    
    // Réponse avec l'ID du job et le statut
    let response = JobCreatedResponse {
        job: job.clone().into(),
        message: format!("Job de quantification {} créé avec succès", job.id),
        estimated_time_minutes: estimated_time,
    };
    
    Ok(HttpResponse::Accepted().json(response))
}

/// Endpoint pour obtenir un job spécifique
#[get("/jobs/{id}")]
pub async fn get_job(
    req: HttpRequest,
    path: web::Path<Uuid>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let job_id = path.into_inner();
    let user = get_current_user(&req, db.clone()).await?;
    
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = jobs_repo.get_by_id(&job_id).await?;
    
    // Vérifier que l'utilisateur est propriétaire du job
    if job.user_id != user.id {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Vous n'avez pas les permissions pour accéder à ce job".to_string()
        ));
    }
    
    Ok(HttpResponse::Ok().json(job))
}

/// Endpoint pour lister les jobs de l'utilisateur
#[get("/jobs")]
pub async fn list_jobs(
    req: HttpRequest,
    query: web::Query<JobListParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let jobs_repo = JobsRepository::new(db.pool.clone());
    
    // Pagination
    let limit = query.limit.unwrap_or(10).min(50); // Max 50 par page
    let offset = query.offset.unwrap_or(0);
    
    let jobs = jobs_repo.get_by_user(&user.id, limit as i64, offset as i64).await?;
    let total = jobs_repo.count_by_user(&user.id).await?;
    
    let response = serde_json::json!({
        "jobs": jobs.into_iter().map(JobSummary::from).collect::<Vec<JobSummary>>(),
        "pagination": {
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": (offset + limit) < total as usize
        }
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// Paramètres pour la liste des jobs
#[derive(Deserialize)]
pub struct JobListParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub status: Option<String>,
}

/// Endpoint pour vérifier le statut d'un job
#[get("/jobs/{id}/status")]
pub async fn check_status(
    path: web::Path<Uuid>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let job_id = path.into_inner();
    let jobs_repo = JobsRepository::new(db.pool.clone());
    
    let job = jobs_repo.get_by_id(&job_id).await?;
    
    let response = serde_json::json!({
        "job_id": job.id,
        "status": job.status,
        "progress_percent": calculate_progress(&job),
        "message": get_status_message(&job),
        "created_at": job.created_at,
        "updated_at": job.updated_at,
        "estimated_completion_time": estimate_completion_time(&job),
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// Endpoint pour télécharger le résultat d'un job
#[get("/jobs/{id}/download")]
pub async fn download_result(
    path: web::Path<Uuid>,
    query: web::Query<DownloadParams>,
    db: web::Data<Database>,
    req: HttpRequest,
) -> AppResult<HttpResponse> {
    let job_id = path.into_inner();
    let token = &query.token;
    
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = jobs_repo.get_by_id(&job_id).await?;
    
    // Vérifier que le job est complété
    if job.status != JobStatus::Completed {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Le job n'est pas encore complété".to_string()
        ));
    }
    
    // Vérifier le token de téléchargement
    if !jobs_repo.verify_download_token(&job_id, token).await? {
        return Err(crate::infrastructure::error::AppError::Unauthorized(
            "Token de téléchargement invalide".to_string()
        ));
    }
    
    // Construire le chemin du fichier
    let file_path = PathBuf::from(&job.output_path);
    
    // Vérifier que le fichier existe
    if !file_path.exists() {
        return Err(crate::infrastructure::error::AppError::NotFound(
            "Fichier non trouvé sur le serveur".to_string()
        ));
    }
    
    // Lire le fichier
    let file_content = fs::read(&file_path)?;
    let file_name = job.quantized_filename();
    
    // Créer la réponse avec les headers appropriés
    let mut response = HttpResponse::Ok()
        .append_header((
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_name),
        ))
        .content_type("application/octet-stream")
        .body(file_content);
    
    Ok(response)
}

/// Endpoint admin : lister tous les jobs
#[get("/admin/jobs")]
pub async fn list_all_jobs(
    query: web::Query<JobListParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let jobs_repo = JobsRepository::new(db.pool.clone());
    
    // Pagination
    let limit = query.limit.unwrap_or(20).min(100); // Max 100 par page pour admin
    let offset = query.offset.unwrap_or(0);
    
    // Récupérer tous les jobs (pas de filtre utilisateur)
    let query = sqlx::query_as!(
        Job,
        r#"
        SELECT 
            id, user_id, model_name, original_size_bytes, quantized_size_bytes,
            quantization_method::VARCHAR as "quantization_method: QuantizationMethod",
            status::VARCHAR as "status: JobStatus",
            error_message, reduction_percent, download_url,
            created_at, updated_at
        FROM jobs
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit as i64,
        offset as i64
    )
    .fetch_all(&db.pool)
    .await?;
    
    // Compter le total
    let total = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as count
        FROM jobs
        "#,
    )
    .fetch_one(&db.pool)
    .await?
    .count
    .unwrap_or(0);
    
    let response = serde_json::json!({
        "jobs": query.into_iter().map(JobSummary::from).collect::<Vec<JobSummary>>(),
        "pagination": {
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": (offset + limit) < total as usize
        }
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// Calcule le progrès du job en pourcentage
fn calculate_progress(job: &Job) -> u8 {
    match job.status {
        JobStatus::Queued => 0,
        JobStatus::Processing => 50,
        JobStatus::Completed => 100,
        JobStatus::Failed => 0,
        JobStatus::Cancelled => 0,
    }
}

/// Récupère un message de statut lisible
fn get_status_message(job: &Job) -> String {
    match job.status {
        JobStatus::Queued => "En attente de traitement".to_string(),
        JobStatus::Processing => "En cours de quantification".to_string(),
        JobStatus::Completed => format!("Complété ! Réduction de {:.1}%", job.reduction_percent.unwrap_or(0.0)),
        JobStatus::Failed => format!("Échoué: {}", job.error_message.as_deref().unwrap_or("Erreur inconnue")),
        JobStatus::Cancelled => "Annulé par l'utilisateur".to_string(),
    }
}

/// Estime le temps de traitement en fonction de la taille et méthode
fn estimate_processing_time(file_size_bytes: u64, method: &QuantizationMethod) -> f32 {
    let file_size_gb = file_size_bytes as f32 / 1_000_000_000.0;
    
    let base_time_per_gb = match method {
        QuantizationMethod::Int8 => 0.5,    // 30 secondes par GB
        QuantizationMethod::Int4 => 1.0,    // 1 minute par GB  
        QuantizationMethod::Gptq => 1.5,    // 1.5 minutes par GB
        QuantizationMethod::Awq => 1.5,     // 1.5 minutes par GB
        QuantizationMethod::Dynamic => 0.3, // 18 secondes par GB
    };
    
    file_size_gb * base_time_per_gb
}

/// Estime le temps de complétion
fn estimate_completion_time(job: &Job) -> Option<chrono::DateTime<chrono::Utc>> {
    if job.status == JobStatus::Queued || job.status == JobStatus::Processing {
        let estimated_minutes = estimate_processing_time(
            job.original_size_bytes as u64,
            &job.quantization_method
        );
        
        Some(Utc::now() + chrono::Duration::minutes(estimated_minutes as i64))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use crate::infrastructure::database::Database;
    use sqlx::PgPool;
    use std::env;
    use uuid::Uuid;
    use std::fs::File;
    use std::io::Write;

    async fn setup_test_app() -> (test::TestServer, PgPool) {
        let database_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://quant_user:quant_pass@localhost:5432/quant_test".to_string());
        
        let pool = PgPool::connect(&database_url).await.unwrap();
        let db = Database::new_with_pool(pool.clone());
        let storage = crate::infrastructure::storage::StorageService::new_test();
        
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(storage))
                .service(create_job)
                .service(get_job)
                .service(list_jobs)
                .service(check_status)
                .service(download_result)
        ).await;
        
        let server = test::TestServer::with_service(app);
        (server, pool)
    }

    async fn clear_jobs_table(pool: &PgPool) {
        sqlx::query("DELETE FROM jobs WHERE model_name LIKE '%test%'")
            .execute(pool)
            .await
            .unwrap();
    }

    async fn clear_users_table(pool: &PgPool) {
        sqlx::query("DELETE FROM users WHERE email LIKE '%@test.com'")
            .execute(pool)
            .await
            .unwrap();
    }

    #[actix_web::test]
    async fn test_job_creation() {
        let (server, pool) = setup_test_app().await;
        clear_jobs_table(&pool).await;
        clear_users_table(&pool).await;
        
        // Créer un utilisateur test et le connecter
        let db = Database::new_with_pool(pool.clone());
        let user_repo = UserRepository::new(db.pool.clone());
        
        let user = user_repo.create(&NewUser {
            name: "Test User".to_string(),
            email: "jobtest@test.com".to_string(),
            password: Some("password123".to_string()),
        }).await.unwrap();
        
        // Créer un abonnement pour l'utilisateur
        let subs_repo = SubscriptionsRepository::new(db.pool.clone());
        subs_repo.create_free_subscription(&user.id).await.unwrap();
        
        // Créer un job
        let req = test::TestRequest::post()
            .uri("/api/jobs/create")
            .insert_header(("Authorization", format!("Bearer {}", create_test_token(&user.id))))
            .set_json(&CreateJobRequest {
                model_name: "Test-Model".to_string(),
                file_name: "test_model.onnx".to_string(),
                original_size_bytes: 1_000_000_000, // 1GB
                quantization_method: "int8".to_string(),
            })
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body: JobCreatedResponse = test::read_body_json(resp).await;
        assert_eq!(body.job.model_name, "Test-Model");
        assert_eq!(body.job.status, JobStatus::Queued);
        assert!(body.estimated_time_minutes > 0.0);
    }

    #[actix_web::test]
    async fn test_job_status_check() {
        let (server, pool) = setup_test_app().await;
        clear_jobs_table(&pool).await;
        
        // Créer un job test dans la base de données
        let db = Database::new_with_pool(pool.clone());
        let jobs_repo = JobsRepository::new(db.pool.clone());
        
        let test_user_id = Uuid::new_v4();
        let job = jobs_repo.create(&NewJob {
            user_id: test_user_id,
            model_name: "Status-Test".to_string(),
            file_name: "status_test.onnx".to_string(),
            original_size_bytes: 500_000_000,
            quantization_method: QuantizationMethod::Int8,
        }).await.unwrap();
        
        // Vérifier le statut
        let req = test::TestRequest::get()
            .uri(&format!("/api/jobs/{}/status", job.id))
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "queued");
        assert_eq!(body["job_id"], job.id.to_string());
    }

    #[actix_web::test]
    async fn test_job_download() {
        let (server, pool) = setup_test_app().await;
        clear_jobs_table(&pool).await;
        
        // Créer un fichier test
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file_path = temp_dir.path().join("test_quantized.onnx");
        let mut file = File::create(&test_file_path).unwrap();
        writeln!(file, "Contenu du modèle quantifié").unwrap();
        
        // Créer un job complété dans la base de données
        let db = Database::new_with_pool(pool.clone());
        let jobs_repo = JobsRepository::new(db.pool.clone());
        
        let test_user_id = Uuid::new_v4();
        let mut job = jobs_repo.create(&NewJob {
            user_id: test_user_id,
            model_name: "Download-Test".to_string(),
            file_name: "download_test.onnx".to_string(),
            original_size_bytes: 1_000_000_000,
            quantization_method: QuantizationMethod::Int8,
        }).await.unwrap();
        
        // Compléter le job
        job = jobs_repo.complete_job(
            &job.id,
            250_000_000, // 250MB
            format!("file://{}", test_file_path.to_string_lossy())
        ).await.unwrap();
        
        // Générer un token de téléchargement valide
        let token = job.download_url
            .split('?')
            .last()
            .and_then(|query| query.split('=').last())
            .unwrap()
            .to_string();
        
        // Télécharger le résultat
        let req = test::TestRequest::get()
            .uri(&format!("/api/jobs/{}/download?token={}", job.id, token))
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body = test::read_body(resp).await;
        assert!(!body.is_empty());
        assert!(String::from_utf8_lossy(&body).contains("Contenu du modèle quantifié"));
    }

    /// Fonction utilitaire pour créer un token JWT de test
    fn create_test_token(user_id: &Uuid) -> String {
        // Dans un vrai test, tu utiliserais un vrai JWT token
        // Pour le MVP, on retourne un token de test
        format!("test_token_{}", user_id)
    }
}