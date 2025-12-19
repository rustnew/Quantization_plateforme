use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Un fichier modèle
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelFile {
    /// ID unique du fichier
    pub id: Uuid,
    
    /// ID de l'utilisateur propriétaire
    pub user_id: Uuid,
    
    /// Nom original du fichier
    pub original_filename: String,
    
    /// Nom du fichier dans le stockage
    pub storage_filename: String,
    
    /// Taille en octets
    pub file_size: i64,
    
    /// Hash SHA256 pour l'intégrité
    pub checksum_sha256: String,
    
    /// Format du modèle
    pub format: ModelFormat,
    
    /// Type de modèle détecté
    pub model_type: Option<String>,
    
    /// Architecture détectée
    pub architecture: Option<String>,
    
    /// Nombre de paramètres (en milliards)
    pub parameter_count: Option<f64>,
    
    /// Bucket de stockage
    pub storage_bucket: String,
    
    /// Chemin dans le stockage
    pub storage_path: String,
    
    /// Token pour téléchargement (temporaire)
    pub download_token: Option<String>,
    
    /// Expiration du token de téléchargement
    pub download_expires_at: Option<DateTime<Utc>>,
    
    /// Date de création
    pub created_at: DateTime<Utc>,
    
    /// Date d'expiration (nettoyage automatique)
    pub expires_at: Option<DateTime<Utc>>,
}

/// Pour uploader un fichier
#[derive(Debug, Clone, Deserialize)]
pub struct FileUpload {
    pub filename: String,
    pub file_size: i64,
    pub checksum_sha256: String,
    pub format: ModelFormat,
}

/// Pour télécharger un fichier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDownload {
    pub id: Uuid,
    pub filename: String,
    pub file_size: i64,
    pub download_url: String,
    pub expires_at: DateTime<Utc>,
}

/// Métadonnées d'un fichier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: Uuid,
    pub filename: String,
    pub file_size: i64,
    pub format: ModelFormat,
    pub model_type: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<f64>,
    pub created_at: DateTime<Utc>,
}

impl ModelFile {
    /// Crée un nouveau fichier
    pub fn new(
        user_id: Uuid,
        original_filename: String,
        file_size: i64,
        checksum_sha256: String,
        format: ModelFormat,
        storage_bucket: String,
        storage_path: String,
    ) -> Self {
        let storage_filename = format!("{}_{}", Uuid::new_v4(), original_filename);
        
        Self {
            id: Uuid::new_v4(),
            user_id,
            original_filename,
            storage_filename,
            file_size,
            checksum_sha256,
            format,
            model_type: None,
            architecture: None,
            parameter_count: None,
            storage_bucket,
            storage_path,
            download_token: None,
            download_expires_at: None,
            created_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(30)), // Nettoyage après 30 jours
        }
    }
    
    /// Génère un token de téléchargement temporaire
    pub fn generate_download_token(&mut self, validity_hours: i64) -> String {
        use rand::Rng;
        
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        
        self.download_token = Some(token.clone());
        self.download_expires_at = Some(Utc::now() + chrono::Duration::hours(validity_hours));
        
        token
    }
    
    /// Vérifie si le token est valide
    pub fn is_download_token_valid(&self, token: &str) -> bool {
        if let (Some(stored_token), Some(expires_at)) = (&self.download_token, &self.download_expires_at) {
            stored_token == token && Utc::now() < *expires_at
        } else {
            false
        }
    }
    
    /// Met à jour les métadonnées du modèle
    pub fn update_metadata(&mut self, metadata: ModelMetadata) {
        self.model_type = metadata.model_type;
        self.architecture = metadata.architecture;
        self.parameter_count = metadata.parameter_count;
    }
    
    /// Convertit en métadonnées publiques
    pub fn to_metadata(&self) -> FileMetadata {
        FileMetadata {
            id: self.id,
            filename: self.original_filename.clone(),
            file_size: self.file_size,
            format: self.format.clone(),
            model_type: self.model_type.clone(),
            architecture: self.architecture.clone(),
            parameter_count: self.parameter_count,
            created_at: self.created_at,
        }
    }
}

/// Métadonnées extraites d'un modèle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_type: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<f64>,
    pub quantization_bits: Option<i32>,
}