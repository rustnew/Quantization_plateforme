

use actix_web::{post, get, web, HttpResponse, Responder, HttpRequest};
use actix_web::http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use validator::Validate;
use uuid::Uuid;
use chrono::{Duration, Utc};

use crate::{
    domain::user::{User, NewUser, UserLogin},
    infrastructure::database::{Database, UserRepository},
    infrastructure::error::AppResult,
    core::auth::{AuthService, JwtClaims, TokenType},
    infrastructure::jwt::create_jwt_token,
};

/// Requête pour la connexion
#[derive(Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    pub password: String,
}

/// Requête pour l'inscription
#[derive(Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 2, message = "Le nom doit contenir au moins 2 caractères"))]
    pub name: String,
    #[validate(email(message = "Format d'email invalide"))]
    pub email: String,
    #[validate(length(min = 8, message = "Le mot de passe doit contenir au moins 8 caractères"))]
    pub password: String,
}

/// Requête pour le renouvellement de token
#[derive(Deserialize, Validate)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// Réponse d'authentification réussie
#[derive(Serialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Réponse simplifiée pour l'utilisateur (exclut les données sensibles)
#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            name: user.name,
            email: user.email,
            is_active: user.is_active,
            created_at: user.created_at,
        }
    }
}

/// Endpoint de connexion (email/mot de passe)
#[post("/auth/login")]
pub async fn login(
    credentials: web::Json<LoginRequest>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    // Validation des inputs
    credentials.validate()?;
    
    let user_repo = UserRepository::new(db.pool.clone());
    let auth_service = AuthService::new(user_repo);
    
    // Authentification de l'utilisateur
    let user = auth_service.authenticate(&credentials.email, &credentials.password).await?;
    
    // Création des tokens JWT
    let access_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Access,
        Duration::hours(2),
    )?;
    
    let refresh_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Refresh,
        Duration::days(30),
    )?;
    
    // Création de la réponse
    let response = AuthResponse {
        user: user.into(),
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 7200, // 2 heures en secondes
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// Endpoint d'inscription
#[post("/auth/register")]
pub async fn register(
    new_user: web::Json<RegisterRequest>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    // Validation des inputs
    new_user.validate()?;
    
    let user_repo = UserRepository::new(db.pool.clone());
    
    // Création du nouvel utilisateur
    let user_data = NewUser {
        name: new_user.name.clone(),
        email: new_user.email.clone(),
        password: Some(new_user.password.clone()),
    };
    
    let user = user_repo.create(&user_data).await?;
    
    // Création automatique de l'abonnement gratuit
    let subs_repo = crate::infrastructure::database::SubscriptionsRepository::new(db.pool.clone());
    subs_repo.create_free_subscription(&user.id).await?;
    
    // Création des tokens JWT
    let access_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Access,
        Duration::hours(2),
    )?;
    
    let refresh_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Refresh,
        Duration::days(30),
    )?;
    
    // Création de la réponse
    let response = AuthResponse {
        user: user.into(),
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 7200,
    };
    
    Ok(HttpResponse::Created().json(response))
}

/// Endpoint de callback pour l'authentification Google OAuth2
#[get("/auth/google/callback")]
pub async fn google_callback(
    query: web::Query<GoogleCallbackParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    if let Some(error) = &query.error {
        return Err(crate::infrastructure::error::AppError::Unauthorized(
            format!("Google auth error: {}", error)
        ));
    }

    if query.code.is_empty() {
        return Err(crate::infrastructure::error::AppError::BadRequest(
            "Missing authorization code".to_string()
        ));
    }

    // Ici, tu implémenterais l'appel à l'API Google pour échanger le code contre un token
    // Pour le MVP, on simule un utilisateur Google
    let user_repo = UserRepository::new(db.pool.clone());
    
    // Création ou récupération de l'utilisateur Google
    let user = user_repo.create_from_social_auth(
        "Google User".to_string(),
        format!("google_user_{}@example.com", Uuid::new_v4()),
        "google",
        query.code.clone(),
    ).await?;
    
    // Création des tokens JWT
    let access_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Access,
        Duration::hours(2),
    )?;
    
    let refresh_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Refresh,
        Duration::days(30),
    )?;
    
    let response = AuthResponse {
        user: user.into(),
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 7200,
    };
    
    // Redirection vers le frontend avec les tokens dans l'URL (pour le MVP)
    let redirect_url = format!(
        "{}/auth/callback?access_token={}&refresh_token={}",
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
        access_token,
        refresh_token
    );
    
    Ok(HttpResponse::Found()
        .append_header(("Location", redirect_url))
        .finish())
}

/// Endpoint de renouvellement de token
#[post("/auth/refresh")]
pub async fn refresh_token(
    request: web::Json<RefreshRequest>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user_repo = UserRepository::new(db.pool.clone());
    let auth_service = AuthService::new(user_repo);
    
    // Vérification du refresh token
    let user_id = auth_service.verify_refresh_token(&request.refresh_token).await?;
    
    // Récupération de l'utilisateur
    let user = auth_service.get_user_by_id(&user_id).await?;
    
    // Création d'un nouveau access token
    let new_access_token = create_jwt_token(
        &user.id.to_string(),
        TokenType::Access,
        Duration::hours(2),
    )?;
    
    let response = serde_json::json!({
        "access_token": new_access_token,
        "token_type": "Bearer",
        "expires_in": 7200
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// Endpoint de déconnexion
#[post("/auth/logout")]
pub async fn logout(
    req: HttpRequest,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    // Dans un système réel, tu stockerais les tokens révoqués dans une blacklist
    // Pour le MVP, on retourne simplement une réponse de succès
    Ok(HttpResponse::Ok().json({
        serde_json::json!({
            "message": "Successfully logged out",
            "success": true
        })
    }))
}

/// Paramètres Google OAuth2 callback
#[derive(Deserialize)]
pub struct GoogleCallbackParams {
    pub code: String,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}
