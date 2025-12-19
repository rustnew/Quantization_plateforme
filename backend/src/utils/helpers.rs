// utils/helpers.rs
use crate::utils::error::{AppError, Result};
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use std::path::{Path, PathBuf};
use std::fs;
use std::io;

/// Générer un UUID v4
pub fn generate_uuid() -> Uuid {
    Uuid::new_v4()
}

/// Formater une date pour l'affichage
pub fn format_date(date: &DateTime<Utc>) -> String {
    date.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Formater une date relative (il y a X temps)
pub fn format_relative_date(date: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now - *date;
    
    if diff.num_days() > 365 {
        let years = diff.num_days() / 365;
        format!("il y a {} an{}", years, if years > 1 { "s" } else { "" })
    } else if diff.num_days() > 30 {
        let months = diff.num_days() / 30;
        format!("il y a {} mois", months)
    } else if diff.num_days() > 0 {
        format!("il y a {} jour{}", diff.num_days(), if diff.num_days() > 1 { "s" } else { "" })
    } else if diff.num_hours() > 0 {
        format!("il y a {} heure{}", diff.num_hours(), if diff.num_hours() > 1 { "s" } else { "" })
    } else if diff.num_minutes() > 0 {
        format!("il y a {} minute{}", diff.num_minutes(), if diff.num_minutes() > 1 { "s" } else { "" })
    } else {
        "à l'instant".to_string()
    }
}

/// Formatter une taille en octets lisible
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let base = 1024_f64;
    let bytes_f64 = bytes as f64;
    let exponent = (bytes_f64.log10() / base.log10()).floor() as i32;
    let unit_index = exponent.min(4).max(0) as usize;
    
    let size = bytes_f64 / base.powi(exponent);
    
    format!("{:.2} {}", size, UNITS[unit_index])
}

/// Calculer un pourcentage
pub fn calculate_percentage(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (part as f64 / total as f64) * 100.0
}

/// Limiter une chaîne de caractères
pub fn truncate_string(s: &str, max_length: usize) -> String {
    if s.len() <= max_length {
        s.to_string()
    } else {
        format!("{}...", &s[..max_length.saturating_sub(3)])
    }
}

/// Nettoyer une chaîne pour un nom de fichier
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | ' ' => c,
            _ => '_',
        })
        .collect()
}

/// Créer un répertoire s'il n'existe pas
pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| AppError::StorageError(e.to_string()))?;
    }
    Ok(())
}

/// Supprimer récursivement un répertoire
pub fn remove_directory(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|e| AppError::StorageError(e.to_string()))?;
    }
    Ok(())
}

/// Lire un fichier en bytes
pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read(path)
        .map_err(|e| AppError::StorageError(e.to_string()))
}

/// Écrire des bytes dans un fichier
pub fn write_file_bytes(path: &Path, data: &[u8]) -> Result<()> {
    fs::write(path, data)
        .map_err(|e| AppError::StorageError(e.to_string()))
}

/// Obtenir la taille d'un fichier
pub fn get_file_size(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| AppError::StorageError(e.to_string()))
}

/// Vérifier si un chemin est un fichier
pub fn is_file(path: &Path) -> bool {
    path.is_file()
}

/// Vérifier si un chemin est un répertoire
pub fn is_directory(path: &Path) -> bool {
    path.is_dir()
}

/// Obtenir l'extension d'un fichier
pub fn get_file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
}

/// Générer un nom de fichier unique
pub fn generate_unique_filename(base_name: &str, extension: &str) -> String {
    let timestamp = Utc::now().timestamp();
    let random_part: u32 = rand::random();
    
    format!("{}_{}_{}.{}", base_name, timestamp, random_part, extension)
}

/// Convertir des secondes en durée lisible
pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{} secondes", seconds)
    } else if seconds < 3600 {
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        format!("{} minutes {} secondes", minutes, remaining_seconds)
    } else {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let remaining_seconds = seconds % 60;
        format!("{} heures {} minutes {} secondes", hours, minutes, remaining_seconds)
    }
}

/// Générer un token CSRF
pub fn generate_csrf_token() -> String {
    use rand::Rng;
    
    let token: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    
    token
}

/// Valider un token CSRF
pub fn validate_csrf_token(token: &str, expected: &str) -> Result<()> {
    if token != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

/// Retarder l'exécution (pour les tests)
pub async fn delay_ms(milliseconds: u64) {
    tokio::time::sleep(tokio::time::Duration::from_millis(milliseconds)).await;
}

/// Exécuter avec timeout
pub async fn with_timeout<F, T>(
    future: F,
    timeout_seconds: u64,
) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::time::timeout(
        tokio::time::Duration::from_secs(timeout_seconds),
        future,
    )
    .await
    .map_err(|_| AppError::ResourceBusy)?
}