// core/user_service.rs
use crate::models::{
    User, NewUser, UserProfile, AuthToken, 
    Subscription, SubscriptionPlan
};
use crate::services::database::Database;
use crate::services::cache::Cache;
use crate::utils::error::{AppError, Result};
use crate::utils::security::{jwt, password};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct UserService {
    db: Arc<Database>,
    cache: Arc<Cache>,
    jwt_secret: String,
    admin_email: String,
    admin_password: String,
}

impl UserService {
    pub fn new(
        db: Arc<Database>,
        cache: Arc<Cache>,
        jwt_secret: String,
        admin_email: String,
        admin_password: String,
    ) -> Self {
        Self {
            db,
            cache,
            jwt_secret,
            admin_email,
            admin_password,
        }
    }

    /// Inscription d'un nouvel utilisateur
    pub async fn register_user(&self, email: &str, password: &str) -> Result<User> {
        // Vérifier si l'utilisateur existe déjà
        if self.db.user_exists_by_email(email).await? {
            return Err(AppError::UserAlreadyExists);
        }

        // Créer l'utilisateur
        let user = User::new(
            email.to_string(),
            password,
        );

        // Sauvegarder en base
        let user = self.db.create_user(&user).await?;

        // Créer un abonnement gratuit par défaut
        let subscription = Subscription::new_free(user.id);
        self.db.create_subscription(&subscription).await?;

        // Créer des crédits initiaux
        self.db.create_credit_transaction(
            user.id,
            "initial",
            1, // 1 crédit gratuit
            "Crédit initial pour plan gratuit",
        ).await?;

        Ok(user)
    }

    /// Authentification email/mot de passe
    pub async fn authenticate_user(&self, email: &str, password: &str) -> Result<User> {
        let user = self.db.get_user_by_email(email).await?;

        if !user.verify_password(password) {
            return Err(AppError::Unauthorized);
        }

        // Mettre à jour la dernière connexion
        self.update_last_login(user.id).await?;

        Ok(user)
    }

    /// Connexion/inscription avec Google
    pub async fn get_or_create_google_user(&self, email: &str, name: &str) -> Result<User> {
        // Essayer de récupérer l'utilisateur existant
        match self.db.get_user_by_email(email).await {
            Ok(user) => {
                self.update_last_login(user.id).await?;
                Ok(user)
            }
            Err(AppError::UserNotFound) => {
                // Créer un nouvel utilisateur Google
                let user = User::from_google(email.to_string());
                let user = self.db.create_user(&user).await?;

                // Créer un abonnement gratuit
                let subscription = Subscription::new_free(user.id);
                self.db.create_subscription(&subscription).await?;

                // Crédits initiaux
                self.db.create_credit_transaction(
                    user.id,
                    "initial",
                    1,
                    "Crédit initial pour utilisateur Google",
                ).await?;

                Ok(user)
            }
            Err(e) => Err(e),
        }
    }

    /// Générer un token JWT
    pub async fn generate_auth_token(&self, user: &User) -> AuthToken {
        let access_token = jwt::generate_access_token(
            user.id,
            &user.email,
            &self.jwt_secret,
        );

        let refresh_token = jwt::generate_refresh_token(
            user.id,
            &self.jwt_secret,
        );

        AuthToken {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 7200, // 2 heures en secondes
        }
    }

    /// Rafraîchir un token
    pub async fn refresh_auth_token(&self, refresh_token: &str) -> Result<AuthToken> {
        let claims = jwt::verify_refresh_token(refresh_token, &self.jwt_secret)?;
        
        let user = self.db.get_user_by_id(claims.user_id).await?;
        
        // Générer de nouveaux tokens
        let auth_token = self.generate_auth_token(&user).await;
        
        Ok(auth_token)
    }

    /// Mettre à jour la dernière connexion
    pub async fn update_last_login(&self, user_id: Uuid) -> Result<()> {
        self.db.update_user_last_login(user_id).await
    }

    /// Obtenir le profil utilisateur
    pub async fn get_user_profile(&self, user_id: Uuid) -> Result<UserProfile> {
        let user = self.db.get_user_by_id(user_id).await?;
        Ok(user.to_profile())
    }

    /// Obtenir l'abonnement utilisateur
    pub async fn get_user_subscription(&self, user_id: Uuid) -> Result<Subscription> {
        self.db.get_user_subscription(user_id).await
    }

    /// Vérifier si l'utilisateur est admin
    pub async fn is_user_admin(&self, user_id: Uuid) -> Result<bool> {
        let user = self.db.get_user_by_id(user_id).await?;
        Ok(user.email == self.admin_email)
    }

    /// Créer une clé API
    pub async fn create_api_key(&self, user_id: Uuid, name: &str, permissions: &[String]) -> Result<String> {
        let api_key = password::generate_api_key();
        
        self.db.create_api_key(
            user_id,
            &api_key,
            name,
            permissions,
        ).await?;

        Ok(api_key)
    }

    /// Vérifier une clé API
    pub async fn verify_api_key(&self, api_key: &str) -> Result<(Uuid, Vec<String>)> {
        self.db.get_api_key_permissions(api_key).await
    }

    /// Initialiser la réinitialisation de mot de passe
    pub async fn initiate_password_reset(&self, email: &str) -> Result<String> {
        let user = self.db.get_user_by_email(email).await?;
        
        // Générer un token de réinitialisation
        let reset_token = password::generate_reset_token();
        
        // Sauvegarder dans le cache (expire dans 24h)
        let key = format!("password_reset:{}", reset_token);
        self.cache.set_ex(
            &key,
            &user.id.to_string(),
            24 * 60 * 60, // 24 heures
        ).await?;

        // Retourner le token (sera envoyé par email)
        Ok(reset_token)
    }

    /// Réinitialiser le mot de passe avec un token
    pub async fn reset_password(&self, token: &str, new_password: &str) -> Result<()> {
        let key = format!("password_reset:{}", token);
        
        // Récupérer l'user ID depuis le cache
        let user_id_str = self.cache.get(&key).await?
            .ok_or(AppError::InvalidToken)?;
        
        let user_id = Uuid::parse_str(&user_id_str)
            .map_err(|_| AppError::InvalidToken)?;
        
        // Mettre à jour le mot de passe
        let password_hash = User::hash_password(new_password);
        self.db.update_user_password(user_id, &password_hash).await?;
        
        // Supprimer le token du cache
        self.cache.delete(&key).await?;
        
        Ok(())
    }

    /// Changer le mot de passe (avec vérification)
    pub async fn change_password(
        &self,
        user_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<()> {
        let user = self.db.get_user_by_id(user_id).await?;
        
        if !user.verify_password(current_password) {
            return Err(AppError::Unauthorized);
        }
        
        let password_hash = User::hash_password(new_password);
        self.db.update_user_password(user_id, &password_hash).await?;
        
        Ok(())
    }

    /// Supprimer un compte utilisateur
    pub async fn delete_user_account(&self, user_id: Uuid, password: &str) -> Result<()> {
        let user = self.db.get_user_by_id(user_id).await?;
        
        if !user.verify_password(password) {
            return Err(AppError::Unauthorized);
        }
        
        // Marquer l'utilisateur comme supprimé (soft delete)
        self.db.soft_delete_user(user_id).await?;
        
        Ok(())
    }
}