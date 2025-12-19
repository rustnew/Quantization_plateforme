// core/notification_service.rs
use crate::models::{Job, SubscriptionPlan};
use crate::utils::error::{AppError, Result};
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;
use serde_json::json;

pub struct NotificationService {
    email_provider: Arc<dyn EmailProvider + Send + Sync>,
    sms_provider: Option<Arc<dyn SmsProvider + Send + Sync>>,
    websocket_broadcaster: broadcast::Sender<WebSocketMessage>,
    frontend_url: String,
}

impl NotificationService {
    pub fn new(
        email_provider: Arc<dyn EmailProvider + Send + Sync>,
        sms_provider: Option<Arc<dyn SmsProvider + Send + Sync>>,
        frontend_url: String,
    ) -> Self {
        let (tx, _) = broadcast::channel(100);
        
        Self {
            email_provider,
            sms_provider,
            websocket_broadcaster: tx,
            frontend_url,
        }
    }

    /// Envoyer une notification de job terminé
    pub async fn send_job_completed(&self, user_id: Uuid, job: &Job) -> Result<()> {
        let user_email = self.get_user_email(user_id).await?;
        
        let email_subject = format!("Votre job '{}' est terminé", job.name);
        let email_body = format!(
            r#"Bonjour,

Votre job de quantification "{}" a été terminé avec succès.

Détails du job:
- ID: {}
- Méthode: {:?}
- Taille originale: {:.2} GB
- Taille quantifiée: {:.2} GB
- Ratio de compression: {:.1}%

Vous pouvez télécharger votre modèle quantifié à cette adresse:
{}/jobs/{}/download

Cordialement,
L'équipe Quantization Platform"#,
            job.name,
            job.id,
            job.quantization_method,
            job.original_size.unwrap_or(0) as f64 / 1e9,
            job.quantized_size.unwrap_or(0) as f64 / 1e9,
            job.compression_ratio().unwrap_or(0.0) * 100.0,
            self.frontend_url,
            job.id
        );

        // Envoyer l'email
        self.email_provider.send(
            &user_email,
            &email_subject,
            &email_body,
        ).await?;

        // Envoyer une notification WebSocket
        let ws_message = WebSocketMessage {
            user_id,
            event_type: "job.completed".to_string(),
            data: json!({
                "job_id": job.id,
                "job_name": job.name,
                "status": "completed",
                "download_url": format!("{}/jobs/{}/download", self.frontend_url, job.id),
            }),
        };

        let _ = self.websocket_broadcaster.send(ws_message);

        Ok(())
    }

    /// Envoyer une notification de job échoué
    pub async fn send_job_failed(&self, user_id: Uuid, job: &Job, error: &str) -> Result<()> {
        let user_email = self.get_user_email(user_id).await?;
        
        let email_subject = format!("Votre job '{}' a échoué", job.name);
        let email_body = format!(
            r#"Bonjour,

Votre job de quantification "{}" a échoué.

Détails:
- ID: {}
- Méthode: {:?}
- Erreur: {}

Nous vous invitons à vérifier votre fichier source et à réessayer.

Cordialement,
L'équipe Quantization Platform"#,
            job.name,
            job.id,
            job.quantization_method,
            error
        );

        self.email_provider.send(
            &user_email,
            &email_subject,
            &email_body,
        ).await?;

        // Notification WebSocket
        let ws_message = WebSocketMessage {
            user_id,
            event_type: "job.failed".to_string(),
            data: json!({
                "job_id": job.id,
                "job_name": job.name,
                "status": "failed",
                "error": error,
            }),
        };

        let _ = self.websocket_broadcaster.send(ws_message);

        Ok(())
    }

    /// Envoyer un email de bienvenue
    pub async fn send_welcome_email(&self, user_id: Uuid, user_email: &str) -> Result<()> {
        let subject = "Bienvenue sur Quantization Platform!";
        let body = format!(
            r#"Bienvenue sur Quantization Platform!

Nous sommes ravis de vous accueillir sur notre plateforme de quantification de modèles d'IA.

Avec votre compte, vous pouvez:
- Quantifier vos modèles jusqu'à 4x plus petits
- Réduire vos coûts d'inférence jusqu'à 70%
- Déployer sur edge devices

Commencez dès maintenant: {}/dashboard

Besoin d'aide? Consultez notre documentation ou contactez notre support.

Cordialement,
L'équipe Quantization Platform"#,
            self.frontend_url
        );

        self.email_provider.send(user_email, subject, &body).await
    }

