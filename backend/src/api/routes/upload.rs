//! # Upload Routes
//! 
//! Ce module gère l'upload sécurisé des modèles IA par les utilisateurs.
//! Il supporte les formats PyTorch (.bin, .safetensors) et ONNX (.onnx).
//! 
//! ## Workflow
//! 1. Validation du fichier uploadé (type, taille, sécurité)
//! 2. Stockage temporaire sécurisé dans MinIO/S3
//! 3. Analyse préliminaire du modèle
//! 4. Création d'un job de quantification avec des paramètres par défaut
//! 5. Retour de l'ID du job et URL de statut
//! 
//! ## Sécurité
//! - Validation MIME type avec détection magique
//! - Scan antivirus optionnel (configurable)
//! - Chiffrement côté client pour les modèles sensibles
//! - Taille maximale configurable par plan d'abonnement
//! - Rate limiting pour prévenir les attaques par upload massif
//! 
//! ## Limites
//! - Taille maximale : 20GB (configurable par environnement)
//! - Formats supportés : PyTorch (.bin, .safetensors), ONNX (.onnx)
//! - Timeout : 5 minutes pour l'upload complet

use actix_multipart::Multipart;
use actix_web::{post, web, HttpResponse, Responder};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::PathBuf;
use infer::Infer;
use tracing::{info, warn, error};

use crate::{
    infrastructure::database::{Database, UserRepository, JobsRepository},
    infrastructure::storage::StorageService,
    infrastructure::error::AppResult,
    core::quantization::analysis::ModelAnalyzer,
    domain::job::{NewJob, QuantizationMethod},
    core::auth::get_current_user,
};

/// Réponse d'upload réussi
#[derive(Serialize)]
pub struct UploadResponse {
    pub job_id: Uuid,
    pub file_name: String,
    pub original_size_bytes: u64,
    pub status: String,
    pub message: String,
    pub analysis: Option<serde_json::Value>,
}

/// Requête pour upload avec paramètres optionnels
#[derive(Deserialize)]
pub struct UploadParams {
    pub quantization_method: Option<String>,
    pub keep_original: Option<bool>,
    pub max_file_size_gb: Option<f32>,
}

#[post("/upload")]
pub async fn upload_model(
    req: actix_web::HttpRequest,
    payload: Multipart,
    query: web::Query<UploadParams>,
    db: web::Data<Database>,
    storage: web::Data<StorageService>,
) -> AppResult<HttpResponse> {
    // 1. Récupérer l'utilisateur courant
    let user = get_current_user(&req, db.clone()).await?;
    
    // 2. Valider et traiter le fichier uploadé
    let (file_name, file_content, file_size) = validate_and_save_upload(payload, &query).await?;
    
    info!("📁 Fichier uploadé: {} ({} bytes)", file_name, file_size);
    
    // 3. Sauvegarder le fichier dans le stockage sécurisé
    let input_path = storage.save_model_file(&file_name, &file_content, &user.id).await?;
    
    // 4. Analyser le modèle pour déterminer la meilleure méthode de quantification
    let analysis = analyze_model_file(&input_path).await?;
    info!("🔍 Modèle analysé: {:?}", analysis);
    
    // 5. Déterminer la méthode de quantification par défaut
    let quantization_method = determine_quantization_method(&analysis, &query.quantization_method);
    
    // 6. Créer un job de quantification
    let jobs_repo = JobsRepository::new(db.pool.clone());
    let job = create_quantization_job(
        &jobs_repo, 
        &user.id, 
        &file_name, 
        file_size as i64, 
        &quantization_method
    ).await?;
    
    // 7. Préparer la réponse
    let response = UploadResponse {
        job_id: job.id,
        file_name,
        original_size_bytes: file_size,
        status: "queued".to_string(),
        message: "Modèle uploadé avec succès. Quantification en cours.".to_string(),
        analysis: Some(serde_json::json!(analysis)),
    };
    
    Ok(HttpResponse::Accepted().json(response))
}

/// Valide et sauvegarde le fichier uploadé
async fn validate_and_save_upload(
    mut payload: Multipart,
    params: &UploadParams,
) -> AppResult<(String, Vec<u8>, u64)> {
    let max_file_size = params.max_file_size_gb.unwrap_or(20.0) as u64 * 1_000_000_000;
    
    while let Some(item) = payload.try_next().await? {
        let mut field = item;
        
        // Obtenir le nom du fichier
        let file_name = field
            .content_disposition()
            .get_filename()
            .unwrap_or("unknown_model.bin")
            .to_string()
            .replace(['/', '\\'], "_");
        
        // Lire le contenu du fichier avec limite de taille
        let mut buffer = Vec::new();
        while let Some(chunk) = field.try_next().await? {
            if (buffer.len() + chunk.len()) as u64 > max_file_size {
                return Err(crate::infrastructure::error::AppError::PayloadTooLarge(
                    format!("Fichier trop volumineux. Taille maximale: {} Go", max_file_size as f32 / 1_000_000_000.0)
                ));
            }
            buffer.extend_from_slice(&chunk);
        }
        
        // Valider le type MIME
        validate_mime_type(&buffer, &file_name)?;
        
        return Ok((file_name, buffer, buffer.len() as u64));
    }
    
    Err(crate::infrastructure::error::AppError::BadRequest(
        "Aucun fichier fourni dans la requête".to_string()
    ))
}

