

use std::fmt;
use std::error::Error as StdError;
use actix_web::{error::ResponseError, HttpResponse, http::StatusCode};
use sqlx::Error as SqlxError;
use validator::ValidationErrors;
use serde::{Serialize, Deserialize};

/// Type de résultat standard pour l'application
pub type AppResult<T> = Result<T, AppError>;

/// Erreurs principales de l'application
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum AppError {
    /// Erreur d'authentification (401 Unauthorized)
    #[error("Authentication failed: {0}")]
    Unauthorized(String),
    
    /// Permissions insuffisantes (403 Forbidden)
    #[error("Access forbidden: {0}")]
    Forbidden(String),
    
    /// Ressource non trouvée (404 Not Found)
    #[error("{0} not found")]
    NotFound(String),
    
    /// Conflit de ressources (409 Conflict)
    #[error("Conflict: {0}")]
    Conflict(String),
    
    /// Données invalides (422 Unprocessable Entity)
    #[error("Validation failed: {0}")]
    ValidationError(ValidationErrors),
    
    /// Requête mal formée (400 Bad Request)
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    /// Erreur interne du serveur (500 Internal Server Error)
    #[error("Internal server error: {0}")]
    InternalError(String),
    
    /// Erreur de base de données (500 Internal Server Error)
    #[error("Database error: {0}")]
    DatabaseError(SqlxError),
    
    /// Erreur de sérialisation/désérialisation (500 Internal Server Error)
    #[error("Serialization error: {0}")]
    SerializationError(serde_json::Error),
    
    /// Erreur d'infrastructure (stockage, file d'attente, etc.) (500 Internal Server Error)
    #[error("Infrastructure error: {0}")]
    InfrastructureError(String),
    
    /// Erreur de configuration (500 Internal Server Error)
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    /// Timeout d'opération (504 Gateway Timeout)
    #[error("Operation timeout: {0}")]
    Timeout(String),
    
    /// Erreur de connexion (502 Bad Gateway)
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Erreur Python (500 Internal Server Error)
    #[error("Python error: {0}")]
    PythonError(String),
    
    /// Type de média non supporté (415 Unsupported Media Type)
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),
    
    /// Payload trop lourd (413 Payload Too Large)
    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),
    
    /// Resource épuisée (429 Too Many Requests)
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}

impl AppError {
    /// Convertit l'erreur en code HTTP approprié
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::ValidationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InfrastructureError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConfigurationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            AppError::ConnectionError(_) => StatusCode::BAD_GATEWAY,
            AppError::PythonError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AppError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::ResourceExhausted(_) => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    /// Convertit l'erreur en message utilisateur-friendly
    /// (à utiliser pour les réponses clients, pas pour le logging)
    pub fn user_friendly_message(&self) -> String {
        match self {
            AppError::Unauthorized(_) => "Authentification échouée. Veuillez vérifier vos identifiants.".to_string(),
            AppError::Forbidden(_) => "Vous n'avez pas les permissions nécessaires pour cette action.".to_string(),
            AppError::NotFound(resource) => format!("{} non trouvé", resource),
            AppError::Conflict(_) => "Conflit: cette ressource existe déjà ou est en cours d'utilisation.".to_string(),
            AppError::ValidationError(errors) => {
                let mut messages = Vec::new();
                for (_, field_errors) in errors.errors() {
                    for error in field_errors {
                        if let Some(msg) = error.message.as_ref() {
                            messages.push(msg.to_string());
                        }
                    }
                }
                if messages.is_empty() {
                    "Données invalides. Veuillez vérifier le format des champs.".to_string()
                } else {
                    messages.join("; ")
                }
            },
            AppError::BadRequest(_) => "Requête incorrecte. Veuillez vérifier les paramètres.".to_string(),
            AppError::Timeout(_) => "L'opération a pris trop de temps. Veuillez réessayer plus tard.".to_string(),
            AppError::ResourceExhausted(_) => "Trop de requêtes. Veuillez réessayer dans quelques minutes.".to_string(),
            AppError::UnsupportedMediaType(_) => "Type de fichier non supporté. Veuillez utiliser un format valide.".to_string(),
            AppError::PayloadTooLarge(_) => "Fichier trop volumineux. Veuillez réduire la taille.".to_string(),
            AppError::InternalError(_) |
            AppError::DatabaseError(_) |
            AppError::SerializationError(_) |
            AppError::InfrastructureError(_) |
            AppError::ConfigurationError(_) |
            AppError::ConnectionError(_) |
            AppError::PythonError(_) => {
                "Une erreur interne est survenue. Notre équipe technique a été notifiée.".to_string()
            }
        }
    }

