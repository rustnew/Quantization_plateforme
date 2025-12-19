// api/auth.rs
use crate::models::{User, NewUser, UserLogin, GoogleAuth, AuthToken};
use crate::core::user_service::UserService;
use crate::services::external::google_auth_client::GoogleAuthClient;
use actix_web::{web, HttpResponse, Responder};
use validator::Validate;

/// Configure les routes d'authentification
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            // Inscription
            .route("/register", web::post().to(register))
            // Connexion email/mot de passe
            .route("/login", web::post().to(login))
            // Connexion Google
            .route("/google", web::post().to(google_login))
            // Rafraîchir token
            .route("/refresh", web::post().to(refresh_token))
            // Déconnexion
            .route("/logout", web::post().to(logout))
            // Mot de passe oublié
            .route("/forgot-password", web::post().to(forgot_password))
            // Réinitialiser mot de passe
            .route("/reset-password", web::post().to(reset_password)),
    );
}

/// Inscription d'un nouvel utilisateur
async fn register(
    user_service: web::Data<UserService>,
    new_user: web::Json<NewUser>,
) -> impl Responder {
    // Validation
    if let Err(errors) = new_user.validate() {
        return HttpResponse::BadRequest().json(errors);
    }
    
    match user_service.register_user(&new_user.email, &new_user.password).await {
        Ok(user) => {
            // Générer le token JWT
            let token = user_service.generate_auth_token(&user).await;
            HttpResponse::Created().json(token)
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::UserAlreadyExists => {
                    HttpResponse::Conflict().json("Un utilisateur avec cet email existe déjà")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Connexion avec email/mot de passe
async fn login(
    user_service: web::Data<UserService>,
    credentials: web::Json<UserLogin>,
) -> impl Responder {
    // Validation
    if let Err(errors) = credentials.validate() {
        return HttpResponse::BadRequest().json(errors);
    }
    
    match user_service.authenticate_user(&credentials.email, &credentials.password).await {
        Ok(user) => {
            // Mettre à jour la dernière connexion
            user_service.update_last_login(user.id).await.ok();
            
            // Générer le token JWT
            let token = user_service.generate_auth_token(&user).await;
            HttpResponse::Ok().json(token)
        }
        Err(e) => {
            match e {
                crate::utils::error::AppError::Unauthorized => {
                    HttpResponse::Unauthorized().json("Email ou mot de passe incorrect")
                }
                crate::utils::error::AppError::UserNotFound => {
                    HttpResponse::NotFound().json("Utilisateur non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Connexion avec Google OAuth
async fn google_login(
    user_service: web::Data<UserService>,
    google_client: web::Data<GoogleAuthClient>,
    auth_data: web::Json<GoogleAuth>,
) -> impl Responder {
    // Vérifier le token Google
    match google_client.verify_token(&auth_data.google_token).await {
        Ok(google_user) => {
            // Récupérer ou créer l'utilisateur
            match user_service.get_or_create_google_user(&google_user.email, &google_user.name).await {
                Ok(user) => {
                    // Mettre à jour la dernière connexion
                    user_service.update_last_login(user.id).await.ok();
                    
                    // Générer le token JWT
                    let token = user_service.generate_auth_token(&user).await;
                    HttpResponse::Ok().json(token)
                }
                Err(e) => {
                    HttpResponse::InternalServerError().json(format!("Erreur: {}", e))
                }
            }
        }
        Err(e) => {
            HttpResponse::Unauthorized().json(format("Token Google invalide: {}", e))
        }
    }
}

/// Rafraîchir le token JWT
async fn refresh_token(
    user_service: web::Data<UserService>,
    refresh_token: web::Json<RefreshTokenRequest>,
) -> impl Responder {
    match user_service.refresh_auth_token(&refresh_token.refresh_token).await {
        Ok(new_token) => HttpResponse::Ok().json(new_token),
        Err(e) => {
            match e {
                crate::utils::error::AppError::Unauthorized => {
                    HttpResponse::Unauthorized().json("Token de rafraîchissement invalide")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Déconnexion
async fn logout() -> impl Responder {
    // Pour une déconnexion côté client, on peut simplement retourner un succès
    // La véritable invalidation se fait côté client en supprimant les tokens
    HttpResponse::Ok().json("Déconnexion réussie")
}

/// Mot de passe oublié
async fn forgot_password(
    user_service: web::Data<UserService>,
    request: web::Json<ForgotPasswordRequest>,
) -> impl Responder {
    match user_service.initiate_password_reset(&request.email).await {
        Ok(_) => HttpResponse::Ok().json("Email de réinitialisation envoyé"),
        Err(e) => {
            // Ne pas révéler si l'email existe ou non (sécurité)
            HttpResponse::Ok().json("Si l'email existe, un lien de réinitialisation a été envoyé")
        }
    }
}

/// Réinitialiser le mot de passe
async fn reset_password(
    user_service: web::Data<UserService>,
    request: web::Json<ResetPasswordRequest>,
) -> impl Responder {
    match user_service.reset_password(&request.token, &request.new_password).await {
        Ok(_) => HttpResponse::Ok().json("Mot de passe réinitialisé avec succès"),
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidToken => {
                    HttpResponse::BadRequest().json("Token invalide ou expiré")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

// Structures de requête spécifiques
#[derive(Debug, serde::Deserialize)]
struct RefreshTokenRequest {
    refresh_token: String,
}

#[derive(Debug, serde::Deserialize)]
struct ForgotPasswordRequest {
    email: String,
}

#[derive(Debug, serde::Deserialize)]
struct ResetPasswordRequest {
    token: String,
    new_password: String,
}