// utils/validation.rs
use crate::utils::error::{AppError, Result};
use validator::Validate;
use std::path::Path;

/// Valider un email
pub fn validate_email(email: &str) -> Result<()> {
    if !validator::validate_email(email) {
        return Err(AppError::Validation("Invalid email format".to_string()));
    }
    Ok(())
}

/// Valider un mot de passe
pub fn validate_password(password: &str) -> Result<()> {
    if password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 characters long".to_string()));
    }
    Ok(())
}

/// Valider un nom de fichier
pub fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty() {
        return Err(AppError::Validation("Filename cannot be empty".to_string()));
    }
    
    if filename.len() > 255 {
        return Err(AppError::Validation("Filename too long (max 255 characters)".to_string()));
    }
    
    // Éviter les chemins relatifs
    if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
        return Err(AppError::Validation("Invalid filename".to_string()));
    }
    
    Ok(())
}

/// Valider une taille de fichier
pub fn validate_file_size(file_size: u64, max_size_mb: u64) -> Result<()> {
    let max_size_bytes = max_size_mb * 1024 * 1024;
    
    if file_size == 0 {
        return Err(AppError::Validation("File cannot be empty".to_string()));
    }
    
    if file_size > max_size_bytes {
        return Err(AppError::FileTooLarge);
    }
    
    Ok(())
}

/// Valider un format de modèle
pub fn validate_model_format(format: &str) -> Result<()> {
    let valid_formats = ["pytorch", "safetensors", "onnx", "gguf"];
    
    if !valid_formats.contains(&format.to_lowercase().as_str()) {
        return Err(AppError::Validation(
            format!("Invalid model format. Must be one of: {}", valid_formats.join(", "))
        ));
    }
    
    Ok(())
}

/// Valider une méthode de quantification
pub fn validate_quantization_method(method: &str) -> Result<()> {
    let valid_methods = ["int8", "gptq", "awq", "gguf_q4_0", "gguf_q5_0"];
    
    if !valid_methods.contains(&method.to_lowercase().as_str()) {
        return Err(AppError::Validation(
            format!("Invalid quantization method. Must be one of: {}", valid_methods.join(", "))
        ));
    }
    
    Ok(())
}

/// Valider un plan d'abonnement
pub fn validate_plan(plan: &str) -> Result<()> {
    let valid_plans = ["free", "starter", "pro"];
    
    if !valid_plans.contains(&plan.to_lowercase().as_str()) {
        return Err(AppError::Validation(
            format!("Invalid plan. Must be one of: {}", valid_plans.join(", "))
        ));
    }
    
    Ok(())
}

/// Valider un UUID
pub fn validate_uuid(uuid_str: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(uuid_str)
        .map_err(|_| AppError::Validation("Invalid UUID format".to_string()))
}

/// Valider une URL
pub fn validate_url(url: &str) -> Result<()> {
    if !validator::validate_url(url) {
        return Err(AppError::Validation("Invalid URL format".to_string()));
    }
    Ok(())
}

/// Valider un chemin de fichier
pub fn validate_file_path(path: &str) -> Result<()> {
    let path_obj = Path::new(path);
    
    if !path_obj.exists() {
        return Err(AppError::Validation("File does not exist".to_string()));
    }
    
    if !path_obj.is_file() {
        return Err(AppError::Validation("Path is not a file".to_string()));
    }
    
    Ok(())
}

/// Valider un nombre positif
pub fn validate_positive_number(value: i64, field_name: &str) -> Result<()> {
    if value <= 0 {
        return Err(AppError::Validation(
            format!("{} must be a positive number", field_name)
        ));
    }
    Ok(())
}

/// Valider un pourcentage (0-100)
pub fn validate_percentage(value: f64, field_name: &str) -> Result<()> {
    if value < 0.0 || value > 100.0 {
        return Err(AppError::Validation(
            format!("{} must be between 0 and 100", field_name)
        ));
    }
    Ok(())
}

/// Valider une chaîne non vide
pub fn validate_non_empty_string(value: &str, field_name: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(
            format!("{} cannot be empty", field_name)
        ));
    }
    Ok(())
}

/// Valider une liste non vide
pub fn validate_non_empty_list<T>(list: &[T], field_name: &str) -> Result<()> {
    if list.is_empty() {
        return Err(AppError::Validation(
            format!("{} cannot be empty", field_name)
        ));
    }
    Ok(())
}

/// Fonction utilitaire pour valider un objet Validate
pub fn validate_object<T: Validate>(obj: &T) -> Result<()> {
    obj.validate()
        .map_err(|e| AppError::Validation(e.to_string()))
}