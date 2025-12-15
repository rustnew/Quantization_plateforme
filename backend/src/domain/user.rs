

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;
use argon2::{Argon2, password_hash::{PasswordHasher, PasswordVerifier, SaltString, PasswordHash}};
use rand::rngs::OsRng; // Correction 1: Importer OsRng au lieu d'utiliser thread_rng déprécié

/// Représente un utilisateur du système
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Validate)] // Correction 2: Ajouter le derive Validate
pub struct User {
    /// Identifiant unique de l'utilisateur (UUID)
    pub id: Uuid,
    /// Nom complet de l'utilisateur
    #[validate(length(min = 2, message = "Le nom doit contenir au moins 2 caractères"))]
    pub name: String,
    /// Email de l'utilisateur (unique)
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    /// Hash du mot de passe (stocké sécurisé, non exposé dans les APIs)
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    /// Fournisseur d'authentification (email, google, github)
    pub auth_provider: Option<String>,
    /// ID du fournisseur d'authentification (pour les comptes sociaux)
    pub auth_provider_id: Option<String>,
    /// Date de création du compte
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour
    pub updated_at: DateTime<Utc>,
    /// Statut du compte (actif/désactivé)
    pub is_active: bool,
}

/// Données requises pour créer un nouvel utilisateur
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewUser {
    #[validate(length(min = 2, message = "Le nom doit contenir au moins 2 caractères"))]
    pub name: String,
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    #[validate(length(min = 8, message = "Le mot de passe doit contenir au moins 8 caractères"))]
    pub password: Option<String>,
}

/// Données pour la connexion d'un utilisateur
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UserLogin {
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    pub password: String,
}

impl User {
    /// Crée un nouvel utilisateur avec un mot de passe hashé
    pub fn new(name: String, email: String, password: Option<String>) -> Self {
        let password_hash = password.map(|pwd| Self::hash_password(&pwd));
        
        Self {
            id: Uuid::new_v4(),
            name,
            email,
            password_hash,
            auth_provider: Some("email".to_string()),
            auth_provider_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_active: true,
        }
    }

    /// Hash un mot de passe avec Argon2
    pub fn hash_password(password: &str) -> String {
        let argon2 = Argon2::default();
        // Correction 3: Utiliser OsRng au lieu de thread_rng
        let salt = SaltString::generate(rand_core::OsRng);
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .expect("Erreur lors du hashage du mot de passe")
            .to_string();
        
        password_hash
    }

    /// Vérifie si un mot de passe correspond au hash stocké
    pub fn verify_password(&self, password: &str) -> bool {
        if let Some(hash) = &self.password_hash {
            let argon2 = Argon2::default();
            let parsed_hash = PasswordHash::new(hash).expect("Hash invalide");
            argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok()
        } else {
            false
        }
    }

    /// Met à jour le mot de passe de l'utilisateur
    pub fn update_password(&mut self, new_password: &str) {
        self.password_hash = Some(Self::hash_password(new_password));
    }

    /// Crée un utilisateur depuis une authentification sociale (Google/GitHub)
    pub fn from_social_auth(name: String, email: String, provider: &str, provider_id: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            email,
            password_hash: None,
            auth_provider: Some(provider.to_string()),
            auth_provider_id: Some(provider_id),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_active: true,
        }
    }
}
