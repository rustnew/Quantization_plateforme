// services/external.rs
use crate::utils::error::{AppError, Result};
use reqwest::{Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Client pour l'authentification Google
pub struct GoogleAuthClient {
    http_client: Arc<HttpClient>,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl GoogleAuthClient {
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        let http_client = Arc::new(
            HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client")
        );

        Self {
            http_client,
            client_id,
            client_secret,
            redirect_uri,
        }
    }

    /// Vérifier un token Google
    pub async fn verify_token(&self, token: &str) -> Result<GoogleUserInfo> {
        let response = self.http_client
            .get("https://www.googleapis.com/oauth2/v3/tokeninfo")
            .query(&[("id_token", token)])
            .send()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if response.status() != StatusCode::OK {
            return Err(AppError::InvalidToken);
        }

        let token_info: GoogleTokenInfo = response
            .json()
            .await
            .map_err(|e| AppError::ParseError(e.to_string()))?;

        // Vérifier l'audience
        if token_info.aud != self.client_id {
            return Err(AppError::InvalidToken);
        }

        // Vérifier l'expiration
        let now = chrono::Utc::now().timestamp();
        if token_info.exp < now {
            return Err(AppError::TokenExpired);
        }

        Ok(GoogleUserInfo {
            email: token_info.email,
            name: token_info.name,
            picture: token_info.picture,
            locale: token_info.locale,
        })
    }

    /// Obtenir l'URL d'authentification Google
    pub fn get_auth_url(&self, state: &str) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "email profile"),
            ("state", state),
            ("access_type", "online"),
            ("prompt", "consent"),
        ];

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("https://accounts.google.com/o/oauth2/v2/auth?{}", query_string)
    }

    /// Échanger un code d'autorisation contre un token
    pub async fn exchange_code(&self, code: &str) -> Result<GoogleTokenResponse> {
        let params = [
            ("code", code),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("redirect_uri", &self.redirect_uri),
            ("grant_type", "authorization_code"),
        ];

        let response = self.http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if response.status() != StatusCode::OK {
            return Err(AppError::InvalidToken);
        }

        let token_response: GoogleTokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::ParseError(e.to_string()))?;

        Ok(token_response)
    }
}

/// Client SendGrid pour les emails
pub struct SendGridClient {
    http_client: Arc<HttpClient>,
    api_key: String,
    from_email: String,
    from_name: String,
}

impl SendGridClient {
    pub fn new(api_key: String, from_email: String, from_name: String) -> Self {
        let http_client = Arc::new(
            HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client")
        );

        Self {
            http_client,
            api_key,
            from_email,
            from_name,
        }
    }

    /// Envoyer un email
    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        html_content: &str,
        text_content: Option<&str>,
    ) -> Result<()> {
        let payload = SendGridEmail {
            personalizations: vec![SendGridPersonalization {
                to: vec![SendGridEmailAddress {
                    email: to.to_string(),
                    name: None,
                }],
                subject: Some(subject.to_string()),
            }],
            from: SendGridEmailAddress {
                email: self.from_email.clone(),
                name: Some(self.from_name.clone()),
            },
            content: vec![
                SendGridEmailContent {
                    type_field: "text/plain".to_string(),
                    value: text_content.unwrap_or(html_content).to_string(),
                },
                SendGridEmailContent {
                    type_field: "text/html".to_string(),
                    value: html_content.to_string(),
                },
            ],
            subject: subject.to_string(),
        };

        let response = self.http_client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(AppError::ExternalService(format!("SendGrid error: {}", error_text)))
        }
    }

    /// Vérifier la santé du service
    pub async fn health_check(&self) -> Result<()> {
        let response = self.http_client
            .get("https://api.sendgrid.com/v3/user/profile")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(AppError::ExternalService("SendGrid health check failed".to_string()))
        }
    }
}

/// Client Python pour exécuter des scripts
pub struct PythonClient {
    scripts_dir: std::path::PathBuf,
    python_path: String,
    timeout_seconds: u64,
}

impl PythonClient {
    pub fn new(scripts_dir: &str, python_path: Option<&str>, timeout_seconds: u64) -> Self {
        Self {
            scripts_dir: std::path::PathBuf::from(scripts_dir),
            python_path: python_path.unwrap_or("python3").to_string(),
            timeout_seconds,
        }
    }

    /// Exécuter un script Python
    pub async fn call_script(&self, script_name: &str, args: &[&str]) -> Result<String> {
        let script_path = self.scripts_dir.join(script_name);
        
        if !script_path.exists() {
            return Err(AppError::ExternalService(format!("Script not found: {}", script_name)));
        }

        let mut command = tokio::process::Command::new(&self.python_path);
        command.arg(&script_path);
        
        for arg in args {
            command.arg(arg);
        }

        let output = command
            .output()
            .await
            .map_err(|e| AppError::ExternalService(e.to_string()))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| AppError::ParseError(e.to_string()))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AppError::ExternalService(format!(
                "Python script failed: {}",
                stderr
            )))
        }
    }

    /// Vérifier les dépendances Python
    pub async fn check_dependencies(&self) -> Result<Vec<DependencyStatus>> {
        let scripts = ["quantize_int8.py", "quantize_gptq.py", "convert_gguf.py"];
        let mut statuses = Vec::new();

        for script in &scripts {
            let script_path = self.scripts_dir.join(script);
            
            let status = if script_path.exists() {
                DependencyStatus {
                    name: script.to_string(),
                    status: "ok".to_string(),
                    version: "present".to_string(),
                }
            } else {
                DependencyStatus {
                    name: script.to_string(),
                    status: "missing".to_string(),
                    version: "".to_string(),
                }
            };

            statuses.push(status);
        }

        Ok(statuses)
    }
}

// Structures pour Google OAuth
#[derive(Debug, Deserialize)]
struct GoogleTokenInfo {
    aud: String,
    sub: String,
    email: String,
    email_verified: bool,
    name: String,
    picture: String,
    locale: String,
    exp: i64,
}

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub token_type: String,
    pub refresh_token: Option<String>,
    pub id_token: String,
}

#[derive(Debug)]
pub struct GoogleUserInfo {
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub locale: Option<String>,
}

// Structures pour SendGrid
#[derive(Debug, Serialize)]
struct SendGridEmail {
    personalizations: Vec<SendGridPersonalization>,
    from: SendGridEmailAddress,
    content: Vec<SendGridEmailContent>,
    subject: String,
}

#[derive(Debug, Serialize)]
struct SendGridPersonalization {
    to: Vec<SendGridEmailAddress>,
    subject: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendGridEmailAddress {
    email: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendGridEmailContent {
    #[serde(rename = "type")]
    type_field: String,
    value: String,
}

// Structures pour les dépendances
#[derive(Debug, Serialize)]
pub struct DependencyStatus {
    pub name: String,
    pub status: String,
    pub version: String,
}