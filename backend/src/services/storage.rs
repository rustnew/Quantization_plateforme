// services/storage.rs
use crate::models::{ModelFile, FileMetadata, ModelFormat};
use crate::utils::error::{AppError, Result};
use aws_sdk_s3::{
    Client as S3Client,
    config::{Credentials, Region},
    types::{ByteStream, CompletedPart},
    primitives::ByteStream as S3ByteStream,
};
use uuid::Uuid;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct FileStorage {
    s3_client: Option<S3Client>,
    local_dir: PathBuf,
    bucket: String,
    encryption_key: Option<Vec<u8>>,
    max_file_size: u64,
}

impl FileStorage {
    /// Créer un nouveau service de stockage
    pub fn new(
        endpoint: Option<&str>,
        access_key: Option<&str>,
        secret_key: Option<&str>,
        bucket: &str,
        local_dir: Option<&Path>,
        encryption_key: Option<&str>,
        max_file_size_mb: u64,
    ) -> Self {
        let s3_client = if let (Some(endpoint), Some(access_key), Some(secret_key)) = 
            (endpoint, access_key, secret_key) 
        {
            Some(Self::create_s3_client(endpoint, access_key, secret_key))
        } else {
            None
        };

        let local_dir = local_dir
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./storage"));

        let encryption_key = encryption_key
            .map(|k| k.as_bytes().to_vec());

        Self {
            s3_client,
            local_dir,
            bucket: bucket.to_string(),
            encryption_key,
            max_file_size: max_file_size_mb * 1024 * 1024,
        }
    }

    /// Créer le client S3
    fn create_s3_client(endpoint: &str, access_key: &str, secret_key: &str) -> S3Client {
        let creds = Credentials::new(access_key, secret_key, None, None, "minio");
        
        let config = aws_sdk_s3::Config::builder()
            .credentials_provider(creds)
            .endpoint_url(endpoint)
            .region(Region::new("us-east-1"))
            .force_path_style(true)
            .build();

        S3Client::from_conf(config)
    }

    /// Uploader un fichier
    pub async fn upload_file(
        &self,
        user_id: Uuid,
        filename: &str,
        data: &[u8],
        checksum: &str,
        format: ModelFormat,
    ) -> Result<FileMetadata> {
        // Vérifier la taille
        if data.len() as u64 > self.max_file_size {
            return Err(AppError::FileTooLarge);
        }

        // Générer un nom de fichier unique
        let file_id = Uuid::new_v4();
        let storage_filename = format!("{}_{}", file_id, filename);
        
        // Chiffrer les données si nécessaire
        let data_to_store = if let Some(key) = &self.encryption_key {
            self.encrypt_data(data, key)?
        } else {
            data.to_vec()
        };

        // Stocker le fichier
        let storage_path = if let Some(client) = &self.s3_client {
            self.upload_to_s3(&storage_filename, &data_to_store).await?
        } else {
            self.save_locally(&storage_filename, &data_to_store).await?
        };

        // Créer les métadonnées
        let file = ModelFile::new(
            user_id,
            filename.to_string(),
            data.len() as i64,
            checksum.to_string(),
            format,
            self.bucket.clone(),
            storage_path,
        );

        Ok(file.to_metadata())
    }

    /// Uploader vers S3/MinIO
    async fn upload_to_s3(&self, filename: &str, data: &[u8]) -> Result<String> {
        let client = self.s3_client.as_ref().unwrap();
        
        // Vérifier que le bucket existe
        self.ensure_bucket_exists().await?;

        let stream = ByteStream::from(data.to_vec());
        
        client
            .put_object()
            .bucket(&self.bucket)
            .key(filename)
            .body(stream)
            .send()
            .await
            .map_err(|e| AppError::StorageError(e.to_string()))?;

        Ok(filename.to_string())
    }

    /// Sauvegarder localement
    async fn save_locally(&self, filename: &str, data: &[u8]) -> Result<String> {
        // Créer le dossier si nécessaire
        fs::create_dir_all(&self.local_dir).await
            .map_err(|e| AppError::StorageError(e.to_string()))?;

        let file_path = self.local_dir.join(filename);
        
        let mut file = fs::File::create(&file_path).await
            .map_err(|e| AppError::StorageError(e.to_string()))?;
        
        file.write_all(data).await
            .map_err(|e| AppError::StorageError(e.to_string()))?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// Télécharger un fichier
    pub async fn download_file(&self, file: &ModelFile) -> Result<Vec<u8>> {
        let data = if let Some(client) = &self.s3_client {
            self.download_from_s3(&file.storage_path).await?
        } else {
            self.read_locally(&file.storage_path).await?
        };

        // Déchiffrer si nécessaire
        if let Some(key) = &self.encryption_key {
            self.decrypt_data(&data, key)
        } else {
            Ok(data)
        }
    }

    /// Télécharger depuis S3
    async fn download_from_s3(&self, key: &str) -> Result<Vec<u8>> {
        let client = self.s3_client.as_ref().unwrap();
        
        let response = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::StorageError(e.to_string()))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| AppError::StorageError(e.to_string()))?
            .to_vec();

