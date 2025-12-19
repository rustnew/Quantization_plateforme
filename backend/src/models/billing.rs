use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Plan d'abonnement
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "subscription_plan", rename_all = "snake_case")]
pub enum SubscriptionPlan {
    Free,      // Gratuit
    Starter,   // Starter
    Pro,       // Professionnel
}

/// État d'un abonnement
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "subscription_status", rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,       // Actif
    PastDue,      // En retard de paiement
    Cancelled,    // Annulé
    Trialing,     // En période d'essai
}

/// Un abonnement utilisateur
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Subscription {
    /// ID unique
    pub id: Uuid,
    
    /// ID de l'utilisateur
    pub user_id: Uuid,
    
    /// Plan d'abonnement
    pub plan: SubscriptionPlan,
    
    /// État actuel
    pub status: SubscriptionStatus,
    
    /// Date de début de la période
    pub current_period_start: DateTime<Utc>,
    
    /// Date de fin de la période
    pub current_period_end: DateTime<Utc>,
    
    /// ID Stripe de l'abonnement
    pub stripe_subscription_id: Option<String>,
    
    /// ID Stripe du prix
    pub stripe_price_id: Option<String>,
    
    /// Date d'annulation
    pub cancelled_at: Option<DateTime<Utc>>,
    
    /// Date de création
    pub created_at: DateTime<Utc>,
    
    /// Date de mise à jour
    pub updated_at: DateTime<Utc>,
}

/// Informations de crédits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditInfo {
    pub total_credits: i32,
    pub used_credits: i32,
    pub remaining_credits: i32,
    pub reset_date: Option<DateTime<Utc>>,
}

/// Transaction de crédits
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CreditTransaction {
    /// ID unique
    pub id: Uuid,
    
    /// ID de l'utilisateur
    pub user_id: Uuid,
    
    /// Type de transaction
    pub transaction_type: String, // "purchase", "consumption", "reset", "bonus"
    
    /// Montant (positif = ajout, négatif = consommation)
    pub amount: i32,
    
    /// Solde après transaction
    pub balance_after: i32,
    
    /// ID du job lié (si consommation)
    pub job_id: Option<Uuid>,
    
    /// Description
    pub description: Option<String>,
    
    /// Date de la transaction
    pub created_at: DateTime<Utc>,
}

/// Informations de plan pour l'API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInfo {
    pub plan: SubscriptionPlan,
    pub name: String,
    pub price_monthly: i32, // en centimes d'euros
    pub credits_per_month: i32,
    pub features: Vec<String>,
}

impl SubscriptionPlan {
    /// Retourne les informations du plan
    pub fn info(&self) -> PlanInfo {
        match self {
            SubscriptionPlan::Free => PlanInfo {
                plan: SubscriptionPlan::Free,
                name: "Free".to_string(),
                price_monthly: 0,
                credits_per_month: 1,
                features: vec![
                    "1 quantification gratuite par mois".to_string(),
                    "Support de base".to_string(),
                    "Fichiers conservés 7 jours".to_string(),
                ],
            },
            SubscriptionPlan::Starter => PlanInfo {
                plan: SubscriptionPlan::Starter,
                name: "Starter".to_string(),
                price_monthly: 1900, // 19€
                credits_per_month: 10,
                features: vec![
                    "10 crédits par mois".to_string(),
                    "Support prioritaire".to_string(),
                    "Fichiers conservés 30 jours".to_string(),
                    "Queue prioritaire".to_string(),
                ],
            },
            SubscriptionPlan::Pro => PlanInfo {
                plan: SubscriptionPlan::Pro,
                name: "Pro".to_string(),
                price_monthly: 9900, // 99€
                credits_per_month: -1, // Illimité
                features: vec![
                    "Crédits illimités".to_string(),
                    "Support dédié".to_string(),
                    "Fichiers conservés 90 jours".to_string(),
                    "Queue haute priorité".to_string(),
                    "API étendue".to_string(),
                ],
            },
        }
    }
    
    /// Coût en crédits pour un job
    pub fn job_cost(&self, job_type: &str) -> i32 {
        match self {
            SubscriptionPlan::Free => 1, // 1 crédit = 1 job
            SubscriptionPlan::Starter => match job_type {
                "int8" => 1,
                "gptq" => 2,
                "awq" => 2,
                "gguf" => 1,
                _ => 1,
            },
            SubscriptionPlan::Pro => 0, // Gratuit pour Pro
        }
    }
    
    /// Priorité dans la queue
    pub fn queue_priority(&self) -> i32 {
        match self {
            SubscriptionPlan::Free => 1,
            SubscriptionPlan::Starter => 2,
            SubscriptionPlan::Pro => 3,
        }
    }
}

impl Subscription {
    /// Crée un nouvel abonnement gratuit
    pub fn new_free(user_id: Uuid) -> Self {
        let now = Utc::now();
        
        Self {
            id: Uuid::new_v4(),
            user_id,
            plan: SubscriptionPlan::Free,
            status: SubscriptionStatus::Active,
            current_period_start: now,
            current_period_end: now + chrono::Duration::days(30),
            stripe_subscription_id: None,
            stripe_price_id: None,
            cancelled_at: None,
            created_at: now,
            updated_at: now,
        }
    }
    
    /// Vérifie si l'abonnement est actif
    pub fn is_active(&self) -> bool {
        self.status == SubscriptionStatus::Active && Utc::now() < self.current_period_end
    }
    
    /// Met à jour le plan
    pub fn upgrade(&mut self, new_plan: SubscriptionPlan, stripe_subscription_id: Option<String>) {
        let now = Utc::now();
        
        self.plan = new_plan;
        self.status = SubscriptionStatus::Active;
        self.current_period_start = now;
        self.current_period_end = now + chrono::Duration::days(30);
        self.stripe_subscription_id = stripe_subscription_id;
        self.updated_at = now;
        self.cancelled_at = None;
    }
    
    /// Annule l'abonnement
    pub fn cancel(&mut self) {
        self.status = SubscriptionStatus::Cancelled;
        self.cancelled_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}