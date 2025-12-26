use jsonwebtoken::{decode, DecodingKey, Validation};

use crate::config::JwtConfig;
use crate::error::AppError;

use super::Claims;

pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtValidator {
    pub fn new(config: &JwtConfig) -> Self {
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());

        let mut validation = Validation::default();

        if let Some(ref issuer) = config.issuer {
            validation.set_issuer(&[issuer]);
        }

        if let Some(ref audience) = config.audience {
            validation.set_audience(&[audience]);
        }

        Self {
            decoding_key,
            validation,
        }
    }

    pub fn validate(&self, token: &str) -> Result<Claims, AppError> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| AppError::Auth(format!("Invalid token: {}", e)))?;

        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key-for-testing".to_string(),
            issuer: None,
            audience: None,
        }
    }

    fn create_test_token(claims: &Claims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_valid_token() {
        let config = create_test_config();
        let validator = JwtValidator::new(&config);

        let claims = Claims {
            sub: "user-123".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            roles: vec!["user".to_string()],
            extra: Default::default(),
        };

        let token = create_test_token(&claims, &config.secret);
        let result = validator.validate(&token);

        assert!(result.is_ok());
        let validated_claims = result.unwrap();
        assert_eq!(validated_claims.sub, "user-123");
    }

    #[test]
    fn test_invalid_token() {
        let config = create_test_config();
        let validator = JwtValidator::new(&config);

        let result = validator.validate("invalid-token");
        assert!(result.is_err());
    }
}
