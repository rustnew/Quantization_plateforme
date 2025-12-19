// services/cache.rs
use crate::utils::error::{AppError, Result};
use redis::{AsyncCommands, Client};
use std::sync::Arc;
use std::time::Duration;
use serde::{Serialize, de::DeserializeOwned};

pub struct Cache {
    client: Arc<Client>,
    prefix: String,
    default_ttl: Duration,
}

impl Cache {
    /// Créer un nouveau cache Redis
    pub async fn new(redis_url: &str, prefix: Option<&str>, default_ttl_seconds: u64) -> Result<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        // Tester la connexion
        let mut conn = client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let _: () = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(Self {
            client: Arc::new(client),
            prefix: prefix.unwrap_or("cache:").to_string(),
            default_ttl: Duration::from_secs(default_ttl_seconds),
        })
    }

    /// Stocker une valeur avec TTL
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set_ex(key, value, self.default_ttl.as_secs() as usize).await
    }

    /// Stocker une valeur avec TTL spécifique
    pub async fn set_ex<T: Serialize>(&self, key: &str, value: &T, ttl_seconds: usize) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let serialized = serde_json::to_string(value)
            .map_err(|e| AppError::SerializeError(e.to_string()))?;

        conn.set_ex(&full_key, serialized, ttl_seconds).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Récupérer une valeur
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let value: Option<String> = conn.get(&full_key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        match value {
            Some(json) => {
                let deserialized: T = serde_json::from_str(&json)
                    .map_err(|e| AppError::ParseError(e.to_string()))?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    /// Supprimer une clé
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let deleted: i64 = conn.del(&full_key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(deleted > 0)
    }

    /// Vérifier si une clé existe
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let exists: bool = conn.exists(&full_key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(exists)
    }

    /// Incrémenter une valeur
    pub async fn incr(&self, key: &str, by: i64) -> Result<i64> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let value: i64 = conn.incr(&full_key, by).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(value)
    }

    /// Décrémenter une valeur
    pub async fn decr(&self, key: &str, by: i64) -> Result<i64> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let value: i64 = conn.decr(&full_key, by).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(value)
    }

    /// Obtenir le TTL restant
    pub async fn ttl(&self, key: &str) -> Result<Option<Duration>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let ttl_seconds: i64 = conn.ttl(&full_key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        if ttl_seconds > 0 {
            Ok(Some(Duration::from_secs(ttl_seconds as u64)))
        } else if ttl_seconds == -1 {
            // Pas d'expiration
            Ok(None)
        } else {
            // Clé expirée ou inexistante
            Ok(None)
        }
    }

    /// Mettre à jour le TTL
    pub async fn expire(&self, key: &str, ttl_seconds: usize) -> Result<bool> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let success: bool = conn.expire(&full_key, ttl_seconds).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(success)
    }

    /// Stocker dans un hash
    pub async fn hset<T: Serialize>(&self, key: &str, field: &str, value: &T) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let serialized = serde_json::to_string(value)
            .map_err(|e| AppError::SerializeError(e.to_string()))?;

        conn.hset(&full_key, field, serialized).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Récupérer depuis un hash
    pub async fn hget<T: DeserializeOwned>(&self, key: &str, field: &str) -> Result<Option<T>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let value: Option<String> = conn.hget(&full_key, field).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        match value {
            Some(json) => {
                let deserialized: T = serde_json::from_str(&json)
                    .map_err(|e| AppError::ParseError(e.to_string()))?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    /// Supprimer un champ d'un hash
    pub async fn hdel(&self, key: &str, field: &str) -> Result<bool> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let deleted: i64 = conn.hdel(&full_key, field).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(deleted > 0)
    }

    /// Obtenir tous les champs d'un hash
    pub async fn hgetall<T: DeserializeOwned>(&self, key: &str) -> Result<Vec<T>> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_key = self.key(key);
        let values: Vec<String> = conn.hvals(&full_key).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let mut result = Vec::new();
        for json in values {
            if let Ok(deserialized) = serde_json::from_str::<T>(&json) {
                result.push(deserialized);
            }
        }

        Ok(result)
    }

    /// Nettoyer le cache par pattern
    pub async fn clear_pattern(&self, pattern: &str) -> Result<u64> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let full_pattern = self.key(pattern);
        let keys: Vec<String> = conn.keys(&full_pattern).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        if keys.is_empty() {
            return Ok(0);
        }

        let deleted: i64 = conn.del(keys).await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(deleted as u64)
    }

    /// Obtenir des statistiques du cache
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let info: String = redis::cmd("INFO")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let lines = info.lines();
        let mut stats = CacheStats::default();

        for line in lines {
            if line.starts_with("used_memory:") {
                if let Some(value) = line.split(':').nth(1) {
                    stats.used_memory_bytes = value.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("used_memory_peak:") {
                if let Some(value) = line.split(':').nth(1) {
                    stats.peak_memory_bytes = value.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("total_commands_processed:") {
                if let Some(value) = line.split(':').nth(1) {
                    stats.total_commands = value.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("keyspace_hits:") {
                if let Some(value) = line.split(':').nth(1) {
                    stats.hits = value.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("keyspace_misses:") {
                if let Some(value) = line.split(':').nth(1) {
                    stats.misses = value.trim().parse().unwrap_or(0);
                }
            }
        }

        if stats.hits + stats.misses > 0 {
            stats.hit_rate = stats.hits as f64 / (stats.hits + stats.misses) as f64;
        }

        Ok(stats)
    }

    /// Vérifier la santé du cache
    pub async fn health_check(&self) -> Result<()> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        let _: () = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::RedisError(e.to_string()))?;

        Ok(())
    }

    /// Helper pour ajouter le préfixe aux clés
    fn key(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            prefix: self.prefix.clone(),
            default_ttl: self.default_ttl,
        }
    }
}

/// Statistiques du cache
#[derive(Debug, Default, Serialize)]
pub struct CacheStats {
    pub used_memory_bytes: u64,
    pub peak_memory_bytes: u64,
    pub total_commands: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}