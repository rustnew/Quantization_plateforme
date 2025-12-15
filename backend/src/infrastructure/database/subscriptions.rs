

use sqlx::{Pool, Postgres, Error as SqlxError, Row, query_as, query_scalar, query};
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use validator::Validate;
use serde::{Serialize, Deserialize};


use crate::{
    domain::jobs::Job,
    infrastructure::error::{AppError, AppResult},
};

/// Plans d'abonnement disponibles
#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum PlanName {
    Free,
    Starter,
    Pro,
}

impl PlanName {
    /// Retourne le nombre de crédits mensuels pour chaque plan
    pub fn monthly_credits(&self) -> i32 {
        match self {
            PlanName::Free => 1,
            PlanName::Starter => 10,
            PlanName::Pro => 1000, // Illimité pour les pratiques
        }
    }

    /// Retourne le prix mensuel en EUR
    pub fn monthly_price(&self) -> f64 {
        match self {
            PlanName::Free => 0.0,
            PlanName::Starter => 19.0,
            PlanName::Pro => 99.0,
        }
    }

    /// Retourne la description marketing du plan
    pub fn description(&self) -> &'static str {
        match self {
            PlanName::Free => "1 quantification gratuite par mois",
            PlanName::Starter => "10 quantifications par mois (INT8/INT4)",
            PlanName::Pro => "Quantifications illimitées + support prioritaire",
        }
    }
}

/// Repository pour les opérations sur les abonnements
#[derive(Clone)]
pub struct SubscriptionsRepository {
    pool: Pool<Postgres>,
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("Abonnement non trouvé")]
    NotFound,
    #[error("Aucun crédit disponible pour cet abonnement")]
    NoCreditsAvailable,
    #[error("Abonnement déjà actif")]
    AlreadyActive,
    #[error("Abonnement expiré ou inactif")]
    Inactive,
    #[error("Plan d'abonnement invalide")]
    InvalidPlan,
    #[error("ID Stripe invalide")]
    InvalidStripeId,
    #[error("Période de facturation invalide")]
    InvalidBillingPeriod,
    #[error("Erreur de base de données: {0}")]
    DatabaseError(#[from] SqlxError),
}

impl From<SubscriptionError> for AppError {
    fn from(error: SubscriptionError) -> Self {
        match error {
            SubscriptionError::NotFound => AppError::NotFound("Abonnement".to_string()),
            SubscriptionError::NoCreditsAvailable => AppError::Forbidden("Aucun crédit disponible".to_string()),
            SubscriptionError::AlreadyActive => AppError::Conflict("Abonnement déjà actif".to_string()),
            SubscriptionError::Inactive => AppError::Forbidden("Abonnement inactif".to_string()),
            SubscriptionError::InvalidPlan => AppError::ValidationError(
                validator::ValidationError::new("plan_name").into()
            ),
            SubscriptionError::InvalidStripeId => AppError::ValidationError(
                validator::ValidationError::new("stripe_id").into()
            ),
            SubscriptionError::InvalidBillingPeriod => AppError::ValidationError(
                validator::ValidationError::new("billing_period").into()
            ),
            SubscriptionError::DatabaseError(e) => AppError::DatabaseError(e),
        }
    }
}

impl SubscriptionsRepository {
    /// Crée une nouvelle instance du repository
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Crée un abonnement gratuit pour un nouvel utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement créé
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn create_free_subscription(&self, user_id: &Uuid) -> AppResult<Subscription> {
        self.create_subscription(user_id, PlanName::Free).await
    }

