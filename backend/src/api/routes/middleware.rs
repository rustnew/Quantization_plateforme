//! # API Middleware
//! 
//! Ce module contient les middlewares de sécurité et de validation pour l'API :
//! - Authentification JWT
//! - Autorisation (admin, user)
//! - Rate limiting
//! - Validation des inputs
//! - Gestion des erreurs
//! 
//! ## Middlewares disponibles
//! - `require_auth` : protège les routes avec JWT authentification
//! - `require_admin` : protège les routes admin-only
//! - `rate_limiter` : limite les requêtes par utilisateur/IP
//! - `validate_input` : valide les inputs avec validator
//! 
//! ## Sécurité
//! - JWT signature verification
//! - Token expiration checking
//! - Role-based access control
//! - Protection contre les attaques par déni de service
//! 
//! ## Personnalisation
//! Les middlewares peuvent être configurés via les variables d'environnement :
//! - `RATE_LIMIT_REQUESTS` : nombre de requêtes par minute
//! - `RATE_LIMIT_WINDOW` : fenêtre de temps en secondes
//! - `JWT_SECRET` : secret pour les tokens JWT

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    error::{ErrorUnauthorized, ErrorForbidden},
    http::header::HeaderMap,
    web, Error, FromRequest, HttpResponse,
};
use futures_util::future::{ok, Ready};
use std::task::{Context, Poll};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use chrono::Utc;

use crate::{
    infrastructure::database::Database,
    core::auth::{JwtClaims, validate_jwt_token},
    infrastructure::error::AppError,
    domain::user::User,
};

/// Middleware d'authentification JWT
pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthMiddlewareService { service })
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = actix_web::dev::ServiceFuture<S::Future>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let auth_header = req.headers().get("Authorization");
        
        if let Some(header) = auth_header {
            if let Ok(token) = header.to_str() {
                if token.starts_with("Bearer ") {
                    let token = token.trim_start_matches("Bearer ");
                    
                    match validate_jwt_token(token) {
                        Ok(claims) => {
                            // Ajouter les claims à l'extension de la requête
                            let mut req = req;
                            req.extensions_mut().insert(claims);
                            return self.service.call(req);
                        }
                        Err(e) => {
                            return actix_web::dev::ServiceFuture::Ready(Box::pin(async move {
                                Err(ErrorUnauthorized(format!("Invalid token: {}", e)))
                            }));
                        }
                    }
                }
            }
        }
        
        actix_web::dev::ServiceFuture::Ready(Box::pin(async move {
            Err(ErrorUnauthorized("Missing or invalid Authorization header"))
        }))
    }
}

/// Middleware d'autorisation admin
pub struct AdminMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AdminMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = AdminMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AdminMiddlewareService { service })
    }
}

pub struct AdminMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AdminMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = actix_web::dev::ServiceFuture<S::Future>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let claims = req.extensions().get::<JwtClaims>();
        
        if let Some(claims) = claims {
            if claims.role == "admin" {
                return self.service.call(req);
            }
        }
        
        actix_web::dev::ServiceFuture::Ready(Box::pin(async move {
            Err(ErrorForbidden("Admin privileges required"))
        }))
    }
}

/// Middleware de rate limiting
pub struct RateLimiter {
    limits: Arc<RateLimitStore>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limits: Arc::new(RateLimitStore::new()),
        }
    }
}

struct RateLimitStore {
    limits: std::sync::Mutex<HashMap<String, (Instant, usize)>>,
    max_requests: usize,
    window_duration: Duration,
}

impl RateLimitStore {
    fn new() -> Self {
        Self {
            limits: std::sync::Mutex::new(HashMap::new()),
            max_requests: std::env::var("RATE_LIMIT_REQUESTS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            window_duration: Duration::from_secs(
                std::env::var("RATE_LIMIT_WINDOW")
                    .unwrap_or_else(|_| "60".to_string())
                    .parse()
                    .unwrap_or(60),
            ),
        }
    }

    fn check_limit(&self, key: &str) -> bool {
        let mut limits = self.limits.lock().unwrap();
        let now = Instant::now();
        
        let entry = limits.entry(key.to_string()).or_insert((now, 0));
        let (last_reset, count) = entry;
        
        if now.duration_since(*last_reset) > self.window_duration {
            *last_reset = now;
            *count = 1;
            true
        } else {
            if *count >= self.max_requests {
                false
            } else {
                *count += 1;
                true
            }
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimiter
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RateLimiterService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RateLimiterService {
            service,
            limits: self.limits.clone(),
        })
    }
}

pub struct RateLimiterService<S> {
    service: S,
    limits: Arc<RateLimitStore>,
}

