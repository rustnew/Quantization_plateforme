// api/job.rs
use crate::models::{Job, NewJob, JobResult, PaginatedResponse};
use crate::api::AuthenticatedUser;
use crate::core::job_service::JobService;
use crate::core::billing_service::BillingService;
use crate::services::storage::FileStorage;
use actix_web::{web, HttpResponse, Responder};
use validator::Validate;

/// Configure les routes des jobs
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/jobs")
            .wrap(crate::api::auth_middleware::require_auth())
            // Créer un job
            .route("", web::post().to(create_job))
            // Lister les jobs
            .route("", web::get().to(list_jobs))
            // Obtenir un job spécifique
            .route("/{job_id}", web::get().to(get_job))
            // Annuler un job
            .route("/{job_id}/cancel", web::post().to(cancel_job))
            // Télécharger le résultat
            .route("/{job_id}/download", web::get().to(download_result))
            // Obtenir la progression en temps réel (WebSocket/SSE)
            .route("/{job_id}/progress", web::get().to(get_job_progress)),
    );
}

/// Créer un nouveau job de quantification
async fn create_job(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    billing_service: web::Data<BillingService>,
    storage: web::Data<FileStorage>,
    new_job: web::Json<NewJob>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    // Validation
    if let Err(errors) = new_job.validate() {
        return HttpResponse::BadRequest().json(errors);
    }
    
    // Vérifier que l'utilisateur a suffisamment de crédits
    match billing_service.check_user_credits(user.id).await {
        Ok(has_credits) => {
            if !has_credits {
                return HttpResponse::PaymentRequired().json("Crédits insuffisants");
            }
        }
        Err(e) => {
            return HttpResponse::InternalServerError().json("Erreur de vérification des crédits");
        }
    }
    
    // Extraire l'ID du fichier du header ou du body
    let file_id = match extract_file_id(&req) {
        Some(id) => id,
        None => {
            return HttpResponse::BadRequest().json("ID de fichier requis");
        }
    };
    
    // Vérifier que le fichier appartient à l'utilisateur
    match storage.get_file_owner(file_id).await {
        Ok(owner_id) => {
            if owner_id != user.id {
                return HttpResponse::Forbidden().json("Fichier non autorisé");
            }
        }
        Err(_) => {
            return HttpResponse::NotFound().json("Fichier non trouvé");
        }
    }
    
    // Créer le job
    match job_service.create_job(
        user.id,
        file_id,
        new_job.name.clone(),
        new_job.quantization_method.clone(),
        new_job.output_format.clone(),
    ).await {
        Ok(job) => {
            // Consommer les crédits
            billing_service.consume_job_credits(user.id, job.id).await.ok();
            
            HttpResponse::Created().json(job)
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidFileFormat => {
                    HttpResponse::BadRequest().json("Format de fichier non supporté")
                }
                crate::utils::error::AppError::InsufficientCredits => {
                    HttpResponse::PaymentRequired().json("Crédits insuffisants")
                }
                _ => HttpResponse::InternalServerError().json("Erreur lors de la création du job"),
            }
        }
    }
}