    /// Crée un abonnement pour un utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// * `plan_name` - Le plan d'abonnement
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement créé
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn create_subscription(&self, user_id: &Uuid, plan_name: PlanName) -> AppResult<Subscription> {
        let subscription_id = Uuid::new_v4();
        let now = Utc::now();
        let current_period_end = now + Duration::days(30); // Période de 30 jours

        let subscription = query_as!(
            Subscription,
            r#"
            INSERT INTO subscriptions (
                id, user_id, plan_name, monthly_credits, credits_used,
                is_active, current_period_end, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            subscription_id,
            user_id,
            plan_name.to_string(),
            plan_name.monthly_credits(),
            0,
            true,
            current_period_end,
            now,
            now
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(subscription)
    }

    /// Récupère l'abonnement actif d'un utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement trouvé
    /// * `Err(AppError)` - Si aucun abonnement actif n'existe
    pub async fn get_active_subscription(&self, user_id: &Uuid) -> AppResult<Subscription> {
        let subscription = query_as!(
            Subscription,
            r#"
            SELECT 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            FROM subscriptions
            WHERE user_id = $1 AND is_active = true
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SubscriptionError::NotFound)?;

        // Vérifier si l'abonnement est expiré
        if subscription.current_period_end < Utc::now() && subscription.plan_name != PlanName::Pro {
            return Err(SubscriptionError::Inactive.into());
        }

        Ok(subscription)
    }

    /// Met à jour les informations Stripe d'un abonnement
    /// 
    /// # Arguments
    /// * `subscription_id` - L'identifiant de l'abonnement
    /// * `stripe_customer_id` - ID client Stripe
    /// * `stripe_subscription_id` - ID abonnement Stripe
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement mis à jour
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn update_with_stripe_info(
        &self,
        subscription_id: &Uuid,
        stripe_customer_id: &str,
        stripe_subscription_id: &str,
    ) -> AppResult<Subscription> {
        // Validation basique des IDs Stripe
        if !stripe_customer_id.starts_with("cus_") || stripe_customer_id.len() < 10 {
            return Err(SubscriptionError::InvalidStripeId.into());
        }
        if !stripe_subscription_id.starts_with("sub_") || stripe_subscription_id.len() < 10 {
            return Err(SubscriptionError::InvalidStripeId.into());
        }

        let now = Utc::now();
        let updated_subscription = query_as!(
            Subscription,
            r#"
            UPDATE subscriptions
            SET 
                stripe_customer_id = $1,
                stripe_subscription_id = $2,
                updated_at = $3
            WHERE id = $4
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            stripe_customer_id,
            stripe_subscription_id,
            now,
            subscription_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_subscription)
    }

    /// Consomme un crédit pour un utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(())` - Si le crédit a été consommé
    /// * `Err(AppError)` - Si aucun crédit disponible
    pub async fn consume_credit(&self, user_id: &Uuid) -> AppResult<()> {
        let mut subscription = self.get_active_subscription(user_id).await?;
        
        // Vérifier si des crédits sont disponibles
        if subscription.credits_used >= subscription.monthly_credits {
            return Err(SubscriptionError::NoCreditsAvailable.into());
        }

        // Consommer un crédit
        subscription.credits_used += 1;

        let now = Utc::now();
        query!(
            r#"
            UPDATE subscriptions
            SET credits_used = $1, updated_at = $2
            WHERE id = $3
            "#,
            subscription.credits_used,
            now,
            subscription.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Vérifie si un utilisateur a des crédits disponibles
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// 
    /// # Retourne
    /// * `Ok(bool)` - true si des crédits sont disponibles
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn has_credits_available(&self, user_id: &Uuid) -> AppResult<bool> {
        let subscription = self.get_active_subscription(user_id).await?;
        Ok(subscription.credits_used < subscription.monthly_credits)
    }

    /// Réinitialise les crédits utilisés pour un nouvel cycle de facturation
    /// 
    /// # Arguments
    /// * `subscription_id` - L'identifiant de l'abonnement
    /// * `new_period_end` - Date de fin du nouveau cycle
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement mis à jour
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn reset_credits_for_new_cycle(
        &self,
        subscription_id: &Uuid,
        new_period_end: DateTime<Utc>,
    ) -> AppResult<Subscription> {
        if new_period_end <= Utc::now() {
            return Err(SubscriptionError::InvalidBillingPeriod.into());
        }

        let now = Utc::now();
        let updated_subscription = query_as!(
            Subscription,
            r#"
            UPDATE subscriptions
            SET 
                credits_used = 0,
                current_period_end = $1,
                updated_at = $2
            WHERE id = $3
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            new_period_end,
            now,
            subscription_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_subscription)
    }

    /// Met à jour le plan d'abonnement d'un utilisateur
    /// 
    /// # Arguments
    /// * `subscription_id` - L'identifiant de l'abonnement
    /// * `new_plan` - Le nouveau plan d'abonnement
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement mis à jour
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn update_plan(&self, subscription_id: &Uuid, new_plan: PlanName) -> AppResult<Subscription> {
        let now = Utc::now();
        let updated_subscription = query_as!(
            Subscription,
            r#"
            UPDATE subscriptions
            SET 
                plan_name = $1,
                monthly_credits = $2,
                updated_at = $3
            WHERE id = $4
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            new_plan.to_string(),
            new_plan.monthly_credits(),
            now,
            subscription_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_subscription)
    }

    /// Désactive un abonnement (résiliation)
    /// 
    /// # Arguments
    /// * `subscription_id` - L'identifiant de l'abonnement
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement désactivé
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn cancel_subscription(&self, subscription_id: &Uuid) -> AppResult<Subscription> {
        let now = Utc::now();
        let cancelled_subscription = query_as!(
            Subscription,
            r#"
            UPDATE subscriptions
            SET 
                is_active = false,
                updated_at = $1
            WHERE id = $2
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            now,
            subscription_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(cancelled_subscription)
    }

    /// Réactive un abonnement annulé
    /// 
    /// # Arguments
    /// * `subscription_id` - L'identifiant de l'abonnement
    /// 
    /// # Retourne
    /// * `Ok(Subscription)` - L'abonnement réactivé
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn reactivate_subscription(&self, subscription_id: &Uuid) -> AppResult<Subscription> {
        let now = Utc::now();
        let current_period_end = now + Duration::days(30);
        
        let reactivated_subscription = query_as!(
            Subscription,
            r#"
            UPDATE subscriptions
            SET 
                is_active = true,
                current_period_end = $1,
                updated_at = $2
            WHERE id = $3
            RETURNING 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            "#,
            current_period_end,
            now,
            subscription_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(reactivated_subscription)
    }

    /// Récupère l'historique des abonnements d'un utilisateur
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// * `limit` - Nombre maximum d'abonnements à retourner
    /// 
    /// # Retourne
    /// * `Ok(Vec<Subscription>)` - Liste des abonnements
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn get_subscription_history(&self, user_id: &Uuid, limit: i64) -> AppResult<Vec<Subscription>> {
        let subscriptions = query_as!(
            Subscription,
            r#"
            SELECT 
                id, user_id, plan_name, monthly_credits, credits_used,
                stripe_customer_id, stripe_subscription_id, is_active, 
                current_period_end, created_at, updated_at
            FROM subscriptions
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            user_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(subscriptions)
    }

    /// Calcule le coût d'un job de quantification
    /// 
    /// # Arguments
    /// * `job` - Le job de quantification
    /// 
    /// # Retourne
    /// * `f64` - Le coût en EUR
    pub fn calculate_job_cost(job: &Job) -> f64 {
        match job.quantization_method {
            crate::domain::job::QuantizationMethod::Int8 => 2.0,
            crate::domain::job::QuantizationMethod::Int4 | 
            crate::domain::job::QuantizationMethod::Gptq | 
            crate::domain::job::QuantizationMethod::Awq => 4.0,
            _ => 2.0, // Valeur par défaut
        }
    }

    /// Vérifie si un utilisateur peut créer un job avec ses crédits restants
    /// 
    /// # Arguments
    /// * `user_id` - L'identifiant de l'utilisateur
    /// * `job` - Le job à créer
    /// 
    /// # Retourne
    /// * `Ok(true)` - Si l'utilisateur peut créer le job
    /// * `Ok(false)` - Si l'utilisateur n'a pas assez de crédits
    /// * `Err(AppError)` - En cas d'erreur de base de données
    pub async fn can_create_job(&self, user_id: &Uuid, job: &Job) -> AppResult<bool> {
        let subscription = self.get_active_subscription(user_id).await?;
        
        // Les administrateurs et les utilisateurs Pro ont des crédits illimités
        if subscription.plan_name == PlanName::Pro {
            return Ok(true);
        }

        // Calculer le nombre de crédits nécessaires
        let credits_needed = match job.quantization_method {
            crate::domain::job::QuantizationMethod::Int8 => 1,
            _ => 2, // INT4/GPTQ/AWQ coûtent plus cher
        };

        Ok((subscription.monthly_credits - subscription.credits_used) >= credits_needed)
    }
}

/// Représente un abonnement dans la base de données
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub user_id: Uuid,
    pub plan_name: PlanName,
    pub monthly_credits: i32,
    pub credits_used: i32,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub is_active: bool,
    pub current_period_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Données pour créer un nouvel abonnement
#[derive(Debug, Clone, Validate)]
pub struct NewSubscription {
    pub user_id: Uuid,
    pub plan_name: PlanName,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
}
