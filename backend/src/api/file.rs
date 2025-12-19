// api/file.rs
use crate::models::{ModelFile, FileUpload, FileMetadata, PaginatedResponse};
use crate::api::AuthenticatedUser;
use crate::services::storage::FileStorage;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use futures_util::StreamExt as _;
use validator::Validate;

/// Configure les routes des fichiers
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/files")
            .wrap(crate::api::auth_middleware::require_auth())
            // Upload de fichier
            .route("/upload", web::post().to(upload_file))
            // Lister les fichiers
            .route("", web::get().to(list_files))
            // Obtenir les métadonnées d'un fichier
            .route("/{file_id}", web::get().to(get_file))
            // Supprimer un fichier
            .route("/{file_id}", web::delete().to(delete_file))
            // Télécharger un fichier
            .route("/{file_id}/download", web::get().to(download_file)),
    );
}

/// Uploader un fichier modèle
async fn upload_file(
    user: AuthenticatedUser,
    storage: web::Data<FileStorage>,
    mut payload: Multipart,
) -> impl Responder {
    let mut file_data = Vec::new();
    let mut filename = None;
    let mut content_type = None;
    
    // Lire le multipart form
    while let Some(item) = payload.next().await {
        match item {
            Ok(mut field) => {
                let field_name = field.name().to_string();
                
                if field_name == "file" {
                    filename = field.content_disposition().get_filename().map(|s| s.to_string());
                    content_type = field.content_type().map(|ct| ct.to_string());
                    
                    // Lire les données du fichier
                    while let Some(chunk) = field.next().await {
                        match chunk {
                            Ok(data) => {
                                file_data.extend_from_slice(&data);
                            }
                            Err(e) => {
                                return HttpResponse::InternalServerError()
                                    .json(format!("Erreur de lecture du fichier: {}", e));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                return HttpResponse::BadRequest().json(format!("Erreur de parsing: {}", e));
            }
        }
    }
    
    // Vérifier qu'un fichier a été fourni
    let filename = match filename {
        Some(name) => name,
        None => return HttpResponse::BadRequest().json("Aucun fichier fourni"),
    };
    
    // Vérifier la taille du fichier (max 10GB)
    if file_data.len() > 10 * 1024 * 1024 * 1024 {
        return HttpResponse::PayloadTooLarge().json("Fichier trop volumineux (max 10GB)");
    }
    
    // Calculer le hash SHA256
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let checksum = format!("{:x}", hasher.finalize());
    
    // Détecter le format du fichier
    let format = detect_file_format(&filename, content_type.as_deref());
    
    // Uploader le fichier vers le stockage
    match storage.upload_file(
        user.id,
        &filename,
        &file_data,
        &checksum,
        format,
    ).await {
        Ok(file_metadata) => {
            // Analyser le modèle pour extraire les métadonnées
            let metadata = analyze_model_metadata(&file_data, &filename).await;
            storage.update_file_metadata(file_metadata.id, metadata).await.ok();
            
            HttpResponse::Created().json(file_metadata)
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidFileFormat => {
                    HttpResponse::BadRequest().json("Format de fichier non supporté")
                }
                crate::utils::error::AppError::FileTooLarge => {
                    HttpResponse::PayloadTooLarge().json("Fichier trop volumineux")
                }
                _ => HttpResponse::InternalServerError().json("Erreur lors de l'upload"),
            }
        }
    }
}

/// Lister les fichiers de l'utilisateur
async fn list_files(
    user: AuthenticatedUser,
    storage: web::Data<FileStorage>,
    query: web::Query<ListFilesQuery>,
) -> impl Responder {
    match storage.list_user_files(
        user.id,
        query.format.as_deref(),
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(20),
    ).await {
        Ok(files) => {
            let total = files.len() as i64;
            let response = PaginatedResponse {
                items: files,
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

/// Obtenir les métadonnées d'un fichier
async fn get_file(
    user: AuthenticatedUser,
    storage: web::Data<FileStorage>,
    file_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match storage.get_file_metadata(*file_id).await {
        Ok(file_metadata) => {
            // Vérifier que l'utilisateur est propriétaire du fichier
            if file_metadata.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            HttpResponse::Ok().json(file_metadata)
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::FileNotFound => {
                    HttpResponse::NotFound().json("Fichier non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Supprimer un fichier
async fn delete_file(
    user: AuthenticatedUser,
    storage: web::Data<FileStorage>,
    file_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    // Vérifier que l'utilisateur est propriétaire du fichier
    match storage.get_file_metadata(*file_id).await {
        Ok(file_metadata) => {
            if file_metadata.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            // Supprimer le fichier
            match storage.delete_file(*file_id).await {
                Ok(_) => HttpResponse::NoContent().finish(),
                Err(e) => HttpResponse::InternalServerError().json("Erreur lors de la suppression"),
            }
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::FileNotFound => {
                    HttpResponse::NotFound().json("Fichier non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Télécharger un fichier
async fn download_file(
    user: AuthenticatedUser,
    storage: web::Data<FileStorage>,
    file_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match storage.get_file_metadata(*file_id).await {
        Ok(file_metadata) => {
            // Vérifier que l'utilisateur est propriétaire du fichier
            if file_metadata.user_id != user.id {
                return HttpResponse::Forbidden().json("Accès non autorisé");
            }
            
            // Générer une URL de téléchargement signée
            match storage.generate_download_url(*file_id).await {
                Ok(download_url) => {
                    let response = crate::models::file::FileDownload {
                        id: *file_id,
                        filename: file_metadata.filename,
                        file_size: file_metadata.file_size,
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
                crate::utils::error::AppError::FileNotFound => {
                    HttpResponse::NotFound().json("Fichier non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Détecter le format du fichier
fn detect_file_format(filename: &str, content_type: Option<&str>) -> crate::models::ModelFormat {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    
    match ext.as_str() {
        "pt" | "pth" => crate::models::ModelFormat::PyTorch,
        "bin" | "safetensors" => crate::models::ModelFormat::Safetensors,
        "onnx" => crate::models::ModelFormat::Onnx,
        "gguf" => crate::models::ModelFormat::Gguf,
        _ => {
            // Essayer avec le content-type
            if let Some(ct) = content_type {
                if ct.contains("application/octet-stream") && filename.contains(".safetensors") {
                    return crate::models::ModelFormat::Safetensors;
                }
            }
            crate::models::ModelFormat::PyTorch // Par défaut
        }
    }
}

/// Analyser les métadonnées du modèle (simplifié pour MVP)
async fn analyze_model_metadata(file_data: &[u8], filename: &str) -> crate::models::ModelMetadata {
    // Dans le MVP, on fait une détection basique
    // En production, on utiliserait une librairie Python comme `huggingface_hub`
    
    let filename_lower = filename.to_lowercase();
    
    let model_type = if filename_lower.contains("llama") {
        Some("llama".to_string())
    } else if filename_lower.contains("bert") {
        Some("bert".to_string())
    } else if filename_lower.contains("whisper") {
        Some("whisper".to_string())
    } else {
        None
    };
    
    // Estimation basée sur la taille du fichier
    let file_size_mb = file_data.len() as f64 / (1024.0 * 1024.0);
    let parameter_count = if file_size_mb > 10_000.0 {
        Some(70.0) // ~70B
    } else if file_size_mb > 3_000.0 {
        Some(13.0) // ~13B
    } else if file_size_mb > 1_500.0 {
        Some(7.0) // ~7B
    } else {
        Some(3.0) // ~3B
    };
    
    crate::models::ModelMetadata {
        model_type,
        architecture: None,
        parameter_count,
        quantization_bits: None,
    }
}

// Query parameters pour la liste des fichiers
#[derive(Debug, serde::Deserialize)]
struct ListFilesQuery {
    format: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}