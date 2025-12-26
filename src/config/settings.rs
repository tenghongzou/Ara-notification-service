use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub redis: RedisConfig,
    pub api: ApiConfig,
    #[serde(default)]
    pub websocket: WebSocketConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSocketConfig {
    /// Heartbeat interval in seconds (server sends ping)
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    /// Connection timeout in seconds (disconnect if no activity)
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// Cleanup task interval in seconds
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval: u64,
}

fn default_heartbeat_interval() -> u64 {
    30 // 30 seconds
}

fn default_connection_timeout() -> u64 {
    120 // 2 minutes
}

fn default_cleanup_interval() -> u64 {
    60 // 1 minute
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default)]
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub key: Option<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8081
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        // Load .env file if exists
        let _ = dotenvy::dotenv();

        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let builder = Config::builder()
            // Start with default values
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8081)?
            .set_default("redis.url", "redis://localhost:6379")?
            .set_default("websocket.heartbeat_interval", 30)?
            .set_default("websocket.connection_timeout", 120)?
            .set_default("websocket.cleanup_interval", 60)?
            // Load config file if exists
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            // Load from environment variables
            // SERVER_HOST, SERVER_PORT, JWT_SECRET, REDIS_URL, etc.
            .add_source(
                Environment::default()
                    .separator("_")
                    .try_parsing(true)
                    .list_separator(","),
            );

        builder.build()?.try_deserialize()
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors_origins: vec![],
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            channels: vec![],
        }
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: default_heartbeat_interval(),
            connection_timeout: default_connection_timeout(),
            cleanup_interval: default_cleanup_interval(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let server = ServerConfig::default();
        assert_eq!(server.host, "0.0.0.0");
        assert_eq!(server.port, 8081);
    }
}
