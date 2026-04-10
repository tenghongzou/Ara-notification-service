use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default tenant ID when multi-tenancy is disabled or tenant_id is not provided
pub const DEFAULT_TENANT_ID: &str = "default";

/// Build a queue key that includes tenant scope.
/// Non-default tenants get `{tenant_id}:{user_id}`, default tenant gets `{user_id}`.
pub fn tenant_scoped_key(tenant_id: &str, user_id: &str) -> String {
    if tenant_id == DEFAULT_TENANT_ID {
        user_id.to_string()
    } else {
        format!("{}:{}", tenant_id, user_id)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_scoped_key_default_tenant() {
        assert_eq!(tenant_scoped_key("default", "user-123"), "user-123");
    }

    #[test]
    fn test_tenant_scoped_key_custom_tenant() {
        assert_eq!(
            tenant_scoped_key("acme-corp", "user-123"),
            "acme-corp:user-123"
        );
    }

    #[test]
    fn test_claims_tenant_id_default() {
        let claims = Claims {
            sub: "user1".to_string(),
            exp: i64::MAX,
            iat: 0,
            roles: vec![],
            tenant_id: None,
            extra: std::collections::HashMap::new(),
        };
        assert_eq!(claims.tenant_id(), "default");
    }

    #[test]
    fn test_claims_tenant_id_custom() {
        let claims = Claims {
            sub: "user1".to_string(),
            exp: i64::MAX,
            iat: 0,
            roles: vec![],
            tenant_id: Some("acme".to_string()),
            extra: std::collections::HashMap::new(),
        };
        assert_eq!(claims.tenant_id(), "acme");
    }
}