/// Lister les jobs de l'utilisateur
async fn list_jobs(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    query: web::Query<ListJobsQuery>,
) -> impl Responder {
    match job_service.list_user_jobs(
        user.id,
        query.status.as_deref(),
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(20),
    ).await {
        Ok(jobs) => {
            let total = jobs.len() as i64;
            let response = PaginatedResponse {
                items: jobs,
                total,
                page: query.page.unwrap_or(1),
                per_page: query.per_page.unwrap_or(20),
                total_pages: (total as f64 / query.per_page.unwrap_or(20) as f64).ceil() as i64,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir les détails d'un job
async fn get_job(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match job_service.get_job(*job_id).await {
        Ok(job) => {
            // Vérifier que l'utilisateur est propriétaire du job
            if job.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            HttpResponse::Ok().json(job)
        }
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

/// Annuler un job
async fn cancel_job(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier que l'utilisateur est propriétaire du job
    match job_service.get_job(*job_id).await {
        Ok(job) => {
            if job.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            // Vérifier que le job peut être annulé
            if !job.can_be_cancelled() {
                return HttpResponse::BadRequest().json("Ce job ne peut pas être annulé");
            }
            
            // Annuler le job
            match job_service.cancel_job(*job_id).await {
                Ok(_) => HttpResponse::Ok().json("Job annulé avec succès"),
                Err(e) => HttpResponse::InternalServerError().json("Erreur lors de l'annulation"),
            }
        }
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

/// Télécharger le résultat d'un job
async fn download_result(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    storage: web::Data<FileStorage>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match job_service.get_job(*job_id).await {
        Ok(job) => {
            // Vérifier que l'utilisateur est propriétaire du job
            if job.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            // Vérifier que le job est terminé avec succès
            if !job.is_completed() {
                return HttpResponse::BadRequest().json("Le job n'est pas encore terminé");
            }
            
            // Obtenir l'URL de téléchargement
            match storage.generate_download_url(job.output_file_id.unwrap()).await {
                Ok(download_url) => {
                    let response = crate::models::file::FileDownload {
                        id: job.id,
                        filename: format!("{}_{}.{}", job.name, job.id, job.output_format.extension()),
                        file_size: job.quantized_size.unwrap_or(0),
                        download_url,
                        expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
                    };
                    HttpResponse::Ok().json(response)
                }
                Err(e) => HttpResponse::InternalServerError().json("Erreur de génération du lien"),
            }
        }
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

/// Obtenir la progression d'un job en temps réel
async fn get_job_progress(
    user: AuthenticatedUser,
    job_service: web::Data<JobService>,
    job_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match job_service.get_job(*job_id).await {
        Ok(job) => {
            // Vérifier que l'utilisateur est propriétaire du job
            if job.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            // Pour SSE (Server-Sent Events)
            use actix_web::{HttpResponse, web};
            use tokio_stream::StreamExt;
            
            let job_id_clone = *job_id;
            let user_id_clone = user.id;
            
            // Créer un stream d'événements
            let stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(tokio::time::Duration::from_secs(2)))
                .then(move |_| {
                    let job_service_clone = job_service.clone();
                    async move {
                        match job_service_clone.get_job(job_id_clone).await {
                            Ok(job) => {
                                if job.user_id == user_id_clone {
                                    Ok(web::Bytes::from(format!(
                                        "event: progress\ndata: {}\n\n",
                                        serde_json::to_string(&job.progress_info()).unwrap()
                                    )))
                                } else {
                                    Err(actix_web::error::ErrorBadRequest("Accès non autorisé"))
                                }
                            }
                            Err(_) => Err(actix_web::error::ErrorNotFound("Job non trouvé")),
                        }
                    }
                })
                .take_while(|result| {
                    // Continuer jusqu'à ce que le job soit terminé
                    match result {
                        Ok(bytes) => {
                            let data = String::from_utf8_lossy(&bytes);
                            !data.contains("\"status\":\"completed\"") && 
                            !data.contains("\"status\":\"failed\"") && 
                            !data.contains("\"status\":\"cancelled\"")
                        }
                        Err(_) => false,
                    }
                });
            
            HttpResponse::Ok()
                .content_type("text/event-stream")
                .keep_alive()
                .streaming(stream)
        }
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

// Helper pour extraire l'ID de fichier
fn extract_file_id(req: &actix_web::HttpRequest) -> Option<uuid::Uuid> {
    // Essayer depuis le header
    if let Some(file_id) = req.headers().get("X-File-Id") {
        if let Ok(file_id_str) = file_id.to_str() {
            if let Ok(uuid) = uuid::Uuid::parse_str(file_id_str) {
                return Some(uuid);
            }
        }
    }
    
    // Essayer depuis les query parameters
    if let Some(file_id) = req.query_string().split('&')
        .find(|param| param.starts_with("file_id="))
        .and_then(|param| param.split('=').nth(1))
    {
        if let Ok(uuid) = uuid::Uuid::parse_str(file_id) {
            return Some(uuid);
        }
    }
    
    None
}

// Query parameters pour la liste des jobs
#[derive(Debug, serde::Deserialize)]
struct ListJobsQuery {
    status: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}