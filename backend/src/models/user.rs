use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;

/// Représente un utilisateur du système
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Validate)]
pub struct User {
    /// Identifiant unique de l'utilisateur (UUID)
    pub id: Uuid,
    
    /// Email de l'utilisateur (unique) - utilisé pour la connexion
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    
    /// Hash du mot de passe (stocké sécurisé)
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    
    /// Date de création du compte
    pub created_at: DateTime<Utc>,
    
    /// Date de dernière connexion
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Données requises pour créer un nouvel utilisateur
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewUser {
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    
    #[validate(length(min = 8, message = "Le mot de passe doit contenir au moins 8 caractères"))]
    pub password: String,
}

/// Données pour la connexion d'un utilisateur
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UserLogin {
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    
    pub password: String,
}

/// Données pour l'authentification Google
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct GoogleAuth {
    pub google_token: String,
}

/// Token JWT pour l'authentification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Données du profil utilisateur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl User {
    /// Crée un nouvel utilisateur avec un mot de passe hashé
    pub fn new(email: String, password: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            email,
            password_hash: Some(Self::hash_password(password)),
            created_at: Utc::now(),
            last_login_at: None,
        }
    }
    
    /// Crée un utilisateur depuis Google
    pub fn from_google(email: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            email,
            password_hash: None,
            created_at: Utc::now(),
            last_login_at: Some(Utc::now()),
        }
    }
    
    /// Hash un mot de passe avec Argon2
    pub fn hash_password(password: &str) -> String {
        use argon2::{
            password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
            Argon2,
        };
        
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        argon2
            .hash_password(password.as_bytes(), &salt)
            .expect("Erreur lors du hashage du mot de passe")
            .to_string()
    }
    
    /// Vérifie si un mot de passe correspond au hash stocké
    pub fn verify_password(&self, password: &str) -> bool {
        if let Some(hash) = &self.password_hash {
            use argon2::{
                password_hash::{PasswordHash, PasswordVerifier},
                Argon2,
            };
            
            let argon2 = Argon2::default();
            let parsed_hash = PasswordHash::new(hash).expect("Hash invalide");
            argon2
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok()
        } else {
            false // Pour les utilisateurs Google sans mot de passe
        }
    }
    
    /// Convertit en profil public
    pub fn to_profile(&self) -> UserProfile {
        UserProfile {
            id: self.id,
            email: self.email.clone(),
            created_at: self.created_at,
            last_login_at: self.last_login_at,
        }
    }
    
    /// Met à jour la dernière connexion
    pub fn update_last_login(&mut self) {
        self.last_login_at = Some(Utc::now());
    }
}