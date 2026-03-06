// src/data/handler.rs
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Invoice {
    pub id: u64,
    pub amount: f64,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invoice_serialization() {
        let invoice = Invoice {
            id: 1,
            amount: 250.0,
            description: "Sample Invoice".to_string(),
        };

        let serialized = serde_json::to_string(&invoice).unwrap();
        assert_eq!(serialized, r#"{"id":1,"amount":250.0,"description":"Sample Invoice"}"#);
    }

    #[test]
    fn test_invoice_deserialization() {
        let json_str = r#"{"id":1,"amount":250.0,"description":"Sample Invoice"}"#;
        let invoice: Invoice = serde_json::from_str(json_str).unwrap();
        assert_eq!(invoice.id, 1);
        assert_eq!(invoice.amount, 250.0);
        assert_eq!(invoice.description, "Sample Invoice".to_string());
    }
}