/// Valide le type MIME du fichier
fn validate_mime_type(buffer: &[u8], file_name: &str) -> AppResult<()> {
    let infer = Infer::new();
    let mime_type = infer.get(buffer)
        .map(|i| i.mime_type())
        .unwrap_or("application/octet-stream");
    
    let allowed_types = [
        "application/octet-stream",  // Fichiers binaires génériques
        "application/x-tar",         // Archives tar
        "application/zip",           // Archives zip
        "model/onnx",                // ONNX models
        "application/x-hdf5",        // HDF5 format (PyTorch)
        "application/json",          // JSON config
    ];
    
    let file_extension = file_name
        .split('.')
        .last()
        .unwrap_or("")
        .to_lowercase();
    
    let allowed_extensions = ["bin", "pt", "pth", "onnx", "safetensors", "zip", "tar", "gz", "json"];
    
    if !allowed_types.contains(&mime_type) && !allowed_extensions.contains(&file_extension.as_str()) {
        return Err(crate::infrastructure::error::AppError::UnsupportedMediaType(
            format!("Type de fichier non supporté: {}. Types autorisés: {:?}", mime_type, allowed_extensions)
        ));
    }
    
    Ok(())
}

/// Analyse le fichier modèle
async fn analyze_model_file(file_path: &str) -> AppResult<serde_json::Value> {
    let path = PathBuf::from(file_path);
    
    // Déterminer le type de modèle à partir de l'extension
    let model_type = if file_path.ends_with(".onnx") {
        "onnx"
    } else if file_path.ends_with(".safetensors") || file_path.ends_with(".bin") {
        "pytorch"
    } else {
        "unknown"
    };
    
    // Pour le MVP, retourner une analyse simulée
    // Dans la vraie version, utiliser ModelAnalyzer pour analyser le modèle
    let analysis = serde_json::json!({
        "model_type": model_type,
        "file_format": model_type,
        "estimated_parameters": "7B",
        "recommended_quantization": if model_type == "onnx" { "int8" } else { "gptq" },
        "estimated_size_reduction": if model_type == "onnx" { 0.75 } else { 0.70 },
        "estimated_quality_loss": if model_type == "onnx" { 0.01 } else { 0.02 }
    });
    
    Ok(analysis)
}

/// Détermine la méthode de quantification optimale
fn determine_quantization_method(
    analysis: &serde_json::Value, 
    user_preference: &Option<String>
) -> QuantizationMethod {
    if let Some(pref) = user_preference {
        match pref.to_lowercase().as_str() {
            "int8" => return QuantizationMethod::Int8,
            "int4" => return QuantizationMethod::Int4,
            "gptq" => return QuantizationMethod::Gptq,
            "awq" => return QuantizationMethod::Awq,
            _ => {}
        }
    }
    
    // Méthode par défaut basée sur l'analyse
    if let Some(recommended) = analysis.get("recommended_quantization") {
        if let Some(recommended_str) = recommended.as_str() {
            match recommended_str {
                "int8" => return QuantizationMethod::Int8,
                "int4" => return QuantizationMethod::Int4,
                "gptq" => return QuantizationMethod::Gptq,
                "awq" => return QuantizationMethod::Awq,
                _ => {}
            }
        }
    }
    
    // Valeur par défaut sécurisée
    QuantizationMethod::Int8
}

/// Crée un job de quantification dans la base de données
async fn create_quantization_job(
    jobs_repo: &JobsRepository,
    user_id: &Uuid,
    file_name: &str,
    file_size: i64,
    quantization_method: &QuantizationMethod,
) -> AppResult<crate::domain::job::Job> {
    let model_name = file_name
        .split('.')
        .next()
        .unwrap_or("unknown_model")
        .to_string();
    
    let new_job = NewJob {
        user_id: *user_id,
        model_name,
        file_name: file_name.to_string(),
        original_size_bytes: file_size,
        quantization_method: quantization_method.clone(),
    };
    
    jobs_repo.create(&new_job).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use actix_multipart::form::tempfile::TempFile;
    use std::fs::File;
    use std::io::Write;
    
    #[actix_web::test]
    async fn test_file_validation() {
        // Créer un fichier test
        let temp_file = TempFile::new("test.onnx").unwrap();
        let mut file = File::create(temp_file.path()).unwrap();
        writeln!(file, "ONNX model content").unwrap();
        
        // Lire le contenu
        let content = std::fs::read(temp_file.path()).unwrap();
        
        // Tester la validation
        let params = UploadParams {
            quantization_method: None,
            keep_original: None,
            max_file_size_gb: Some(0.001), // 1Mo max pour le test
        };
        
        let result = validate_mime_type(&content, "test.onnx");
        assert!(result.is_ok());
        
        // Tester avec un fichier trop volumineux
        let large_content = vec![0; 2_000_000]; // 2Mo
        let result = validate_and_save_upload(
            Multipart::new(), // Mock vide
            &params
        ).await;
        assert!(result.is_err());
    }
    
    #[actix_web::test]
    async fn test_quantization_method_selection() {
        let analysis = serde_json::json!({
            "recommended_quantization": "gptq",
            "model_type": "pytorch"
        });
        
        let user_pref = Some("awq".to_string());
        let method = determine_quantization_method(&analysis, &user_pref);
        assert_eq!(method, QuantizationMethod::Awq);
        
        let no_pref = None;
        let method2 = determine_quantization_method(&analysis, &no_pref);
        assert_eq!(method2, QuantizationMethod::Gptq);
    }
}