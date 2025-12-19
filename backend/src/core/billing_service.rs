// core/billing_service.rs
use crate::models::{
    Subscription, SubscriptionPlan, SubscriptionStatus,
    CreditInfo, CreditTransaction, PlanInfo,
};
use crate::services::database::Database;
use crate::utils::error::{AppError, Result};
use uuid::Uuid;
use chrono::{Utc, DateTime, Duration};
use std::sync::Arc;

pub struct BillingService {
    db: Arc<Database>,
    stripe_secret_key: String,
    stripe_webhook_secret: String,
    stripe_currency: String,
    stripe_trial_days: i64,
}

impl BillingService {
    pub fn new(
        db: Arc<Database>,
        stripe_secret_key: String,
        stripe_webhook_secret: String,
        stripe_currency: String,
        stripe_trial_days: i64,
    ) -> Self {
        Self {
            db,
            stripe_secret_key,
            stripe_webhook_secret,
            stripe_currency,
            stripe_trial_days,
        }
    }

    /// Obtenir l'abonnement d'un utilisateur
    pub async fn get_user_subscription(&self, user_id: Uuid) -> Result<Subscription> {
        self.db.get_user_subscription(user_id).await
    }

    /// Créer un abonnement gratuit
    pub async fn create_free_subscription(&self, user_id: Uuid) -> Result<Subscription> {
        let subscription = Subscription::new_free(user_id);
        self.db.create_subscription(&subscription).await?;
        
        // Crédits initiaux pour le plan gratuit
        self.add_credits(user_id, 1, "initial", "Crédits initiaux pour plan gratuit").await?;
        
        Ok(subscription)
    }

    /// Mettre à jour l'abonnement
    pub async fn update_subscription(
        &self,
        user_id: Uuid,
        plan_name: &str,
        payment_method_id: Option<&str>,
    ) -> Result<Subscription> {
        let current_sub = self.db.get_user_subscription(user_id).await?;
        
        let new_plan = match plan_name.to_lowercase().as_str() {
            "free" => SubscriptionPlan::Free,
            "starter" => SubscriptionPlan::Starter,
            "pro" => SubscriptionPlan::Pro,
            _ => return Err(AppError::InvalidPlan),
        };

        // Si l'utilisateur passe de Free à payant
        if matches!(current_sub.plan, SubscriptionPlan::Free) && !matches!(new_plan, SubscriptionPlan::Free) {
            // Créer un client Stripe si nécessaire
            let stripe_customer_id = self.create_stripe_customer(user_id).await?;
            
            // Créer l'abonnement Stripe
            let stripe_sub_id = self.create_stripe_subscription(
                stripe_customer_id,
                &new_plan,
                payment_method_id,
            ).await?;

            // Mettre à jour l'abonnement en base
            let mut updated_sub = current_sub;
            updated_sub.upgrade(new_plan, Some(stripe_sub_id));
            self.db.update_subscription(&updated_sub).await?;

            // Ajouter les crédits du nouveau plan
            let credits = new_plan.info().credits_per_month;
            if credits > 0 {
                self.add_credits(user_id, credits, "subscription_upgrade", &format!("Mise à jour vers plan {:?}", new_plan)).await?;
            }

            Ok(updated_sub)
        } else {
            // Changer de plan payant
            self.change_stripe_plan(current_sub.stripe_subscription_id.as_deref(), &new_plan).await?;
            
            let mut updated_sub = current_sub;
            updated_sub.plan = new_plan;
            updated_sub.updated_at = Utc::now();
            self.db.update_subscription(&updated_sub).await?;

            Ok(updated_sub)
        }
    }

    /// Annuler un abonnement
    pub async fn cancel_subscription(&self, user_id: Uuid) -> Result<()> {
        let mut subscription = self.db.get_user_subscription(user_id).await?;
        
        if matches!(subscription.plan, SubscriptionPlan::Free) {
            return Err(AppError::NoSubscription);
        }

        // Annuler chez Stripe
        if let Some(stripe_id) = &subscription.stripe_subscription_id {
            self.cancel_stripe_subscription(stripe_id).await?;
        }

        // Rétrograder vers Free
        subscription.plan = SubscriptionPlan::Free;
        subscription.status = SubscriptionStatus::Cancelled;
        subscription.cancelled_at = Some(Utc::now());
        subscription.updated_at = Utc::now();
        subscription.stripe_subscription_id = None;
        subscription.stripe_price_id = None;

        self.db.update_subscription(&subscription).await?;

        Ok(())
    }

