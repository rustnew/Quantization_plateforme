

use actix_web::{get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::info;

use crate::{
    infrastructure::database::{Database, JobsRepository, UserRepository},
    infrastructure::storage::StorageService,
    infrastructure::error::AppResult,
    core::auth::get_current_user,
    domain::job::{JobStatus, Job},
};

/// Paramètres pour la liste des modèles
#[derive(Deserialize)]
pub struct ListModelsParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub status: Option<String>,
    pub quantization_method: Option<String>,
}

/// Résumé d'un modèle
#[derive(Serialize)]
pub struct ModelSummary {
    pub id: Uuid,
    pub model_name: String,
    pub quantization_method: String,
    pub original_size_gb: f32,
    pub quantized_size_gb: f32,
    pub reduction_percent: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub download_url: String,
}

/// Métadonnées détaillées d'un modèle
#[derive(Serialize)]
pub struct ModelMetadata {
    pub id: Uuid,
    pub model_name: String,
    pub quantization_method: String,
    pub framework: String,
    pub architecture: String,
    pub parameters_count: Option<u64>,
    pub original_size_bytes: i64,
    pub quantized_size_bytes: i64,
    pub reduction_percent: f32,
    pub quality_loss_percent: f32,
    pub latency_improvement_percent: f32,
    pub cost_savings_percent: f32,
    pub download_formats: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[get("/models")]
pub async fn list_models(
    req: actix_web::HttpRequest,
    query: web::Query<ListModelsParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let jobs_repo = JobsRepository::new(db.pool.clone());
    
    // Pagination
    let limit = query.limit.unwrap_or(10).min(50);
    let offset = query.offset.unwrap_or(0);
    
    // Filtrage par statut
    let status = if let Some(s) = &query.status {
        match s.to_lowercase().as_str() {
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "processing" => Some(JobStatus::Processing),
            "queued" => Some(JobStatus::Queued),
            _ => None,
        }
    } else {
        None
    };
    
    // Récupérer les jobs de l'utilisateur
    let jobs = if let Some(status_filter) = status {
        jobs_repo.get_by_status_and_user(&user.id, status_filter, limit as i64, offset as i64).await?
    } else {
        jobs_repo.get_by_user(&user.id, limit as i64, offset as i64).await?
    };
    
    // Total pour la pagination
    let total = jobs_repo.count_by_user(&user.id).await?;
    
    // Convertir en modèles
    let models: Vec<ModelSummary> = jobs.into_iter()
        .filter(|j| j.status == JobStatus::Completed)
        .map(|j| ModelSummary {
            id: j.id,
            model_name: j.model_name,
            quantization_method: format!("{:?}", j.quantization_method),
            original_size_gb: j.original_size_bytes as f32 / 1_000_000_000.0,
            quantized_size_gb: j.quantized_size_bytes.unwrap_or(0) as f32 / 1_000_000_000.0,
            reduction_percent: j.reduction_percent.unwrap_or(0.0),
            created_at: j.created_at,
            download_url: j.download_url.clone().unwrap_or_default(),
        })
        .collect();
    
    let response = serde_json::json!({
        "models": models,
        "pagination": {
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": (offset + limit) < total as usize
        }
    });
    
    Ok(HttpResponse::Ok().json(response))
}

#[get("/models/{id}")]
pub async fn get_model(
    req: actix_web::HttpRequest,
    path: web::Path<Uuid>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let job_id = path.into_inner();
    
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = jobs_repo.get_by_id(&job_id).await?;
    
    // Vérifier que l'utilisateur est propriétaire
    if job.user_id != user.id {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Vous n'avez pas accès à ce modèle".to_string()
        ));
    }
    
    // Vérifier que le job est complété
    if job.status != JobStatus::Completed {
        return Err(crate::infrastructure::error::AppError::BadRequest(
            "Le modèle n'est pas encore prêt".to_string()
        ));
    }
    
    // Générer les métadonnées complètes
    let metadata = generate_model_metadata(&job).await?;
    
    Ok(HttpResponse::Ok().json(metadata))
}

