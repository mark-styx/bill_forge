//! Request validation utilities

use crate::error::ValidationError;
use std::collections::HashMap;

/// Validation builder for request validation
pub struct Validator {
    errors: HashMap<String, Vec<String>>,
}

impl Validator {
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }

    pub fn add_error(&mut self, field: &str, message: impl Into<String>) {
        self.errors
            .entry(field.to_string())
            .or_default()
            .push(message.into());
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn result(self) -> Result<(), ValidationError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError {
                message: String::from("Validation failed"),
                field_errors: self.errors,
            })
        }
    }

    pub fn email(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "Email is required");
        } else if !value.contains('@') || !value.contains('.') {
            self.add_error(field, "Invalid email format");
        }
        self
    }

    pub fn uuid(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "UUID is required");
        } else if value.len() != 36 {
            self.add_error(field, "Invalid UUID format");
        }
        self
    }

    pub fn required_string(
        &mut self,
        field: &str,
        value: &str,
        min_len: usize,
        max_len: usize,
    ) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "Field is required");
        } else if value.len() < min_len {
            self.add_error(field, "Value is too short");
        } else if value.len() > max_len {
            self.add_error(field, "Value is too long");
        }
        self
    }

    pub fn optional_string(
        &mut self,
        field: &str,
        value: Option<&str>,
        max_len: usize,
    ) -> &mut Self {
        if let Some(v) = value {
            if v.len() > max_len {
                self.add_error(field, "Value is too long");
            }
        }
        self
    }

    pub fn password(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "Password is required");
        } else if value.len() < 8 {
            self.add_error(field, "Password must be at least 8 characters");
        }
        self
    }

    pub fn money_cents(&mut self, field: &str, value: i64) -> &mut Self {
        if value < 0 {
            self.add_error(field, "Amount cannot be negative");
        }
        self
    }

    pub fn invoice_number(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "Invoice number is required");
        }
        self
    }

    pub fn date(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.add_error(field, "Date is required");
        }
        self
    }

    pub fn optional_date(&mut self, _field: &str, _value: Option<&str>) -> &mut Self {
        self
    }

    pub fn safe_string(&mut self, _field: &str, _value: &str) -> &mut Self {
        self
    }

    pub fn positive_int(&mut self, field: &str, value: i32) -> &mut Self {
        if value <= 0 {
            self.add_error(field, "Value must be positive");
        }
        self
    }

    pub fn non_negative_int(&mut self, field: &str, value: i32) -> &mut Self {
        if value < 0 {
            self.add_error(field, "Value cannot be negative");
        }
        self
    }

    pub fn one_of(&mut self, field: &str, value: &str, allowed: &[&str]) -> &mut Self {
        if !allowed.contains(&value) {
            self.add_error(field, "Invalid value");
        }
        self
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn sanitize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

pub fn sanitize_and_trim(input: &str) -> String {
    sanitize_string(input.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_inputs_reject_invalid_email_and_short_password() {
        let mut validator = Validator::new();
        validator
            .email("email", "admin-at-example")
            .password("password", "short");
        let result = validator.result();

        let err = result.expect_err("invalid auth fields should fail validation");
        assert!(err.field_errors.contains_key("email"));
        assert!(err.field_errors.contains_key("password"));
    }

    #[test]
    fn invoice_and_queue_inputs_reject_bad_numeric_and_enum_values() {
        let mut validator = Validator::new();
        validator
            .money_cents("total_amount", -1)
            .positive_int("sla_hours", 0)
            .one_of(
                "queue_type",
                "not-a-queue",
                &["review", "approval", "payment"],
            );
        let result = validator.result();

        let err = result.expect_err("bad invoice and queue fields should fail validation");
        assert!(err.field_errors.contains_key("total_amount"));
        assert!(err.field_errors.contains_key("sla_hours"));
        assert!(err.field_errors.contains_key("queue_type"));
    }

    #[test]
    fn integration_secret_sanitizer_strips_shell_and_markup_punctuation() {
        let sanitized = sanitize_and_trim("  token; rm -rf / <script>alert(1)</script>  ");

        assert_eq!(sanitized, "token rm rf scriptalert1script");
    }

    #[test]
    fn uuid_validation_rejects_non_uuid_queue_ids() {
        let mut validator = Validator::new();
        validator.uuid("queue_id", "not-a-uuid");
        let result = validator.result();

        let err = result.expect_err("invalid queue id should fail validation");
        assert!(err.field_errors.contains_key("queue_id"));
    }
}