    /// Obtenir les informations de crédits
    pub async fn get_user_credits(&self, user_id: Uuid) -> Result<CreditInfo> {
        let total_credits = self.db.get_user_total_credits(user_id).await?;
        let used_credits = self.db.get_user_used_credits(user_id).await?;
        let remaining_credits = total_credits - used_credits;
        
        // Date de réinitialisation (fin du mois pour les plans payants)
        let subscription = self.db.get_user_subscription(user_id).await?;
        let reset_date = subscription.current_period_end;

        Ok(CreditInfo {
            total_credits,
            used_credits,
            remaining_credits,
            reset_date,
        })
    }

    /// Vérifier si un utilisateur a suffisamment de crédits
    pub async fn check_user_credits(&self, user_id: Uuid) -> Result<bool> {
        let credits = self.get_user_credits(user_id).await?;
        Ok(credits.remaining_credits > 0)
    }

    /// Consommer des crédits pour un job
    pub async fn consume_job_credits(&self, user_id: Uuid, job_id: Uuid) -> Result<()> {
        let job = self.db.get_job(job_id).await?;
        let credits_needed = job.credits_used;

        // Vérifier les crédits disponibles
        let current_credits = self.get_user_credits(user_id).await?;
        if current_credits.remaining_credits < credits_needed {
            return Err(AppError::InsufficientCredits);
        }

        // Débiter les crédits
        self.db.create_credit_transaction(
            user_id,
            "consumption",
            -credits_needed,
            &format!("Job de quantification: {}", job.name),
        ).await?;

        Ok(())
    }

    /// Ajouter des crédits à un utilisateur
    pub async fn add_credits(
        &self,
        user_id: Uuid,
        amount: i32,
        transaction_type: &str,
        description: &str,
    ) -> Result<()> {
        self.db.create_credit_transaction(
            user_id,
            transaction_type,
            amount,
            description,
        ).await
    }