/// Génère les métadonnées complètes pour un modèle
async fn generate_model_metadata(job: &Job) -> AppResult<ModelMetadata> {
    // Pour le MVP, calculer les valeurs basées sur le job
    let original_size_gb = job.original_size_bytes as f32 / 1_000_000_000.0;
    let quantized_size_gb = job.quantized_size_bytes.unwrap_or(0) as f32 / 1_000_000_000.0;
    let reduction_percent = job.reduction_percent.unwrap_or(0.0);
    
    // Estimations basées sur la méthode de quantification
    let (quality_loss_percent, latency_improvement_percent, cost_savings_percent) = 
        match job.quantization_method {
            crate::domain::job::QuantizationMethod::Int8 => (0.8, 40.0, 40.0),
            crate::domain::job::QuantizationMethod::Int4 |
            crate::domain::job::QuantizationMethod::Gptq |
            crate::domain::job::QuantizationMethod::Awq => (1.5, 65.0, 70.0),
            _ => (1.0, 50.0, 50.0),
        };
    
    // Déterminer le framework basé sur le nom du fichier
    let framework = if job.file_name.ends_with(".onnx") {
        "onnx"
    } else if job.file_name.ends_with(".safetensors") || job.file_name.ends_with(".bin") {
        "pytorch"
    } else {
        "unknown"
    }.to_string();
    
    // Estimation des paramètres basée sur la taille
    let parameters_count = estimate_parameters_from_size(job.original_size_bytes as u64, &framework);
    
    Ok(ModelMetadata {
        id: job.id,
        model_name: job.model_name.clone(),
        quantization_method: format!("{:?}", job.quantization_method),
        framework,
        architecture: "llama".to_string(), // Valeur par défaut - à améliorer
        parameters_count,
        original_size_bytes: job.original_size_bytes,
        quantized_size_bytes: job.quantized_size_bytes.unwrap_or(0),
        reduction_percent,
        quality_loss_percent,
        latency_improvement_percent,
        cost_savings_percent,
        download_formats: vec!["onnx".to_string(), "gguf".to_string()],
        created_at: job.created_at,
        updated_at: job.updated_at,
    })
}

/// Estime le nombre de paramètres à partir de la taille
fn estimate_parameters_from_size(size_bytes: u64, framework: &str) -> Option<u64> {
    match framework {
        "pytorch" => {
            // PyTorch : ~2 bytes par paramètre en FP16
            Some(size_bytes / 2)
        },
        "onnx" => {
            // ONNX : ~4 bytes par paramètre en FP32
            Some(size_bytes / 4)
        },
        _ => None,
    }
}

#[get("/models/{id}/report")]
pub async fn get_report(
    req: actix_web::HttpRequest,
    path: web::Path<Uuid>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let job_id = path.into_inner();
    
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = jobs_repo.get_by_id(&job_id).await?;
    
    // Vérifier que l'utilisateur est propriétaire
    if job.user_id != user.id {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Vous n'avez pas accès à ce rapport".to_string()
        ));
    }
    
    // Vérifier que le job est complété
    if job.status != JobStatus::Completed {
        return Err(crate::infrastructure::error::AppError::BadRequest(
            "Le rapport n'est pas encore disponible".to_string()
        ));
    }
    
    // Générer un rapport détaillé
    let report = generate_detailed_report(&job).await?;
    
    Ok(HttpResponse::Ok().json(report))
}

/// Génère un rapport détaillé de performance
async fn generate_detailed_report(job: &Job) -> AppResult<serde_json::Value> {
    let reduction_percent = job.reduction_percent.unwrap_or(0.0);
    let original_size_gb = job.original_size_bytes as f64 / 1_073_741_824.0;
    let quantized_size_gb = job.quantized_size_bytes.unwrap_or(0) as f64 / 1_073_741_824.0;
    
    // Estimer les économies basées sur la réduction de taille
    let cost_savings_percent = if reduction_percent > 70.0 {
        70.0
    } else if reduction_percent > 50.0 {
        50.0
    } else {
        30.0
    };
    
    let report = serde_json!({
        "job_id": job.id,
        "model_name": job.model_name,
        "quantization_method": format!("{:?}", job.quantization_method),
        "original_size_gb": original_size_gb,
        "quantized_size_gb": quantized_size_gb,
        "reduction_percent": reduction_percent,
        "estimated_metrics": {
            "quality_loss_percent": 1.2,
            "latency_improvement_percent": 62.5,
            "cost_savings_percent": cost_savings_percent,
            "memory_usage_reduction_gb": original_size_gb - quantized_size_gb,
            "inference_speedup": 3.2
        },
        "hardware_recommendations": {
            "minimum_ram_gb": if reduction_percent > 70.0 { 8.0 } else { 16.0 },
            "recommended_gpu": if job.quantization_method == crate::domain::job::QuantizationMethod::Int8 {
                "RTX 3060 or equivalent"
            } else {
                "RTX 3090 or equivalent"
            },
            "cloud_instance_type": if reduction_percent > 70.0 {
                "g4dn.xlarge (AWS) / NC4as_T4_v3 (Azure)"
            } else {
                "g4dn.2xlarge (AWS) / NC8as_T4_v3 (Azure)"
            }
        },
        "download_urls": {
            "onnx": format!("{}?format=onnx", job.download_url.as_ref().unwrap_or("")),
            "gguf": format!("{}?format=gguf", job.download_url.as_ref().unwrap_or("")),
            "pytorch": format!("{}?format=pytorch", job.download_url.as_ref().unwrap_or(""))
        },
        "generated_at": chrono::Utc::now().to_rfc3339()
    });
    
    Ok(report)
}
