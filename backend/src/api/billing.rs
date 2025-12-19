// api/billing.rs
use crate::models::{Subscription, PlanInfo, CreditInfo, CreditTransaction, PaginatedResponse};
use crate::api::AuthenticatedUser;
use crate::core::billing_service::BillingService;
use actix_web::{web, HttpResponse, Responder};

/// Configure les routes de facturation
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/billing")
            .wrap(crate::api::auth_middleware::require_auth())
            // Informations sur les plans
            .route("/plans", web::get().to(list_plans))
            // Abonnement actuel
            .route("/subscription", web::get().to(get_subscription))
            .route("/subscription", web::post().to(update_subscription))
            .route("/subscription/cancel", web::post().to(cancel_subscription))
            // Crédits
            .route("/credits", web::get().to(get_credit_info))
            .route("/credits/history", web::get().to(get_credit_history))
            // Paiement
            .route("/checkout", web::post().to(create_checkout_session))
            .route("/portal", web::post().to(create_customer_portal))
            // Webhook Stripe (pas d'authentification requise)
            .route("/webhook/stripe", web::post().to(stripe_webhook)),
    );
}

/// Lister tous les plans disponibles
async fn list_plans(
    billing_service: web::Data<BillingService>,
) -> impl Responder {
    let plans = vec![
        crate::models::SubscriptionPlan::Free.info(),
        crate::models::SubscriptionPlan::Starter.info(),
        crate::models::SubscriptionPlan::Pro.info(),
    ];
    
    HttpResponse::Ok().json(plans)
}

/// Obtenir l'abonnement actuel
async fn get_subscription(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
) -> impl Responder {
    match billing_service.get_user_subscription(user.id).await {
        Ok(subscription) => HttpResponse::Ok().json(subscription),
        Err(e) => {
            match e {
                crate::utils::error::AppError::NotFound => {
                    // Créer un abonnement gratuit par défaut
                    match billing_service.create_free_subscription(user.id).await {
                        Ok(subscription) => HttpResponse::Ok().json(subscription),
                        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
                    }
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Mettre à jour l'abonnement
async fn update_subscription(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
    request: web::Json<UpdateSubscriptionRequest>,
) -> impl Responder {
    match billing_service.update_subscription(
        user.id,
        &request.plan,
        &request.payment_method_id,
    ).await {
        Ok(subscription) => HttpResponse::Ok().json(subscription),
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidPlan => {
                    HttpResponse::BadRequest().json("Plan invalide")
                }
                crate::utils::error::AppError::PaymentFailed => {
                    HttpResponse::PaymentRequired().json("Échec du paiement")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Annuler l'abonnement
async fn cancel_subscription(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
) -> impl Responder {
    match billing_service.cancel_subscription(user.id).await {
        Ok(_) => HttpResponse::Ok().json("Abonnement annulé avec succès"),
        Err(e) => {
            match e {
                crate::utils::error::AppError::NoSubscription => {
                    HttpResponse::NotFound().json("Aucun abonnement actif")
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Obtenir les informations de crédits
async fn get_credit_info(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
) -> impl Responder {
    match billing_service.get_user_credits(user.id).await {
        Ok(credit_info) => HttpResponse::Ok().json(credit_info),
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Obtenir l'historique des crédits
async fn get_credit_history(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
    query: web::Query<CreditHistoryQuery>,
) -> impl Responder {
    match billing_service.get_credit_history(
        user.id,
        query.page.unwrap_or(1),
        query.per_page.unwrap_or(20),
    ).await {
        Ok(transactions) => {
            let total = transactions.len() as i64;
            let response = PaginatedResponse {
                items: transactions,
                total,
                page: query.page.unwrap_or(1),
                per_page: query.per_page.unwrap_or(20),
                total_pages: (total as f64 / query.per_page.unwrap_or(20) as f64).ceil() as i64,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json("Erreur serveur"),
    }
}

/// Créer une session de checkout Stripe
async fn create_checkout_session(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
    request: web::Json<CreateCheckoutRequest>,
) -> impl Responder {
    match billing_service.create_checkout_session(
        user.id,
        &request.plan,
        &request.success_url,
        &request.cancel_url,
    ).await {
        Ok(checkout_session) => HttpResponse::Ok().json(checkout_session),
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidPlan => {
                    HttpResponse::BadRequest().json("Plan invalide")
                }
                crate::utils::error::AppError::StripeError(err) => {
                    HttpResponse::InternalServerError().json(format!("Erreur Stripe: {}", err))
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Créer un portail client Stripe
async fn create_customer_portal(
    user: AuthenticatedUser,
    billing_service: web::Data<BillingService>,
) -> impl Responder {
    match billing_service.create_customer_portal(user.id).await {
        Ok(portal_url) => HttpResponse::Ok().json(portal_url),
        Err(e) => {
            match e {
                crate::utils::error::AppError::StripeError(err) => {
                    HttpResponse::InternalServerError().json(format!("Erreur Stripe: {}", err))
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

/// Webhook Stripe pour les événements de paiement
async fn stripe_webhook(
    billing_service: web::Data<BillingService>,
    req: actix_web::HttpRequest,
    payload: web::Bytes,
) -> impl Responder {
    // Extraire la signature Stripe
    let signature = match req.headers().get("Stripe-Signature") {
        Some(sig) => sig.to_str().unwrap_or(""),
        None => return HttpResponse::BadRequest().json("Signature manquante"),
    };
    
    // Traiter le webhook
    match billing_service.handle_stripe_webhook(&payload, signature).await {
        Ok(_) => HttpResponse::Ok().json("Webhook traité"),
        Err(e) => {
            match e {
                crate::utils::error::AppError::InvalidSignature => {
                    HttpResponse::BadRequest().json("Signature invalide")
                }
                crate::utils::error::AppError::StripeError(err) => {
                    HttpResponse::InternalServerError().json(format!("Erreur Stripe: {}", err))
                }
                _ => HttpResponse::InternalServerError().json("Erreur serveur"),
            }
        }
    }
}

// Structures de requête
#[derive(Debug, serde::Deserialize)]
struct UpdateSubscriptionRequest {
    plan: String,
    payment_method_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CreditHistoryQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct CreateCheckoutRequest {
    plan: String,
    success_url: String,
    cancel_url: String,
}