    /// Obtenir l'historique des crédits
    pub async fn get_credit_history(
        &self,
        user_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<CreditTransaction>> {
        self.db.get_user_credit_transactions(user_id, page, per_page).await
    }

    /// Réinitialiser les crédits mensuels
    pub async fn reset_monthly_credits(&self) -> Result<u64> {
        let reset_count = self.db.reset_monthly_credits().await?;
        Ok(reset_count)
    }

    /// Gérer un webhook Stripe
    pub async fn handle_stripe_webhook(
        &self,
        payload: &[u8],
        signature: &str,
    ) -> Result<()> {
        use stripe::{Webhook, Event};
        
        // Vérifier la signature
        let event = Webhook::construct_event(
            payload,
            signature,
            &self.stripe_webhook_secret,
        ).map_err(|e| AppError::StripeError(e.to_string()))?;

        match event {
            Event::PaymentIntentSucceeded(payment_intent) => {
                self.handle_payment_success(payment_intent).await?;
            }
            Event::InvoicePaymentSucceeded(invoice) => {
                self.handle_invoice_payment(invoice).await?;
            }
            Event::CustomerSubscriptionDeleted(subscription) => {
                self.handle_subscription_cancelled(subscription).await?;
            }
            Event::ChargeFailed(charge) => {
                self.handle_payment_failed(charge).await?;
            }
            _ => {
                // Ignorer les autres événements pour le MVP
            }
        }

        Ok(())
    }

    /// Créer une session de checkout Stripe
    pub async fn create_checkout_session(
        &self,
        user_id: Uuid,
        plan_name: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<String> {
        let plan = match plan_name.to_lowercase().as_str() {
            "starter" => SubscriptionPlan::Starter,
            "pro" => SubscriptionPlan::Pro,
            _ => return Err(AppError::InvalidPlan),
        };

        let plan_info = plan.info();
        let price_id = self.get_stripe_price_id(&plan).await?;

        use stripe::{CheckoutSession, CheckoutSessionMode, Client, CreateCheckoutSession, CreateCheckoutSessionLineItems, CreateCheckoutSessionPaymentMethodType, CreateCheckoutSessionLineItemsPriceData, CreateCheckoutSessionLineItemsPriceDataProductData, Currency};
        
        let client = Client::new(&self.stripe_secret_key);
        
        let mut create_session = CreateCheckoutSession::new();
        create_session.mode = Some(CheckoutSessionMode::Subscription);
        create_session.success_url = Some(success_url);
        create_session.cancel_url = Some(cancel_url);
        create_session.customer = self.get_stripe_customer_id(user_id).await?;
        create_session.payment_method_types = Some(vec![
            CreateCheckoutSessionPaymentMethodType::Card,
        ]);

        // Ajouter l'élément de ligne (l'abonnement)
        let mut line_item = CreateCheckoutSessionLineItems::default();
        line_item.price = Some(price_id);
        line_item.quantity = Some(1);

        create_session.line_items = Some(vec![line_item]);

        // Créer la session
        let session = CheckoutSession::create(&client, create_session)
            .await
            .map_err(|e| AppError::StripeError(e.to_string()))?;

        Ok(session.url.unwrap_or_default())
    }

    // === Méthodes privées Stripe ===

    async fn create_stripe_customer(&self, user_id: Uuid) -> Result<String> {
        use stripe::{Customer, CreateCustomer, Client};
        
        let user = self.db.get_user_by_id(user_id).await?;
        let client = Client::new(&self.stripe_secret_key);
        
        let mut create_customer = CreateCustomer::new();
        create_customer.email = Some(&user.email);
        create_customer.name = Some(&user.email);
        
        let customer = Customer::create(&client, create_customer)
            .await
            .map_err(|e| AppError::StripeError(e.to_string()))?;
        
        // Sauvegarder l'ID Stripe en base
        self.db.update_user_stripe_id(user_id, &customer.id).await?;
        
        Ok(customer.id)
    }

    async fn get_stripe_customer_id(&self, user_id: Uuid) -> Result<Option<String>> {
        let user = self.db.get_user_by_id(user_id).await?;
        Ok(user.stripe_customer_id)
    }

    async fn create_stripe_subscription(
        &self,
        customer_id: String,
        plan: &SubscriptionPlan,
        payment_method_id: Option<&str>,
    ) -> Result<String> {
        use stripe::{Subscription, CreateSubscription, Client, CreateSubscriptionItems};
        
        let client = Client::new(&self.stripe_secret_key);
        let price_id = self.get_stripe_price_id(plan).await?;
        
        let mut create_sub = CreateSubscription::new(customer_id);
        
        let mut item = CreateSubscriptionItems::default();
        item.price = Some(price_id);
        create_sub.items = Some(vec![item]);
        
        if let Some(pm_id) = payment_method_id {
            create_sub.default_payment_method = Some(pm_id);
        }
        
        if self.stripe_trial_days > 0 {
            create_sub.trial_period_days = Some(self.stripe_trial_days as u64);
        }
        
        let subscription = Subscription::create(&client, create_sub)
            .await
            .map_err(|e| AppError::StripeError(e.to_string()))?;
        
        Ok(subscription.id)
    }

    async fn get_stripe_price_id(&self, plan: &SubscriptionPlan) -> Result<String> {
        match plan {
            SubscriptionPlan::Free => Ok("price_free_mock".to_string()),
            SubscriptionPlan::Starter => {
                // En production, récupérer depuis la config
                Ok(std::env::var("STRIPE_PRICE_STARTER")
                    .unwrap_or_else(|_| "price_starter_monthly".to_string()))
            }
            SubscriptionPlan::Pro => {
                Ok(std::env::var("STRIPE_PRICE_PRO")
                    .unwrap_or_else(|_| "price_pro_monthly".to_string()))
            }
        }
    }

    async fn change_stripe_plan(
        &self,
        subscription_id: Option<&str>,
        new_plan: &SubscriptionPlan,
    ) -> Result<()> {
        if let Some(sub_id) = subscription_id {
            use stripe::{Subscription, UpdateSubscription, Client};
            
            let client = Client::new(&self.stripe_secret_key);
            let new_price_id = self.get_stripe_price_id(new_plan).await?;
            
            let mut update_sub = UpdateSubscription::default();
            update_sub.items = Some(vec![stripe::UpdateSubscriptionItems {
                price: Some(new_price_id),
                ..Default::default()
            }]);
            
            Subscription::update(&client, sub_id, update_sub)
                .await
                .map_err(|e| AppError::StripeError(e.to_string()))?;
        }
        
        Ok(())
    }

    async fn cancel_stripe_subscription(&self, subscription_id: &str) -> Result<()> {
        use stripe::{Subscription, CancelSubscription, Client};
        
        let client = Client::new(&self.stripe_secret_key);
        let cancel_sub = CancelSubscription::default();
        
        Subscription::cancel(&client, subscription_id, cancel_sub)
            .await
            .map_err(|e| AppError::StripeError(e.to_string()))?;
        
        Ok(())
    }

    async fn handle_payment_success(&self, payment_intent: stripe::PaymentIntent) -> Result<()> {
        // TODO: Implémenter la logique de traitement du paiement
        Ok(())
    }

    async fn handle_invoice_payment(&self, invoice: stripe::Invoice) -> Result<()> {
        // TODO: Implémenter la logique de facturation
        Ok(())
    }

    async fn handle_subscription_cancelled(&self, subscription: stripe::Subscription) -> Result<()> {
        // TODO: Implémenter la logique d'annulation
        Ok(())
    }

    async fn handle_payment_failed(&self, charge: stripe::Charge) -> Result<()> {
        // TODO: Implémenter la logique d'échec de paiement
        Ok(())
    }
}