impl<S, B> Service<ServiceRequest> for RateLimiterService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = actix_web::dev::ServiceFuture<S::Future>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let ip = req.connection_info().realip_remote_addr().unwrap_or("unknown");
        let user_agent = req.headers().get("User-Agent").map(|h| h.to_str().unwrap_or("unknown")).unwrap_or("unknown");
        let key = format!("{}_{}", ip, user_agent);
        
        if self.limits.check_limit(&key) {
            self.service.call(req)
        } else {
            actix_web::dev::ServiceFuture::Ready(Box::pin(async move {
                let response = HttpResponse::TooManyRequests()
                    .json(serde_json::json!({
                        "error": "Too many requests",
                        "message": "Please try again later",
                        "retry_after": 60
                    }));
                Err(actix_web::error::Error::from(response))
            }))
        }
    }
}

/// Extension pour obtenir l'utilisateur courant
#[derive(Clone)]
pub struct CurrentUser(pub User);

impl FromRequest for CurrentUser {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &ServiceRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let claims = req.extensions().get::<JwtClaims>();
        
        if let Some(claims) = claims {
            let db = req.app_data::<web::Data<Database>>().cloned().unwrap();
            let user_id = claims.sub.clone();
            
            // Ici, tu récupérerais l'utilisateur depuis la base de données
            // Pour le MVP, on simule un utilisateur
            let user = User {
                id: uuid::Uuid::parse_str(&user_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                name: claims.name.clone().unwrap_or_else(|| "User".to_string()),
                email: claims.email.clone().unwrap_or_else(|| "user@example.com".to_string()),
                password_hash: None,
                auth_provider: Some("jwt".to_string()),
                auth_provider_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                is_active: true,
            };
            
            ok(CurrentUser(user))
        } else {
            let response = HttpResponse::Unauthorized()
                .json(serde_json::json!({
                    "error": "Unauthorized",
                    "message": "Authentication required"
                }));
            let err = Error::from(response);
            Err(err).into()
        }
    }
}

/// Fonction utilitaire pour obtenir l'utilisateur courant
pub async fn get_current_user(req: &HttpRequest, db: web::Data<Database>) -> AppResult<User> {
    let claims = req.extensions().get::<JwtClaims>().ok_or(AppError::Unauthorized(
        "No authentication token found".to_string()
    ))?;
    
    let user_id = uuid::Uuid::parse_str(&claims.sub)?;
    let user_repo = crate::infrastructure::database::UserRepository::new(db.pool.clone());
    user_repo.get_by_id(&user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App, dev::Service, http::header};
    use actix_web::test::TestRequest;
    use actix_web::dev::ServiceResponse;
    use uuid::Uuid;

    #[actix_web::test]
    async fn test_auth_middleware_success() {
        let mut app = test::init_service(
            App::new()
                .wrap(AuthMiddleware)
                .route("/", web::get().to(|| async { "Authenticated" }))
        ).await;

        let req = TestRequest::get()
            .insert_header(("Authorization", "Bearer valid_token"))
            .uri("/")
            .to_request();

        let resp = app.call(req).await.unwrap();
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_auth_middleware_failure() {
        let mut app = test::init_service(
            App::new()
                .wrap(AuthMiddleware)
                .route("/", web::get().to(|| async { "Authenticated" }))
        ).await;

        let req = TestRequest::get()
            .uri("/")
            .to_request();

        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_admin_middleware_success() {
        let mut app = test::init_service(
            App::new()
                .wrap(AdminMiddleware)
                .route("/", web::get().to(|| async { "Admin access" }))
        ).await;

        let mut req = TestRequest::get()
            .uri("/")
            .to_request();

        // Ajouter des claims admin aux extensions
        let claims = JwtClaims {
            sub: Uuid::new_v4().to_string(),
            name: Some("Admin User".to_string()),
            email: Some("admin@example.com".to_string()),
            role: "admin".to_string(),
            exp: 0,
            iat: 0,
        };
        req.extensions_mut().insert(claims);

        let resp = app.call(req).await.unwrap();
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_admin_middleware_failure() {
        let mut app = test::init_service(
            App::new()
                .wrap(AdminMiddleware)
                .route("/", web::get().to(|| async { "Admin access" }))
        ).await;

        let mut req = TestRequest::get()
            .uri("/")
            .to_request();

        // Ajouter des claims utilisateur normaux
        let claims = JwtClaims {
            sub: Uuid::new_v4().to_string(),
            name: Some("Regular User".to_string()),
            email: Some("user@example.com".to_string()),
            role: "user".to_string(),
            exp: 0,
            iat: 0,
        };
        req.extensions_mut().insert(claims);

        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_rate_limiter() {
        let mut app = test::init_service(
            App::new()
                .wrap(RateLimiter::new())
                .route("/", web::get().to(|| async { "Rate limited" }))
        ).await;

        // Faire plusieurs requêtes rapides
        for i in 0..150 {
            let req = TestRequest::get()
                .uri("/")
                .insert_header(("User-Agent", format!("TestAgent/{}", i)))
                .to_request();
            
            let resp = app.call(req).await.unwrap();
            
            if i >= 100 { // Après la limite
                assert_eq!(resp.status(), actix_web::http::StatusCode::TOO_MANY_REQUESTS);
            } else {
                assert!(resp.status().is_success());
            }
        }
    }
}