        Ok(bytes)
    }

    /// Lire localement
    async fn read_locally(&self, path: &str) -> Result<Vec<u8>> {
        fs::read(path).await
            .map_err(|e| AppError::StorageError(e.to_string()))
    }

    /// Supprimer un fichier
    pub async fn delete_file(&self, file: &ModelFile) -> Result<()> {
        if let Some(client) = &self.s3_client {
            client
                .delete_object()
                .bucket(&self.bucket)
                .key(&file.storage_path)
                .send()
                .await
                .map_err(|e| AppError::StorageError(e.to_string()))?;
        } else {
            fs::remove_file(&file.storage_path).await
                .map_err(|e| AppError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    /// Générer une URL de téléchargement signée
    pub async fn generate_download_url(&self, file: &ModelFile, expires_in_hours: u32) -> Result<String> {
        if let Some(client) = &self.s3_client {
            let presigned_request = client
                .get_object()
                .bucket(&self.bucket)
                .key(&file.storage_path)
                .presigned(
                    aws_sdk_s3::presigning::PresigningConfig::expires_in(
                        std::time::Duration::from_secs(expires_in_hours as u64 * 3600)
                    )
                    .map_err(|e| AppError::StorageError(e.to_string()))?,
                )
                .await
                .map_err(|e| AppError::StorageError(e.to_string()))?;

            Ok(presigned_request.uri().to_string())
        } else {
            // Pour le stockage local, on retourne un chemin relatif
            Ok(format!("/download/{}", file.id))
        }
    }

    /// Obtenir les métadonnées d'un fichier
    pub async fn get_file_metadata(&self, file_id: Uuid) -> Result<FileMetadata> {
        // Dans une vraie implémentation, on récupérerait depuis la base
        // Pour le MVP, on simule
        Ok(FileMetadata {
            id: file_id,
            filename: "model.bin".to_string(),
            file_size: 1024 * 1024 * 100, // 100MB
            format: ModelFormat::PyTorch,
            model_type: Some("llama".to_string()),
            architecture: Some("llama-2-7b".to_string()),
            parameter_count: Some(7.0),
            created_at: chrono::Utc::now(),
        })
    }

    /// Vérifier que le bucket existe
    async fn ensure_bucket_exists(&self) -> Result<()> {
        if let Some(client) = &self.s3_client {
            match client
                .head_bucket()
                .bucket(&self.bucket)
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(_) => {
                    // Le bucket n'existe pas, le créer
                    client
                        .create_bucket()
                        .bucket(&self.bucket)
                        .send()
                        .await
                        .map_err(|e| AppError::StorageError(e.to_string()))?;
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }

    /// Chiffrer des données
    fn encrypt_data(&self, data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| AppError::EncryptionError(e.to_string()))?;
        
        let nonce = Nonce::from_slice(&key[..12]);
        
        cipher.encrypt(nonce, data)
            .map_err(|e| AppError::EncryptionError(e.to_string()))
    }

    /// Déchiffrer des données
    fn decrypt_data(&self, encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| AppError::EncryptionError(e.to_string()))?;
        
        let nonce = Nonce::from_slice(&key[..12]);
        
        cipher.decrypt(nonce, encrypted)
            .map_err(|e| AppError::EncryptionError(e.to_string()))
    }

    /// Nettoyer les fichiers temporaires
    pub async fn cleanup_temp_files(&self, max_age_days: i64) -> Result<u64> {
        let mut deleted = 0;
        
        if let Ok(mut entries) = fs::read_dir(&self.local_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let metadata = entry.metadata().await.ok();
                let is_temp = entry.file_name().to_string_lossy().contains("temp_");
                let is_old = metadata
                    .and_then(|m| m.modified().ok())
                    .map(|modified| {
                        let age = std::time::SystemTime::now()
                            .duration_since(modified)
                            .unwrap_or_default();
                        age.as_secs() > (max_age_days as u64 * 24 * 60 * 60)
                    })
                    .unwrap_or(false);

                if is_temp && is_old {
                    let _ = fs::remove_file(entry.path()).await;
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }
}