use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default tenant ID when multi-tenancy is disabled or tenant_id is not provided
pub const DEFAULT_TENANT_ID: &str = "default";

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
    /// Tenant ID for multi-tenancy support
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Additional custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Claims {
    pub fn user_id(&self) -> &str {
        &self.sub
    }

    /// Get the tenant ID, or default if not set
    pub fn tenant_id(&self) -> &str {
        self.tenant_id.as_deref().unwrap_or(DEFAULT_TENANT_ID)
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.exp < now
    }
}
