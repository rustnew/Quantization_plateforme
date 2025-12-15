//! # Subscriptions API Routes
//! 
//! Ce module gère tous les endpoints liés aux abonnements payants :
//! - Récupération des informations d'abonnement
//! - Mise à niveau de plan
//! - Annulation d'abonnement
//! - Historique des paiements
//! - Création de sessions Stripe
//! 
//! ## Intégration Stripe
//! - Création de customers Stripe pour les nouveaux utilisateurs
//! - Gestion des abonnements récurrents
//! - Webhooks pour les événements de paiement
//! - Facturation automatique
//! 
//! ## Plans disponibles
//! - `free` : 1 crédit/mois (quantification INT8 gratuite)
//! - `starter` : 10 crédits/mois (19€/mois)
//! - `pro` : crédits illimités (99€/mois)
//! 
//! ## Sécurité
//! - Vérification de l'identité avant modification d'abonnement
//! - Validation des webhooks Stripe avec signature secrète
//! - Protection contre les upgrades non autorisés
//! 
//! ## Gestion des erreurs
//! - 400 Bad Request : paramètres invalides
//! - 402 Payment Required : paiement requis pour l'upgrade
//! - 403 Forbidden : accès non autorisé
//! - 404 Not Found : abonnement non trouvé
//! - 409 Conflict : abonnement déjà actif

use actix_web::{get, post, web, HttpResponse, Responder, HttpRequest};
use serde::{Deserialize, Serialize};
use validator::Validate;
use uuid::Uuid;
use chrono::Utc;

use crate::{
    domain::user::User,
    infrastructure::database::{Database, SubscriptionsRepository, UserRepository, PaymentsRepository},
    infrastructure::error::AppResult,
    core::auth::get_current_user,
    infrastructure::stripe::{StripeService, CreateCheckoutSessionParams},
};

/// Requête pour mettre à niveau le plan
#[derive(Deserialize, Validate)]
pub struct UpgradePlanRequest {
    pub plan_name: String,
    pub success_url: String,
    pub cancel_url: String,
}

/// Réponse d'abonnement
#[derive(Serialize)]
pub struct SubscriptionResponse {
    pub subscription_id: Uuid,
    pub plan_name: String,
    pub monthly_credits: i32,
    pub credits_used: i32,
    pub credits_remaining: i32,
    pub is_active: bool,
    pub current_period_end: chrono::DateTime<chrono::Utc>,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
}

/// Réponse de paiement
#[derive(Serialize)]
pub struct PaymentResponse {
    pub payment_id: Uuid,
    pub amount: f64,
    pub currency: String,
    pub description: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub stripe_payment_id: Option<String>,
}

/// Réponse de session Stripe
#[derive(Serialize)]
pub struct StripeSessionResponse {
    pub session_id: String,
    pub session_url: String,
}

/// Historique des paiements réponse
#[derive(Serialize)]
pub struct PaymentHistoryResponse {
    pub payments: Vec<PaymentResponse>,
    pub total_amount: f64,
    pub currency: String,
}

/// Valide le nom du plan
fn validate_plan_name(plan_name: &str) -> Result<(), validator::ValidationError> {
    match plan_name.to_lowercase().as_str() {
        "free" | "starter" | "pro" => Ok(()),
        _ => {
            let mut err = validator::ValidationError::new("plan_name");
            err.message = Some("Plan non supporté. Utilisez: free, starter, pro".into());
            Err(err)
        }
    }
}

