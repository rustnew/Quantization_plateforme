// utils/security.rs
use crate::utils::error::{AppError, Result};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, TokenData};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Claims JWT pour les tokens d'accès
#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: Uuid,        // User ID
    pub email: String,    // User email
    pub exp: usize,       // Expiration timestamp
    pub iat: usize,       // Issued at timestamp
    pub jti: String,      // Token ID (pour invalidation)
}

/// Claims JWT pour les refresh tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenClaims {
    pub sub: Uuid,        // User ID
    pub exp: usize,       // Expiration timestamp
    pub iat: usize,       // Issued at timestamp
    pub jti: String,      // Token ID
}

/// Générer un token d'accès JWT
pub fn generate_access_token(user_id: Uuid, email: &str, secret: &str) -> String {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::hours(2);
    
    let claims = AccessTokenClaims {
        sub: user_id,
        email: email.to_string(),
        exp: expires_at.timestamp() as usize,
        iat: now.timestamp() as usize,
        jti: Uuid::new_v4().to_string(),
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to generate access token")
}

/// Générer un refresh token JWT
pub fn generate_refresh_token(user_id: Uuid, secret: &str) -> String {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::days(7);
    
    let claims = RefreshTokenClaims {
        sub: user_id,
        exp: expires_at.timestamp() as usize,
        iat: now.timestamp() as usize,
        jti: Uuid::new_v4().to_string(),
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to generate refresh token")
}

/// Vérifier un token d'accès
pub fn verify_access_token(token: &str, secret: &str) -> Result<TokenData<AccessTokenClaims>> {
    let token_data = decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::InvalidToken)?;
    
    Ok(token_data)
}

/// Vérifier un refresh token
pub fn verify_refresh_token(token: &str, secret: &str) -> Result<TokenData<RefreshTokenClaims>> {
    let token_data = decode::<RefreshTokenClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::InvalidToken)?;
    
    Ok(token_data)
}

/// Générer un hash de mot de passe avec Argon2
pub fn hash_password(password: &str) -> Result<String> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };
    
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::EncryptionError(e.to_string()))
}

/// Vérifier un mot de passe contre un hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    
    let argon2 = Argon2::default();
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| AppError::EncryptionError(e.to_string()))?;
    
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Générer une clé API sécurisée
pub fn generate_api_key() -> String {
    format!("qnt_{}", generate_random_string(32))
}

/// Générer un token de réinitialisation de mot de passe
pub fn generate_reset_token() -> String {
    generate_random_string(32)
}

/// Générer une chaîne aléatoire
pub fn generate_random_string(length: usize) -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// Chiffrer des données avec AES-256-GCM
pub fn encrypt_data(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::{
        aead::{Aead, KeyInit, OsRng},
        Aes256Gcm, Nonce,
    };
    
    if key.len() < 32 {
        return Err(AppError::EncryptionError("Key must be at least 32 bytes".to_string()));
    }
    
    let cipher = Aes256Gcm::new_from_slice(&key[..32])
        .map_err(|e| AppError::EncryptionError(e.to_string()))?;
    
    let nonce = Nonce::from_slice(&key[..12]);
    
    cipher.encrypt(nonce, data)
        .map_err(|e| AppError::EncryptionError(e.to_string()))
}

/// Déchiffrer des données avec AES-256-GCM
pub fn decrypt_data(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    
    if key.len() < 32 {
        return Err(AppError::EncryptionError("Key must be at least 32 bytes".to_string()));
    }
    
    let cipher = Aes256Gcm::new_from_slice(&key[..32])
        .map_err(|e| AppError::EncryptionError(e.to_string()))?;
    
    let nonce = Nonce::from_slice(&key[..12]);
    
    cipher.decrypt(nonce, encrypted)
        .map_err(|e| AppError::EncryptionError(e.to_string()))
}

/// Calculer un hash SHA256
pub fn sha256_hash(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Valider la force d'un mot de passe
pub fn validate_password_strength(password: &str) -> Result<()> {
    if password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 characters long".to_string()));
    }
    
    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_digit = password.chars().any(|c| c.is_digit(10));
    let has_special = password.chars().any(|c| !c.is_alphanumeric());
    
    let score = [has_lowercase, has_uppercase, has_digit, has_special]
        .iter()
        .filter(|&&x| x)
        .count();
    
    if score < 3 {
        return Err(AppError::Validation(
            "Password must contain at least 3 of: lowercase, uppercase, digits, special characters".to_string()
        ));
    }
    
    Ok(())
}