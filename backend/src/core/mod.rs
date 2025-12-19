// core/mod.rs
pub mod user_service;
pub mod job_service;
pub mod quantization_service;
pub mod billing_service;
pub mod notification_service;

// Ré-exports pour faciliter l'import
pub use user_service::UserService;
pub use job_service::JobService;
pub use quantization_service::QuantizationService;
pub use billing_service::BillingService;
pub use notification_service::{NotificationService, EmailProvider, SmsProvider, LogEmailProvider};