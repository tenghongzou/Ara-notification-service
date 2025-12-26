use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// User roles
    #[serde(default)]
    pub roles: Vec<String>,
    /// Additional custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Claims {
    pub fn user_id(&self) -> &str {
        &self.sub
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.exp < now
    }
}