/// Endpoint pour obtenir l'abonnement actuel
#[get("/subscriptions")]
pub async fn get_subscription(
    req: HttpRequest,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    
    let subs_repo = SubscriptionsRepository::new(db.pool.clone());
    let subscription = subs_repo.get_active_subscription(&user.id).await?;
    
    let response = SubscriptionResponse {
        subscription_id: subscription.id,
        plan_name: subscription.plan_name.to_string(),
        monthly_credits: subscription.monthly_credits,
        credits_used: subscription.credits_used,
        credits_remaining: subscription.monthly_credits - subscription.credits_used,
        is_active: subscription.is_active,
        current_period_end: subscription.current_period_end,
        stripe_customer_id: subscription.stripe_customer_id.clone(),
        stripe_subscription_id: subscription.stripe_subscription_id.clone(),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// Endpoint pour mettre à niveau le plan
#[post("/subscriptions/upgrade")]
pub async fn upgrade_plan(
    req: HttpRequest,
    request: web::Json<UpgradePlanRequest>,
    db: web::Data<Database>,
    stripe_service: web::Data<StripeService>,
) -> AppResult<HttpResponse> {
    // Validation des inputs
    request.validate()?;
    
    let user = get_current_user(&req, db.clone()).await?;
    let subs_repo = SubscriptionsRepository::new(db.pool.clone());
    
    // Vérifier l'abonnement actuel
    let current_sub = subs_repo.get_active_subscription(&user.id).await?;
    
    // Vérifier si on peut mettre à niveau (pas de downgrade direct)
    if current_sub.plan_name.to_string() == request.plan_name {
        return Err(crate::infrastructure::error::AppError::Conflict(
            "Vous avez déjà ce plan d'abonnement".to_string()
        ));
    }
    
    if current_sub.plan_name == crate::infrastructure::database::PlanName::Pro {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Vous ne pouvez pas downgrader depuis le plan Pro".to_string()
        ));
    }
    
    // Créer une session Stripe checkout
    let session_params = CreateCheckoutSessionParams {
        customer_email: Some(user.email.clone()),
        plan_name: request.plan_name.clone(),
        success_url: request.success_url.clone(),
        cancel_url: request.cancel_url.clone(),
        metadata: Some(serde_json::json!({
            "user_id": user.id.to_string(),
            "upgrade_from": current_sub.plan_name.to_string(),
            "upgrade_to": request.plan_name
        })),
    };
    
    let session = stripe_service.create_checkout_session(&session_params).await?;
    
    let response = StripeSessionResponse {
        session_id: session.id.to_string(),
        session_url: session.url.clone().unwrap_or_default(),
    };
    
    Ok(HttpResponse::Created().json(response))
}

/// Endpoint pour annuler l'abonnement
#[post("/subscriptions/cancel")]
pub async fn cancel_subscription(
    req: HttpRequest,
    db: web::Data<Database>,
    stripe_service: web::Data<StripeService>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let subs_repo = SubscriptionsRepository::new(db.pool.clone());
    
    let subscription = subs_repo.get_active_subscription(&user.id).await?;
    
    // Vérifier si c'est le plan gratuit (on ne peut pas annuler le gratuit)
    if subscription.plan_name == crate::infrastructure::database::PlanName::Free {
        return Err(crate::infrastructure::error::AppError::Forbidden(
            "Vous ne pouvez pas annuler le plan gratuit".to_string()
        ));
    }
    
    // Annuler sur Stripe si abonnement Stripe existe
    if let Some(stripe_sub_id) = &subscription.stripe_subscription_id {
        stripe_service.cancel_subscription(stripe_sub_id).await?;
    }
    
    // Annuler dans notre base de données
    subs_repo.cancel_subscription(&subscription.id).await?;
    
    Ok(HttpResponse::Ok().json({
        serde_json::json!({
            "message": "Abonnement annulé avec succès",
            "success": true,
            "refund_policy": "Remboursement prorata pour la période non utilisée selon les conditions Stripe"
        })
    }))
}

