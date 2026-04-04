//! Maps EDI document types to BillForge domain models

use crate::types::EdiInvoice;
use anyhow::Result;
use billforge_core::domain::{CreateInvoiceInput, CreateLineItemInput};
use billforge_core::Money;
use uuid::Uuid;

/// Maps EDI documents to BillForge domain models
pub struct EdiMapper;

impl EdiMapper {
    /// Map an EDI 810 Invoice to a BillForge CreateInvoiceInput
    ///
    /// The invoice enters the pipeline as already-captured (structured data,
    /// no OCR needed) and goes straight to the approval workflow.
    pub fn invoice_from_edi(
        edi: &EdiInvoice,
        vendor_id: Option<Uuid>,
        document_id: Uuid,
    ) -> Result<CreateInvoiceInput> {
        let line_items: Vec<CreateLineItemInput> = edi
            .line_items
            .iter()
            .map(|item| CreateLineItemInput {
                description: item.description.clone(),
                quantity: Some(item.quantity),
                unit_price: Some(Money {
                    amount: item.unit_price_cents,
                    currency: edi.currency.clone(),
                }),
                amount: Money {
                    amount: item.total_cents,
                    currency: edi.currency.clone(),
                },
                gl_code: None,
                department: None,
                project: None,
            })
            .collect();

        Ok(CreateInvoiceInput {
            document_id,
            vendor_id,
            vendor_name: edi.vendor.name.clone(),
            invoice_number: edi.invoice_number.clone(),
            invoice_date: Some(edi.invoice_date),
            due_date: edi.due_date,
            po_number: edi.po_number.clone(),
            subtotal: None,
            tax_amount: edi.tax_amount_cents.map(|cents| Money {
                amount: cents,
                currency: edi.currency.clone(),
            }),
            total_amount: Money {
                amount: edi.total_amount_cents,
                currency: edi.currency.clone(),
            },
            currency: edi.currency.clone(),
            line_items,
            ocr_confidence: Some(1.0), // EDI data is fully structured
            department: None,
            gl_code: None,
            cost_center: None,
            notes: Some(format!(
                "Received via EDI (ICN: {})",
                edi.interchange_control
            )),
            tags: vec!["edi".to_string()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EdiLineItem, EdiParty};
    use chrono::NaiveDate;

    fn sample_edi_invoice() -> EdiInvoice {
        EdiInvoice {
            sender_id: "ACME-001".to_string(),
            receiver_id: "BILLFORGE-001".to_string(),
            interchange_control: "000012345".to_string(),
            group_control: Some("1234".to_string()),
            invoice_number: "INV-2026-001".to_string(),
            invoice_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            po_number: Some("PO-5678".to_string()),
            vendor: EdiParty {
                name: "Acme Corporation".to_string(),
                id_qualifier: Some("ZZ".to_string()),
                id_code: Some("ACME-001".to_string()),
                address_line1: Some("123 Main St".to_string()),
                address_line2: None,
                city: Some("Springfield".to_string()),
                state: Some("IL".to_string()),
                postal_code: Some("62701".to_string()),
                country: Some("US".to_string()),
            },
            bill_to: None,
            remit_to: None,
            ship_to: None,
            line_items: vec![
                EdiLineItem {
                    line_number: 1,
                    quantity: 100.0,
                    unit_of_measure: "EA".to_string(),
                    unit_price_cents: 1500,
                    product_id_qualifier: Some("VP".to_string()),
                    product_id: Some("WIDGET-A".to_string()),
                    description: "Widget Type A".to_string(),
                    total_cents: 150000,
                },
                EdiLineItem {
                    line_number: 2,
                    quantity: 50.0,
                    unit_of_measure: "EA".to_string(),
                    unit_price_cents: 2500,
                    product_id_qualifier: Some("VP".to_string()),
                    product_id: Some("GADGET-B".to_string()),
                    description: "Gadget Type B".to_string(),
                    total_cents: 125000,
                },
            ],
            total_amount_cents: 275000,
            currency: "USD".to_string(),
            terms: None,
            due_date: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            charges: vec![],
            tax_amount_cents: None,
        }
    }

    #[test]
    fn test_map_edi_invoice() {
        let edi = sample_edi_invoice();
        let doc_id = Uuid::new_v4();
        let input = EdiMapper::invoice_from_edi(&edi, None, doc_id).unwrap();

        assert_eq!(input.vendor_name, "Acme Corporation");
        assert_eq!(input.invoice_number, "INV-2026-001");
        assert_eq!(input.total_amount.amount, 275000);
        assert_eq!(input.line_items.len(), 2);
        assert_eq!(input.line_items[0].description, "Widget Type A");
        assert!(input.po_number.is_some());
        assert!(input.notes.as_ref().unwrap().contains("EDI"));
        assert_eq!(input.document_id, doc_id);
    }
}
