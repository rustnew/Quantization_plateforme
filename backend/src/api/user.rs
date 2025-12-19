// api/user.rs
use crate::models::{UserProfile, AuthToken};
use crate::api::AuthenticatedUser;
use crate::core::user_service::UserService;
use actix_web::{web, HttpResponse, Responder};

/// Configure les routes utilisateur
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/user")
            // Nécessite authentification
            .wrap(crate::api::auth_middleware::require_auth())
            // Profil
            .route("/profile", web::get().to(get_profile))
            .route("/profile", web::put().to(update_profile))
            // Clés API
            .route("/api-keys", web::get().to(list_api_keys))
            .route("/api-keys", web::post().to(create_api_key))
            .route("/api-keys/{key_id}", web::delete().to(delete_api_key))
            // Paramètres
            .route("/settings", web::get().to(get_settings))
            .route("/settings", web::put().to(update_settings))
            // Changer mot de passe
            .route("/change-password", web::post().to(change_password))
            // Supprimer compte
            .route("/delete-account", web::post().to(delete_account)),
    );
}

/// Obtenir le profil utilisateur
async fn get_profile(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
) -> impl Responder {
    match user_service.get_user_profile(user.id).await {
        Ok(profile) => HttpResponse::Ok().json(profile),
        Err(e) => {
            match e {
                crate::utils::error::AppError::UserNotFound => {
                    HttpResponse::NotFound().json("Utilisateur non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Mettre à jour le profil utilisateur
async fn update_profile(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    update_data: web::Json<UpdateProfileRequest>,
) -> impl Responder {
    match user_service.update_user_profile(user.id, &update_data.name).await {
        Ok(profile) => HttpResponse::Ok().json(profile),
        Err(e) => {
            match e {
                crate::utils::error::AppError::UserNotFound => {
                    HttpResponse::NotFound().json("Utilisateur non trouvé")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Lister les clés API
async fn list_api_keys(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
) -> impl Responder {
    match user_service.get_user_api_keys(user.id).await {
        Ok(api_keys) => HttpResponse::Ok().json(api_keys),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Créer une nouvelle clé API
async fn create_api_key(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    request: web::Json<CreateApiKeyRequest>,
) -> impl Responder {
    match user_service.create_api_key(user.id, &request.name, &request.permissions).await {
        Ok(api_key) => HttpResponse::Created().json(api_key),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Supprimer une clé API
async fn delete_api_key(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    key_id: web::Path<uuid::Uuid>,
) -> impl Responder {
    match user_service.delete_api_key(user.id, *key_id).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            match e {
                crate::utils::error::AppError::NotFound => {
                    HttpResponse::NotFound().json("Clé API non trouvée")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Obtenir les paramètres utilisateur
async fn get_settings(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
) -> impl Responder {
    match user_service.get_user_settings(user.id).await {
        Ok(settings) => HttpResponse::Ok().json(settings),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Mettre à jour les paramètres utilisateur
async fn update_settings(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    settings: web::Json<UserSettings>,
) -> impl Responder {
    match user_service.update_user_settings(user.id, settings.into_inner()).await {
        Ok(updated_settings) => HttpResponse::Ok().json(updated_settings),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Changer le mot de passe
async fn change_password(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    request: web::Json<ChangePasswordRequest>,
) -> impl Responder {
    match user_service.change_password(user.id, &request.current_password, &request.new_password).await {
        Ok(_) => HttpResponse::Ok().json("Mot de passe changé avec succès"),
        Err(e) => {
            match e {
                crate::utils::error::AppError::Unauthorized => {
                    HttpResponse::Unauthorized().json("Mot de passe actuel incorrect")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Supprimer le compte utilisateur
async fn delete_account(
    user: AuthenticatedUser,
    user_service: web::Data<UserService>,
    request: web::Json<DeleteAccountRequest>,
) -> impl Responder {
    match user_service.delete_user_account(user.id, &request.password).await {
        Ok(_) => HttpResponse::Ok().json("Compte supprimé avec succès"),
        Err(e) => {
            match e {
                crate::utils::error::AppError::Unauthorized => {
                    HttpResponse::Unauthorized().json("Mot de passe incorrect")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

// Structures de requête
#[derive(Debug, serde::Deserialize)]
struct UpdateProfileRequest {
    name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    permissions: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct UserSettings {
    email_notifications: bool,
    job_completion_notifications: bool,
    billing_notifications: bool,
    default_quantization_method: Option<String>,
    default_output_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Debug, serde::Deserialize)]
struct DeleteAccountRequest {
    password: String,
}