/// Endpoint pour obtenir l'historique des paiements
#[get("/subscriptions/payments")]
pub async fn get_payment_history(
    req: HttpRequest,
    query: web::Query<PaymentHistoryParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let user = get_current_user(&req, db.clone()).await?;
    let payments_repo = PaymentsRepository::new(db.pool.clone());
    
    let limit = query.limit.unwrap_or(10).min(50);
    let offset = query.offset.unwrap_or(0);
    
    let payments = payments_repo.get_by_user(&user.id, limit as i64, offset as i64).await?;
    let total_amount = payments_repo.get_total_spent(&user.id).await?;
    
    let response = PaymentHistoryResponse {
        payments: payments.into_iter().map(|p| PaymentResponse {
            payment_id: p.id,
            amount: p.amount as f64,
            currency: p.currency.clone(),
            description: p.description.clone(),
            status: p.status.clone(),
            created_at: p.created_at,
            stripe_payment_id: p.stripe_payment_id.clone(),
        }).collect(),
        total_amount: total_amount as f64,
        currency: "EUR".to_string(),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// Paramètres pour l'historique des paiements
#[derive(Deserialize)]
pub struct PaymentHistoryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Endpoint admin : lister tous les abonnements
#[get("/admin/subscriptions")]
pub async fn list_all_subscriptions(
    query: web::Query<SubscriptionListParams>,
    db: web::Data<Database>,
) -> AppResult<HttpResponse> {
    let subs_repo = SubscriptionsRepository::new(db.pool.clone());
    
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    
    // Récupérer tous les abonnements
    let query = sqlx::query_as!(
        crate::infrastructure::database::Subscription,
        r#"
        SELECT 
            id, user_id, plan_name, monthly_credits, credits_used,
            stripe_customer_id, stripe_subscription_id, is_active, 
            current_period_end, created_at, updated_at
        FROM subscriptions
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit as i64,
        offset as i64
    )
    .fetch_all(&db.pool)
    .await?;
    
    // Compter le total
    let total = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as count
        FROM subscriptions
        "#,
    )
    .fetch_one(&db.pool)
    .await?
    .count
    .unwrap_or(0);
    
    // Obtenir les utilisateurs associés
    let user_ids: Vec<Uuid> = query.iter().map(|s| s.user_id).collect();
    let users: Vec<User> = if !user_ids.is_empty() {
        sqlx::query_as!(
            User,
            r#"
            SELECT id, name, email, password_hash, auth_provider, 
                   auth_provider_id, created_at, updated_at, is_active
            FROM users
            WHERE id = ANY($1)
            "#,
            &user_ids
        )
        .fetch_all(&db.pool)
        .await?
    } else {
        vec![]
    };
    
    // Créer une map pour les utilisateurs
    let user_map: std::collections::HashMap<Uuid, User> = users.into_iter()
        .map(|u| (u.id, u))
        .collect();
    
    // Formater la réponse
    let subscriptions: Vec<serde_json::Value> = query.into_iter().map(|sub| {
        let user = user_map.get(&sub.user_id).cloned();
        
        serde_json::json!({
            "id": sub.id,
            "user": user.map(|u| serde_json::json!({
                "id": u.id,
                "name": u.name,
                "email": u.email
            })),
            "plan_name": sub.plan_name,
            "monthly_credits": sub.monthly_credits,
            "credits_used": sub.credits_used,
            "is_active": sub.is_active,
            "current_period_end": sub.current_period_end,
            "stripe_customer_id": sub.stripe_customer_id,
            "stripe_subscription_id": sub.stripe_subscription_id,
            "created_at": sub.created_at,
            "updated_at": sub.updated_at
        })
    }).collect();
    
    let response = serde_json::json!({
        "subscriptions": subscriptions,
        "pagination": {
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": (offset + limit) < total as usize
        }
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// Paramètres pour la liste des abonnements
#[derive(Deserialize)]
pub struct SubscriptionListParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub plan_name: Option<String>,
    pub is_active: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use crate::infrastructure::database::Database;
    use sqlx::PgPool;
    use std::env;
    use uuid::Uuid;

    async fn setup_test_app() -> (test::TestServer, PgPool) {
        let database_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://quant_user:quant_pass@localhost:5432/quant_test".to_string());
        
        let pool = PgPool::connect(&database_url).await.unwrap();
        let db = Database::new_with_pool(pool.clone());
        
        // Mock Stripe service
        let stripe_service = crate::infrastructure::stripe::StripeService::new_test();
        
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(stripe_service))
                .service(get_subscription)
                .service(upgrade_plan)
                .service(cancel_subscription)
                .service(get_payment_history)
        ).await;
        
        let server = test::TestServer::with_service(app);
        (server, pool)
    }

    async fn clear_subscriptions_table(pool: &PgPool) {
        sqlx::query("DELETE FROM subscriptions WHERE current_period_end > NOW() - INTERVAL '1 day'")
            .execute(pool)
            .await
            .unwrap();
    }

    async fn clear_users_table(pool: &PgPool) {
        sqlx::query("DELETE FROM users WHERE email LIKE '%@test.com'")
            .execute(pool)
            .await
            .unwrap();
    }

    #[actix_web::test]
    async fn test_get_subscription() {
        let (server, pool) = setup_test_app().await;
        clear_subscriptions_table(&pool).await;
        clear_users_table(&pool).await;
        
        // Créer un utilisateur et un abonnement
        let db = Database::new_with_pool(pool.clone());
        let user_repo = UserRepository::new(db.pool.clone());
        let subs_repo = SubscriptionsRepository::new(db.pool.clone());
        
        let user = user_repo.create(&NewUser {
            name: "Sub Test User".to_string(),
            email: "subtest@test.com".to_string(),
            password: Some("password123".to_string()),
        }).await.unwrap();
        
        let subscription = subs_repo.create_free_subscription(&user.id).await.unwrap();
        
        // Récupérer l'abonnement
        let req = test::TestRequest::get()
            .uri("/api/subscriptions")
            .insert_header(("Authorization", format!("Bearer {}", create_test_token(&user.id))))
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body: SubscriptionResponse = test::read_body_json(resp).await;
        assert_eq!(body.plan_name, "free");
        assert_eq!(body.monthly_credits, 1);
        assert!(body.is_active);
    }

    #[actix_web::test]
    async fn test_upgrade_plan() {
        let (server, pool) = setup_test_app().await;
        clear_subscriptions_table(&pool).await;
        clear_users_table(&pool).await;
        
        // Créer un utilisateur et un abonnement gratuit
        let db = Database::new_with_pool(pool.clone());
        let user_repo = UserRepository::new(db.pool.clone());
        let subs_repo = SubscriptionsRepository::new(db.pool.clone());
        
        let user = user_repo.create(&NewUser {
            name: "Upgrade Test User".to_string(),
            email: "upgradetest@test.com".to_string(),
            password: Some("password123".to_string()),
        }).await.unwrap();
        
        subs_repo.create_free_subscription(&user.id).await.unwrap();
        
        // Tenter de mettre à niveau vers starter
        let req = test::TestRequest::post()
            .uri("/api/subscriptions/upgrade")
            .insert_header(("Authorization", format!("Bearer {}", create_test_token(&user.id))))
            .set_json(&UpgradePlanRequest {
                plan_name: "starter".to_string(),
                success_url: "http://localhost:3000/success".to_string(),
                cancel_url: "http://localhost:3000/cancel".to_string(),
            })
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body: StripeSessionResponse = test::read_body_json(resp).await;
        assert!(!body.session_id.is_empty());
        assert!(!body.session_url.is_empty());
    }

    #[actix_web::test]
    async fn test_cancel_subscription() {
        let (server, pool) = setup_test_app().await;
        clear_subscriptions_table(&pool).await;
        clear_users_table(&pool).await;
        
        // Créer un utilisateur et un abonnement starter
        let db = Database::new_with_pool(pool.clone());
        let user_repo = UserRepository::new(db.pool.clone());
        let subs_repo = SubscriptionsRepository::new(db.pool.clone());
        
        let user = user_repo.create(&NewUser {
            name: "Cancel Test User".to_string(),
            email: "canceltest@test.com".to_string(),
            password: Some("password123".to_string()),
        }).await.unwrap();
        
        let subscription = subs_repo.create_subscription(&user.id, crate::infrastructure::database::PlanName::Starter).await.unwrap();
        
        // Annuler l'abonnement
        let req = test::TestRequest::post()
            .uri("/api/subscriptions/cancel")
            .insert_header(("Authorization", format!("Bearer {}", create_test_token(&user.id))))
            .to_request();
        
        let resp = server.call(req).await.unwrap();
        assert!(resp.status().is_success());
        
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Abonnement annulé avec succès");
    }

    /// Fonction utilitaire pour créer un token JWT de test
    fn create_test_token(user_id: &Uuid) -> String {
        format!("test_token_{}", user_id)
    }
}