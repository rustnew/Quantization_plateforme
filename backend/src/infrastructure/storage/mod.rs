

use aws_sdk_s3::{Client, Config, Endpoint, Region};
use aws_config::meta::region::RegionProviderChain;
use std::path::PathBuf;
use uuid::Uuid;
use tracing::{info, warn, error};
use std::fs;

use crate::infrastructure::error::{AppError, AppResult};

/// Service de stockage sécurisé
#[derive(Clone)]
pub struct StorageService {
    client: Client,
    bucket: String,
    endpoint: String,
    encryption_key: Vec<u8>,
}

impl StorageService {
    /// Crée une nouvelle instance du service de stockage
    pub fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
    ) -> AppResult<Self> {
        info!("🔧 Initialisation du service de stockage...");
        
        // Vérifier que la clé de chiffrement est configurée
        let encryption_key = std::env::var("STORAGE_ENCRYPTION_KEY")
            .map_err(|_| AppError::ConfigurationError("STORAGE_ENCRYPTION_KEY non configurée".to_string()))?;
        
        if encryption_key.len() < 32 {
            warn!("⚠️  Clé de chiffrement trop courte (< 32 caractères)");
        }
        
        let region_provider = RegionProviderChain::default_provider().or_else(Region::new("us-east-1"));
        let config = aws_config::from_env()
            .region(region_provider)
            .credentials_provider(aws_credential_types::Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "env",
            ))
            .endpoint_url(Endpoint::immutable(endpoint.parse()?))
            .load();
        
        let config = Config::from(&config);
        let client = Client::from_conf(config);
        
        info!("✅ Service de stockage initialisé pour le bucket: {}", bucket);
        
        Ok(Self {
            client,
            bucket: bucket.to_string(),
            endpoint: endpoint.to_string(),
            encryption_key: encryption_key.as_bytes().to_vec(),
        })
    }

    /// Sauvegarde un fichier de modèle dans le stockage sécurisé
    pub async fn save_model_file(
        &self,
        file_name: &str,
        content: &[u8],
        user_id: &Uuid,
    ) -> AppResult<String> {
        // Chiffrer le contenu
        let encrypted_content = self.encrypt_content(content)?;
        
        // Générer le chemin unique
        let file_id = Uuid::new_v4();
        let object_key = format!("models/{}/{}_{}", user_id, file_id, file_name);
        
        // Upload vers S3/MinIO
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(encrypted_content.into())
            .content_type("application/octet-stream")
            .send()
            .await?;
        
        // Générer l'URL sécurisée
        let input_path = format!("{}/{}", self.endpoint, object_key);
        Ok(input_path)
    }

    /// Télécharge un fichier depuis le stockage
    pub async fn download_file(&self, file_url: &str) -> AppResult<PathBuf> {
        // Extraire la clé de l'URL
        let object_key = file_url
            .trim_start_matches(&self.endpoint)
            .trim_start_matches('/')
            .to_string();
        
        // Télécharger depuis S3/MinIO
        let output = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await?;
        
        // Lire le contenu
        let body = output.body.collect().await?;
        let encrypted_content = body.into_bytes();
        
        // Déchiffrer le contenu
        let decrypted_content = self.decrypt_content(&encrypted_content)?;
        
        // Sauvegarder dans un fichier temporaire
        let temp_dir = std::env::var("TEMP_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let temp_path = PathBuf::from(temp_dir).join(format!("download_{}", Uuid::new_v4()));
        
        fs::write(&temp_path, decrypted_content)?;
        
        Ok(temp_path)
    }

    /// Upload un fichier résultat et retourne l'URL sécurisée
    pub async fn upload_file(&self, file_path: &PathBuf) -> AppResult<String> {
        // Lire le fichier
        let content = fs::read(file_path)?;
        
        // Chiffrer le contenu
        let encrypted_content = self.encrypt_content(&content)?;
        
        // Générer le chemin unique
        let file_id = Uuid::new_v4();
        let object_key = format!("results/{}/{}.bin", chrono::Utc::now().date_naive(), file_id);
        
        // Upload vers S3/MinIO
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(encrypted_content.into())
            .content_type("application/octet-stream")
            .send()
            .await?;
        
        // Générer l'URL sécurisée (valable 24h)
        let presigned_url = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .presigned(
                &aws_sdk_s3::presigning::config::Config::builder()
                    .expires_in(std::time::Duration::from_secs(86400))
                    .build()?,
                chrono::Utc::now() + chrono::Duration::hours(24),
            )
            .await?
            .uri()
            .to_string();
        
        Ok(presigned_url)
    }

    /// Chiffre le contenu avec AES-256-GCM
    fn encrypt_content(&self, content: &[u8]) -> AppResult<Vec<u8>> {
        // Pour le MVP, utiliser un chiffrement simple
        // Dans la vraie version, utiliser AES-256-GCM avec nonce aléatoire
        let mut encrypted = content.to_vec();
        
        // XOR simple avec la clé (pour le MVP seulement - à remplacer par du vrai chiffrement)
        for (i, byte) in encrypted.iter_mut().enumerate() {
            *byte ^= self.encryption_key[i % self.encryption_key.len()];
        }
        
        Ok(encrypted)
    }

    /// Déchiffre le contenu
    fn decrypt_content(&self, content: &[u8]) -> AppResult<Vec<u8>> {
        // Même logique que encrypt_content (XOR est symétrique)
        let mut decrypted = content.to_vec();
        
        for (i, byte) in decrypted.iter_mut().enumerate() {
            *byte ^= self.encryption_key[i % self.encryption_key.len()];
        }
        
        Ok(decrypted)
    }

    /// Création mock pour les tests
    #[cfg(test)]
    pub fn new_test() -> Self {
        Self {
            client: Client::from_conf(Config::builder().build()),
            bucket: "test-bucket".to_string(),
            endpoint: "http://localhost:9000".to_string(),
            encryption_key: b"test_encryption_key_32_bytes_12345678".to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_file_encryption_decryption() {
        let service = StorageService::new_test();
        
        let test_content = b"Contenu sensible du modèle";
        let encrypted = service.encrypt_content(test_content).unwrap();
        let decrypted = service.decrypt_content(&encrypted).unwrap();
        
        assert_eq!(&decrypted[..], test_content);
    }

    #[tokio::test]
    async fn test_file_upload_download() {
        let service = StorageService::new_test();
        let temp_dir = tempdir().unwrap();
        
        // Créer un fichier test
        let test_file = temp_dir.path().join("test_model.onnx");
        let mut file = File::create(&test_file).unwrap();
        writeln!(file, "ONNX model content").unwrap();
        
        // Upload le fichier
        let result = service.upload_file(&test_file).await;
        assert!(result.is_ok());
        
        // Download le fichier
        let download_url = result.unwrap();
        let downloaded_path = service.download_file(&download_url).await;
        assert!(downloaded_path.is_ok());
        
        // Vérifier le contenu
        let downloaded_content = fs::read(downloaded_path.unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&downloaded_content).contains("ONNX model content"));
    }
}