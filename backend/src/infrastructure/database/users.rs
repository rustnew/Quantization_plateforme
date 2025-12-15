

use std::sync::Arc;
use sqlx::{Pool, Postgres, Error as SqlxError, Row, query_as, query_scalar};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use validator::Validate;
use argon2::{Argon2, password_hash::{PasswordHasher, PasswordVerifier, SaltString, PasswordHash, Error as PasswordError}};

use crate::{
    domain::user::{User, NewUser, UserLogin},
    infrastructure::error::{AppError, AppResult},
};

/// Repository pour les opérations sur les utilisateurs
#[derive(Clone)]
pub struct UserRepository {
    pool: Pool<Postgres>,
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("Utilisateur non trouvé")]
    NotFound,
    #[error("Email déjà utilisé")]
    EmailExists,
    #[error("Authentification échouée")]
    AuthenticationFailed,
    #[error("Validation échouée: {0}")]
    ValidationError(#[from] validator::ValidationErrors),
    #[error("Erreur de base de données: {0}")]
    DatabaseError(#[from] SqlxError),
    #[error("Erreur de hashage de mot de passe: {0}")]
    PasswordHashError(#[from] PasswordError),
}

impl From<UserError> for AppError {
    fn from(error: UserError) -> Self {
        match error {
            UserError::NotFound => AppError::NotFound("Utilisateur".to_string()),
            UserError::EmailExists => AppError::Conflict("Email déjà utilisé".to_string()),
            UserError::AuthenticationFailed => AppError::Unauthorized("Authentification échouée".to_string()),
            UserError::ValidationError(errors) => AppError::ValidationError(errors),
            UserError::DatabaseError(e) => AppError::DatabaseError(e),
            UserError::PasswordHashError(e) => AppError::InternalError(format!("Erreur de hashage: {}", e)),
        }
    }
}

impl UserRepository {
    /// Crée une nouvelle instance du repository
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Crée un nouvel utilisateur dans la base de données
    /// 
    /// # Arguments
    /// * `new_user` - Les données du nouvel utilisateur
    /// 
    /// # Retourne
    /// * `Ok(User)` - L'utilisateur créé avec son ID généré
    /// * `Err(AppError)` - En cas d'erreur (email existant, validation échouée, etc.)
    pub async fn create(&self, new_user: &NewUser) -> AppResult<User> {
        // Validation des données d'entrée
        new_user.validate().map_err(UserError::ValidationError)?;

        // Vérifier si l'email existe déjà
        if self.email_exists(&new_user.email).await? {
            return Err(UserError::EmailExists.into());
        }

        // Hasher le mot de passe si fourni
        let password_hash = new_user.password.as_ref()
            .map(|pwd| Self::hash_password(pwd))
            .transpose()?;

        let user_id = Uuid::new_v4();
        let now = Utc::now();

        // Créer l'utilisateur dans la base de données
        let user = query_as!(
            User,
            r#"
            INSERT INTO users (
                id, name, email, password_hash, auth_provider, 
                auth_provider_id, created_at, updated_at, is_active
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, name, email, password_hash, auth_provider, 
                     auth_provider_id, created_at, updated_at, is_active
            "#,
            user_id,
            new_user.name,
            new_user.email,
            password_hash,
            Some("email".to_string()), // auth_provider par défaut
            None::<String>, // auth_provider_id
            now,
            now,
            true
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    /// Récupère un utilisateur par son ID
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(User)` - L'utilisateur trouvé
    /// * `Err(AppError)` - Si l'utilisateur n'existe pas ou erreur de base de données
    pub async fn get_by_id(&self, user_id: &Uuid) -> AppResult<User> {
        let user = query_as!(
            User,
            r#"
            SELECT id, name, email, password_hash, auth_provider, 
                   auth_provider_id, created_at, updated_at, is_active
            FROM users
            WHERE id = $1 AND is_active = true
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;

        Ok(user)
    }

    /// Récupère un utilisateur par son email
    /// 
    /// # Arguments
    /// * `email` - L'email de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(User)` - L'utilisateur trouvé
    /// * `Err(AppError)` - Si l'utilisateur n'existe pas ou erreur de base de données
    pub async fn get_by_email(&self, email: &str) -> AppResult<User> {
        let user = query_as!(
            User,
            r#"
            SELECT id, name, email, password_hash, auth_provider, 
                   auth_provider_id, created_at, updated_at, is_active
            FROM users
            WHERE email = $1 AND is_active = true
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UserError::NotFound)?;

        Ok(user)
    }

    /// Authentifie un utilisateur avec email et mot de passe
    /// 
    /// # Arguments
    /// * `email` - L'email de l'utilisateur
    /// * `password` - Le mot de passe en clair
    /// 
    /// # Retourne
    /// * `Ok(User)` - L'utilisateur authentifié
    /// * `Err(AppError)` - Si l'authentification échoue ou erreur de base de données
    pub async fn authenticate(&self, email: &str, password: &str) -> AppResult<User> {
        let user = self.get_by_email(email).await?;

        // Vérifier si l'utilisateur a un mot de passe (certains peuvent être authentifiés via OAuth)
        if let Some(hash) = &user.password_hash {
            if Self::verify_password(password, hash)? {
                return Ok(user);
            }
        }

        Err(UserError::AuthenticationFailed.into())
    }

    /// Met à jour les informations d'un utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// * `update_data` - Les données à mettre à jour
    /// 
    /// # Retourne
    /// * `Ok(User)` - L'utilisateur mis à jour
    /// * `Err(AppError)` - Si l'utilisateur n'existe pas ou erreur de base de données
    pub async fn update(&self, user_id: &Uuid, update_ &UserUpdate) -> AppResult<User> {
        // Récupérer l'utilisateur existant pour les données non modifiées
        let mut existing_user = self.get_by_id(user_id).await?;
        
        // Mettre à jour les champs modifiables
        if let Some(name) = &update_data.name {
            existing_user.name = name.clone();
        }
        
        if let Some(email) = &update_data.email {
            // Vérifier si le nouvel email existe déjà pour un autre utilisateur
            if self.email_exists_for_other_user(email, user_id).await? {
                return Err(UserError::EmailExists.into());
            }
            existing_user.email = email.to_string();
        }
        
        if let Some(new_password) = &update_data.new_password {
            existing_user.update_password(new_password);
        }
        
        let now = Utc::now();

        // Mettre à jour dans la base de données
        let updated_user = query_as!(
            User,
            r#"
            UPDATE users
            SET name = $1, email = $2, password_hash = $3, updated_at = $4
            WHERE id = $5
            RETURNING id, name, email, password_hash, auth_provider, 
                     auth_provider_id, created_at, updated_at, is_active
            "#,
            existing_user.name,
            existing_user.email,
            existing_user.password_hash,
            now,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_user)
    }

    /// Désactive un utilisateur (soft delete)
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(())` - Si l'opération réussit
    /// * `Err(AppError)` - Si l'utilisateur n'existe pas ou erreur de base de données
    pub async fn deactivate(&self, user_id: &Uuid) -> AppResult<()> {
        let result = sqlx::query!(
            r#"
            UPDATE users
            SET is_active = false, updated_at = $1
            WHERE id = $2
            "#,
            Utc::now(),
            user_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(UserError::NotFound.into());
        }

        Ok(())
    }

    /// Vérifie si un email existe déjà dans la base de données
    async fn email_exists(&self, email: &str) -> AppResult<bool> {
        let exists = query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users 
                WHERE email = $1 AND is_active = true
            )
            "#,
            email
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    /// Vérifie si un email existe pour un autre utilisateur que celui spécifié
    async fn email_exists_for_other_user(&self, email: &str, user_id: &Uuid) -> AppResult<bool> {
        let exists = query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users 
                WHERE email = $1 AND id != $2 AND is_active = true
            )
            "#,
            email,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    /// Hash un mot de passe avec Argon2
    fn hash_password(password: &str) -> Result<String, PasswordError> {
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut rand::thread_rng());
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        
        Ok(password_hash)
    }

    /// Vérifie si un mot de passe correspond au hash stocké
    fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
        let argon2 = Argon2::default();
        let parsed_hash = PasswordHash::new(hash)?;
        Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }

    /// Crée un utilisateur depuis une authentification sociale
    pub async fn create_from_social_auth(
        &self, 
        name: String, 
        email: String, 
        provider: &str, 
        provider_id: String
    ) -> AppResult<User> {
        // Validation basique
        if name.len() < 2 {
            return Err(UserError::ValidationError(
                validator::ValidationError::new("name").into()
            ).into());
        }

        // Vérifier si l'email existe déjà
        if self.email_exists(&email).await? {
            return Err(UserError::EmailExists.into());
        }

        let user_id = Uuid::new_v4();
        let now = Utc::now();

        let user = query_as!(
            User,
            r#"
            INSERT INTO users (
                id, name, email, password_hash, auth_provider, 
                auth_provider_id, created_at, updated_at, is_active
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, name, email, password_hash, auth_provider, 
                     auth_provider_id, created_at, updated_at, is_active
            "#,
            user_id,
            name,
            email,
            None::<String>, // Pas de mot de passe pour l'auth sociale
            Some(provider.to_string()),
            Some(provider_id),
            now,
            now,
            true
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }
}

/// Données pour mettre à jour un utilisateur
#[derive(Debug, Clone, Default)]
pub struct UserUpdate {
    pub name: Option<String>,
    pub email: Option<String>,
    pub new_password: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::Database;
    use sqlx::PgPool;
    use std::env;

    async fn setup_test_db() -> PgPool {
        // Utiliser une base de données de test
        let database_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://quant_user:quant_pass@localhost:5432/quant_test".to_string());
        
        PgPool::connect(&database_url).await.unwrap()
    }

    async fn clear_users_table(pool: &PgPool) {
        sqlx::query("DELETE FROM users WHERE email LIKE '%@test.com'")
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test]
    async fn test_user_creation_and_retrieval() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool.clone());
        
        // Nettoyer avant le test
        clear_users_table(&pool).await;
        
        // Créer un nouvel utilisateur
        let new_user = NewUser {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
            password: Some("securepassword123".to_string()),
        };
        
        let created_user = repo.create(&new_user).await.unwrap();
        
        // Vérifier que l'utilisateur a été créé correctement
        assert_eq!(created_user.name, "Test User");
        assert_eq!(created_user.email, "test@example.com");
        assert!(created_user.password_hash.is_some());
        assert!(created_user.is_active);
        
        // Récupérer l'utilisateur par ID
        let retrieved_user = repo.get_by_id(&created_user.id).await.unwrap();
        assert_eq!(retrieved_user.id, created_user.id);
        assert_eq!(retrieved_user.email, "test@example.com");
        
        // Récupérer l'utilisateur par email
        let retrieved_by_email = repo.get_by_email("test@example.com").await.unwrap();
        assert_eq!(retrieved_by_email.id, created_user.id);
        
        // Vérifier l'authentification
        let authenticated_user = repo.authenticate("test@example.com", "securepassword123").await.unwrap();
        assert_eq!(authenticated_user.id, created_user.id);
        
        // Vérifier que l'authentification échoue avec un mauvais mot de passe
        let auth_result = repo.authenticate("test@example.com", "wrongpassword").await;
        assert!(auth_result.is_err());
    }

    #[sqlx::test]
    async fn test_user_update() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool.clone());
        
        clear_users_table(&pool).await;
        
        // Créer un utilisateur
        let new_user = NewUser {
            name: "Original Name".to_string(),
            email: "update@test.com".to_string(),
            password: Some("oldpassword".to_string()),
        };
        
        let user = repo.create(&new_user).await.unwrap();
        
        // Mettre à jour l'utilisateur
        let update = UserUpdate {
            name: Some("Updated Name".to_string()),
            email: Some("newemail@test.com".to_string()),
            new_password: Some("newpassword".to_string()),
        };
        
        let updated_user = repo.update(&user.id, &update).await.unwrap();
        
        assert_eq!(updated_user.name, "Updated Name");
        assert_eq!(updated_user.email, "newemail@test.com");
        
        // Vérifier que le nouveau mot de passe fonctionne
        let auth_result = repo.authenticate("newemail@test.com", "newpassword").await;
        assert!(auth_result.is_ok());
        
        // Vérifier que l'ancien mot de passe ne fonctionne plus
        let old_auth_result = repo.authenticate("newemail@test.com", "oldpassword").await;
        assert!(old_auth_result.is_err());
    }

    #[sqlx::test]
    async fn test_social_auth_user_creation() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool.clone());
        
        clear_users_table(&pool).await;
        
        // Créer un utilisateur via auth sociale
        let user = repo.create_from_social_auth(
            "Google User".to_string(),
            "google@test.com".to_string(),
            "google",
            "google_123456".to_string()
        ).await.unwrap();
        
        assert_eq!(user.name, "Google User");
        assert_eq!(user.email, "google@test.com");
        assert!(user.password_hash.is_none());
        assert_eq!(user.auth_provider.unwrap(), "google");
        assert_eq!(user.auth_provider_id.unwrap(), "google_123456");
        
        // Vérifier que l'utilisateur est récupérable
        let retrieved = repo.get_by_email("google@test.com").await.unwrap();
        assert_eq!(retrieved.id, user.id);
    }
}