    /// Log l'erreur avec un contexte supplémentaire
    pub fn log_with_context(&self, context: &str) -> String {
        match self {
            AppError::DatabaseError(sqlx_error) => {
                format!("Database error [{}]: {}", context, sqlx_error)
            },
            AppError::ValidationError(errors) => {
                let error_details: Vec<String> = errors.field_errors()
                    .iter()
                    .flat_map(|(field, errs)| {
                        errs.iter().map(|err| {
                            format!("field '{}' - {}", field, err.message.as_ref().unwrap_or(&"unknown error".to_string()))
                        })
                    })
                    .collect();
                format!("Validation error [{}]: {}", context, error_details.join(", "))
            },
            _ => format!("{} [{}]: {}", self, context, self.source().map(|s| s.to_string()).unwrap_or_default())
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        let error_response = ErrorResponse {
            error: self.user_friendly_message(),
            code: self.status_code().as_u16(),
        };
        
        HttpResponse::build(self.status_code()).json(error_response)
    }
}

/// Structure de réponse d'erreur standardisée
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
}

// Implémentations From pour les conversions automatiques

impl From<SqlxError> for AppError {
    fn from(error: SqlxError) -> Self {
        // Spécialiser certains types d'erreurs SQL
        match &error {
            SqlxError::RowNotFound => AppError::NotFound("Resource".to_string()),
            SqlxError::Database(db_error) => {
                if db_error.code().map(|code| code == "23505").unwrap_or(false) {
                    AppError::Conflict("Unique constraint violation".to_string())
                } else {
                    AppError::DatabaseError(error)
                }
            },
            _ => AppError::DatabaseError(error),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::SerializationError(error)
    }
}

impl From<ValidationErrors> for AppError {
    fn from(errors: ValidationErrors) -> Self {
        AppError::ValidationError(errors)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::InfrastructureError(format!("IO error: {}", error))
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        AppError::InternalError(error.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(error: tokio::task::JoinError) -> Self {
        AppError::InternalError(format!("Task join error: {}", error))
    }
}

impl From<config::ConfigError> for AppError {
    fn from(error: config::ConfigError) -> Self {
        AppError::ConfigurationError(error.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            AppError::Timeout("Request timeout".to_string())
        } else if error.is_connect() {
            AppError::ConnectionError("Connection failed".to_string())
        } else {
            AppError::InfrastructureError(format!("HTTP request error: {}", error))
        }
    }
}

// Implémentation des traits standard
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl StdError for AppError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            AppError::DatabaseError(e) => Some(e),
            AppError::SerializationError(e) => Some(e),
            _ => None,
        }
    }
}

// Helper functions pour créer des erreurs courantes
pub fn not_found<T: Into<String>>(resource: T) -> AppError {
    AppError::NotFound(resource.into())
}

pub fn validation_error(errors: ValidationErrors) -> AppError {
    AppError::ValidationError(errors)
}

pub fn database_error<T: Into<String>>(message: T) -> AppError {
    AppError::InfrastructureError(format!("Database error: {}", message.into()))
}

pub fn internal_error<T: Into<String>>(message: T) -> AppError {
    AppError::InternalError(message.into())
}

pub fn unauthorized<T: Into<String>>(message: T) -> AppError {
    AppError::Unauthorized(message.into())
}

pub fn forbidden<T: Into<String>>(message: T) -> AppError {
    AppError::Forbidden(message.into())
}

pub fn conflict<T: Into<String>>(message: T) -> AppError {
    AppError::Conflict(message.into())
}