    /// Envoyer un email de réinitialisation de mot de passe
    pub async fn send_password_reset(&self, user_id: Uuid, reset_token: &str) -> Result<()> {
        let user_email = self.get_user_email(user_id).await?;
        
        let reset_url = format!("{}/reset-password?token={}", self.frontend_url, reset_token);
        
        let subject = "Réinitialisation de votre mot de passe";
        let body = format!(
            r#"Bonjour,

Vous avez demandé la réinitialisation de votre mot de passe.

Cliquez sur le lien suivant pour choisir un nouveau mot de passe:
{}

Ce lien expirera dans 24 heures.

Si vous n'avez pas demandé cette réinitialisation, veuillez ignorer cet email.

Cordialement,
L'équipe Quantization Platform"#,
            reset_url
        );

        self.email_provider.send(&user_email, subject, &body).await
    }

    /// Envoyer une notification de crédits épuisés
    pub async fn send_low_credits_notification(&self, user_id: Uuid, remaining_credits: i32) -> Result<()> {
        if remaining_credits > 0 {
            return Ok(());
        }

        let user_email = self.get_user_email(user_id).await?;
        
        let subject = "Vos crédits sont épuisés";
        let body = format!(
            r#"Bonjour,

Vos crédits de quantification sont épuisés.

Pour continuer à utiliser la plateforme, vous pouvez:
1. Attendre la réinitialisation mensuelle de vos crédits
2. Passer à un plan supérieur pour obtenir plus de crédits
3. Acheter des crédits supplémentaires

Consultez vos options: {}/billing

Cordialement,
L'équipe Quantization Platform"#,
            self.frontend_url
        );

        self.email_provider.send(&user_email, subject, &body).await
    }

    /// Envoyer une notification de changement d'abonnement
    pub async fn send_subscription_change(
        &self,
        user_id: Uuid,
        old_plan: &SubscriptionPlan,
        new_plan: &SubscriptionPlan,
    ) -> Result<()> {
        let user_email = self.get_user_email(user_id).await?;
        
        let subject = "Changement de votre abonnement";
        let body = format!(
            r#"Bonjour,

Votre abonnement a été modifié.

Ancien plan: {:?}
Nouveau plan: {:?}

Vos nouveaux avantages:
- Crédits mensuels: {}
- Priorité dans la queue: {}
- Rétention des fichiers: {} jours

Merci pour votre confiance!

Cordialement,
L'équipe Quantization Platform"#,
            old_plan,
            new_plan,
            new_plan.info().credits_per_month,
            new_plan.queue_priority(),
            match new_plan {
                SubscriptionPlan::Free => 7,
                SubscriptionPlan::Starter => 30,
                SubscriptionPlan::Pro => 90,
            }
        );

        self.email_provider.send(&user_email, subject, &body).await
    }

    /// Obtenir un receiver pour les WebSocket
    pub fn get_websocket_receiver(&self) -> broadcast::Receiver<WebSocketMessage> {
        self.websocket_broadcaster.subscribe()
    }

    /// Envoyer une notification en temps réel via WebSocket
    pub fn send_websocket_notification(&self, user_id: Uuid, event_type: &str, data: serde_json::Value) -> Result<()> {
        let message = WebSocketMessage {
            user_id,
            event_type: event_type.to_string(),
            data,
        };

        self.websocket_broadcaster.send(message)
            .map_err(|e| AppError::NotificationError(e.to_string()))?;

        Ok(())
    }

    /// Obtenir l'email de l'utilisateur
    async fn get_user_email(&self, user_id: Uuid) -> Result<String> {
        // Dans une vraie implémentation, on récupérerait depuis la base
        // Pour le MVP, on simule
        Ok(format!("user_{}@example.com", user_id))
    }
}

// Traits pour les fournisseurs de notification
#[async_trait::async_trait]
pub trait EmailProvider: Send + Sync {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()>;
}

#[async_trait::async_trait]
pub trait SmsProvider: Send + Sync {
    async fn send_sms(&self, phone_number: &str, message: &str) -> Result<()>;
}

// Implémentation pour les logs (développement)
pub struct LogEmailProvider;

#[async_trait::async_trait]
impl EmailProvider for LogEmailProvider {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()> {
        println!("[EMAIL] To: {}", to);
        println!("[EMAIL] Subject: {}", subject);
        println!("[EMAIL] Body:\n{}", body);
        Ok(())
    }
}

// Message WebSocket
#[derive(Debug, Clone)]
pub struct WebSocketMessage {
    pub user_id: Uuid,
    pub event_type: String,
    pub data: serde_json::Value,
}