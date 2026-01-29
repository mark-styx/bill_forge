//! Password hashing and verification

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use billforge_core::{Error, Result};

/// Service for password hashing and verification
pub struct PasswordService {
    argon2: Argon2<'static>,
}

impl PasswordService {
    pub fn new() -> Self {
        Self {
            argon2: Argon2::default(),
        }
    }

    /// Hash a password
    pub fn hash(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| Error::Internal(format!("Failed to hash password: {}", e)))?;
        Ok(hash.to_string())
    }

    /// Verify a password against a hash
    pub fn verify(&self, password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| Error::Internal(format!("Invalid password hash: {}", e)))?;
        
        Ok(self
            .argon2
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Check if a password meets minimum requirements
    pub fn validate_password_strength(&self, password: &str) -> Result<()> {
        if password.len() < 8 {
            return Err(Error::Validation(
                "Password must be at least 8 characters".to_string(),
            ));
        }

        let has_uppercase = password.chars().any(|c| c.is_uppercase());
        let has_lowercase = password.chars().any(|c| c.is_lowercase());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());

        if !has_uppercase || !has_lowercase || !has_digit {
            return Err(Error::Validation(
                "Password must contain uppercase, lowercase, and numeric characters".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let service = PasswordService::new();
        let password = "TestPassword123!";
        
        let hash = service.hash(password).unwrap();
        assert!(service.verify(password, &hash).unwrap());
        assert!(!service.verify("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_password_validation() {
        let service = PasswordService::new();
        
        assert!(service.validate_password_strength("Short1").is_err());
        assert!(service.validate_password_strength("alllowercase1").is_err());
        assert!(service.validate_password_strength("ALLUPPERCASE1").is_err());
        assert!(service.validate_password_strength("NoNumbers").is_err());
        assert!(service.validate_password_strength("ValidPass123").is_ok());
    }
}
