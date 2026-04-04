//! Maps EDI document types to BillForge domain models

use crate::types::{
    EdiInvoice, EdiPurchaseOrder, EdiShipNotice,
    EdiRemittanceAdvice, EdiRemittanceDetail, EdiFunctionalAck, EdiParty,
    AckStatus,
};
use anyhow::Result;
use billforge_core::domain::{
    CreateInvoiceInput, CreateLineItemInput, CreatePOLineItemInput, CreatePurchaseOrderInput,
    Invoice, ReceivingLineItem,
};
use billforge_core::Money;
use chrono::Utc;
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

    /// Map an EDI 850 Purchase Order to a BillForge CreatePurchaseOrderInput
    pub fn purchase_order_from_edi(
        edi: &EdiPurchaseOrder,
        vendor_id: Uuid,
    ) -> Result<CreatePurchaseOrderInput> {
        let line_items: Vec<CreatePOLineItemInput> = edi
            .line_items
            .iter()
            .map(|item| CreatePOLineItemInput {
                line_number: Some(item.line_number),
                description: item.description.clone(),
                quantity: item.quantity,
                unit_of_measure: item.unit_of_measure.clone(),
                unit_price: Money {
                    amount: item.unit_price_cents,
                    currency: edi.currency.clone(),
                },
                total: Money {
                    amount: item.total_cents,
                    currency: edi.currency.clone(),
                },
                product_id: item.product_id.clone(),
            })
            .collect();

        let vendor_name = edi
            .vendor
            .as_ref()
            .map(|v| v.name.clone())
            .unwrap_or_else(|| edi.sender_id.clone());

        Ok(CreatePurchaseOrderInput {
            po_number: edi.po_number.clone(),
            vendor_id,
            vendor_name,
            order_date: edi.po_date,
            expected_delivery: edi.expected_delivery,
            line_items,
            total_amount: Money {
                amount: edi.total_amount_cents,
                currency: edi.currency.clone(),
            },
            ship_to_address: edi.ship_to.as_ref().map(|st| {
                format!(
                    "{}, {}, {} {}",
                    st.address_line1.as_deref().unwrap_or(""),
                    st.city.as_deref().unwrap_or(""),
                    st.state.as_deref().unwrap_or(""),
                    st.postal_code.as_deref().unwrap_or("")
                )
            }),
            notes: edi.notes.clone(),
        })
    }

    /// Map an EDI 856 Ship Notice to receiving line items for PO matching
    pub fn receiving_lines_from_asn(
        edi: &EdiShipNotice,
    ) -> Vec<ReceivingLineItem> {
        edi.line_items
            .iter()
            .map(|item| ReceivingLineItem {
                id: Uuid::new_v4(),
                po_line_number: item.po_line_number,
                quantity_received: item.quantity_shipped,
                quantity_damaged: 0.0,
                product_id: item.product_id.clone(),
            })
            .collect()
    }

    /// Generate an 820 Payment Remittance Advice from a paid invoice.
    ///
    /// `sender_id` / `receiver_id` are the ISA identifiers for our company
    /// and the trading partner respectively. `payment_reference` is the
    /// check/ACH/wire reference number. `payment_method` is CHK/ACH/WIR.
    pub fn remittance_from_invoice(
        invoice: &Invoice,
        sender_id: &str,
        receiver_id: &str,
        payment_reference: &str,
        payment_method: &str,
        payer_name: &str,
    ) -> EdiRemittanceAdvice {
        // Use UUID-derived values for uniqueness (X12 ICN is 9 digits, GCN is 1-9 digits)
        let unique = Uuid::new_v4();
        let bytes = unique.as_bytes();
        let icn = format!(
            "{:09}",
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000_000
        );
        let gcn = format!(
            "{:09}",
            u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) % 1_000_000_000
        );

        EdiRemittanceAdvice {
            sender_id: sender_id.to_string(),
            receiver_id: receiver_id.to_string(),
            interchange_control: icn,
            group_control: gcn,
            payment_reference: payment_reference.to_string(),
            payment_date: Utc::now().date_naive(),
            payment_method: payment_method.to_string(),
            total_amount_cents: invoice.total_amount.amount,
            currency: invoice.currency.clone(),
            payer: EdiParty {
                name: payer_name.to_string(),
                id_qualifier: Some("ZZ".to_string()),
                id_code: Some(sender_id.to_string()),
                address_line1: None,
                address_line2: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
            },
            payee: EdiParty {
                name: invoice.vendor_name.clone(),
                id_qualifier: Some("ZZ".to_string()),
                id_code: Some(receiver_id.to_string()),
                address_line1: None,
                address_line2: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
            },
            invoice_references: vec![EdiRemittanceDetail {
                invoice_number: invoice.invoice_number.clone(),
                invoice_date: invoice.invoice_date,
                gross_amount_cents: invoice.total_amount.amount,
                discount_cents: 0,
                net_amount_cents: invoice.total_amount.amount,
                po_number: invoice.po_number.clone(),
            }],
        }
    }

    /// Generate a 997 Functional Acknowledgment for a received inbound document.
    ///
    /// `group_control` is the GS06 from the received document's envelope.
    /// `transaction_control` is the ST02 if available.
    pub fn ack_for_document(
        group_control: &str,
        transaction_control: Option<&str>,
        sender_id: &str,
        receiver_id: &str,
        accepted: bool,
    ) -> EdiFunctionalAck {
        EdiFunctionalAck {
            sender_id: sender_id.to_string(),
            receiver_id: receiver_id.to_string(),
            group_control: group_control.to_string(),
            transaction_control: transaction_control.map(|s| s.to_string()),
            status: if accepted {
                AckStatus::Accepted
            } else {
                AckStatus::Rejected
            },
            errors: vec![],
        }
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

    #[test]
    fn test_map_edi_purchase_order() {
        use crate::types::{EdiPOLineItem, EdiPurchaseOrder};

        let edi_po = EdiPurchaseOrder {
            sender_id: "BUYER-001".to_string(),
            receiver_id: "BILLFORGE-001".to_string(),
            interchange_control: "000054321".to_string(),
            group_control: Some("5678".to_string()),
            po_number: "PO-2026-100".to_string(),
            po_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            expected_delivery: Some(NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()),
            buyer: EdiParty {
                name: "BillForge Inc".to_string(),
                id_qualifier: None,
                id_code: None,
                address_line1: None,
                address_line2: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
            },
            ship_to: Some(EdiParty {
                name: "Warehouse A".to_string(),
                id_qualifier: None,
                id_code: None,
                address_line1: Some("456 Dock Rd".to_string()),
                address_line2: None,
                city: Some("Chicago".to_string()),
                state: Some("IL".to_string()),
                postal_code: Some("60601".to_string()),
                country: Some("US".to_string()),
            }),
            vendor: Some(EdiParty {
                name: "Acme Corporation".to_string(),
                id_qualifier: Some("ZZ".to_string()),
                id_code: Some("ACME-001".to_string()),
                address_line1: None,
                address_line2: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
            }),
            line_items: vec![
                EdiPOLineItem {
                    line_number: 1,
                    quantity: 200.0,
                    unit_of_measure: "EA".to_string(),
                    unit_price_cents: 1500,
                    product_id_qualifier: Some("VP".to_string()),
                    product_id: Some("WIDGET-A".to_string()),
                    description: "Widget Type A".to_string(),
                    total_cents: 300000,
                },
            ],
            total_amount_cents: 300000,
            currency: "USD".to_string(),
            terms: None,
            shipping_instructions: None,
            notes: Some("Rush order".to_string()),
        };

        let vendor_id = Uuid::new_v4();
        let input = EdiMapper::purchase_order_from_edi(&edi_po, vendor_id).unwrap();

        assert_eq!(input.po_number, "PO-2026-100");
        assert_eq!(input.vendor_id, vendor_id);
        assert_eq!(input.vendor_name, "Acme Corporation");
        assert_eq!(input.total_amount.amount, 300000);
        assert_eq!(input.line_items.len(), 1);
        assert_eq!(input.line_items[0].quantity, 200.0);
        assert_eq!(input.line_items[0].product_id, Some("WIDGET-A".to_string()));
        assert!(input.ship_to_address.is_some());
        assert_eq!(input.notes, Some("Rush order".to_string()));
    }

    #[test]
    fn test_map_receiving_lines_from_asn() {
        use crate::types::{EdiShipLineItem, EdiShipNotice};

        let asn = EdiShipNotice {
            sender_id: "ACME-001".to_string(),
            receiver_id: "BILLFORGE-001".to_string(),
            interchange_control: "000099999".to_string(),
            shipment_id: "SHP-001".to_string(),
            ship_date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            expected_delivery: Some(NaiveDate::from_ymd_opt(2026, 4, 12).unwrap()),
            po_number: "PO-2026-100".to_string(),
            carrier: Some("FedEx".to_string()),
            tracking_number: Some("1234567890".to_string()),
            ship_from: None,
            ship_to: None,
            line_items: vec![
                EdiShipLineItem {
                    po_line_number: 1,
                    quantity_shipped: 200.0,
                    unit_of_measure: "EA".to_string(),
                    product_id: Some("WIDGET-A".to_string()),
                    description: "Widget Type A".to_string(),
                },
                EdiShipLineItem {
                    po_line_number: 2,
                    quantity_shipped: 45.0,
                    unit_of_measure: "EA".to_string(),
                    product_id: Some("GADGET-B".to_string()),
                    description: "Gadget Type B".to_string(),
                },
            ],
        };

        let lines = EdiMapper::receiving_lines_from_asn(&asn);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].po_line_number, 1);
        assert_eq!(lines[0].quantity_received, 200.0);
        assert_eq!(lines[1].po_line_number, 2);
        assert_eq!(lines[1].quantity_received, 45.0);
        assert_eq!(lines[1].product_id, Some("GADGET-B".to_string()));
    }

    #[test]
    fn test_remittance_from_invoice() {
        use billforge_core::domain::{
            CaptureStatus, Invoice, InvoiceId, ProcessingStatus,
        };
        use billforge_core::types::{TenantId, UserId};
        use billforge_core::Money;

        let invoice = Invoice {
            id: InvoiceId(Uuid::new_v4()),
            tenant_id: TenantId::from_uuid(Uuid::new_v4()),
            vendor_id: Some(Uuid::new_v4()),
            vendor_name: "Acme Corporation".to_string(),
            invoice_number: "INV-2026-001".to_string(),
            invoice_date: Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            po_number: Some("PO-5678".to_string()),
            subtotal: None,
            tax_amount: None,
            total_amount: Money::new(275000, "USD"),
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Paid,
            current_queue_id: None,
            assigned_to: None,
            supporting_documents: vec![],
            ocr_confidence: Some(1.0),
            categorization_confidence: None,
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: serde_json::json!({}),
            document_id: Uuid::new_v4(),
            created_by: UserId::from_uuid(Uuid::new_v4()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let remittance = EdiMapper::remittance_from_invoice(
            &invoice,
            "BILLFORGE-001",
            "ACME-001",
            "CHK-123456",
            "CHK",
            "BillForge Inc",
        );

        assert_eq!(remittance.sender_id, "BILLFORGE-001");
        assert_eq!(remittance.receiver_id, "ACME-001");
        assert_eq!(remittance.payment_reference, "CHK-123456");
        assert_eq!(remittance.payment_method, "CHK");
        assert_eq!(remittance.total_amount_cents, 275000);
        assert_eq!(remittance.currency, "USD");
        assert_eq!(remittance.payer.name, "BillForge Inc");
        assert_eq!(remittance.payee.name, "Acme Corporation");
        assert_eq!(remittance.invoice_references.len(), 1);
        assert_eq!(
            remittance.invoice_references[0].invoice_number,
            "INV-2026-001"
        );
        assert_eq!(
            remittance.invoice_references[0].po_number,
            Some("PO-5678".to_string())
        );
    }

    #[test]
    fn test_ack_for_document_accepted() {
        let ack = EdiMapper::ack_for_document(
            "12345", Some("0001"), "BILLFORGE-001", "ACME-001", true,
        );
        assert_eq!(ack.group_control, "12345");
        assert_eq!(ack.transaction_control, Some("0001".to_string()));
        assert_eq!(ack.sender_id, "BILLFORGE-001");
        assert_eq!(ack.receiver_id, "ACME-001");
        assert_eq!(ack.status, AckStatus::Accepted);
        assert!(ack.errors.is_empty());
    }

    #[test]
    fn test_ack_for_document_rejected() {
        let ack = EdiMapper::ack_for_document(
            "99999", None, "BILLFORGE-001", "ACME-001", false,
        );
        assert_eq!(ack.group_control, "99999");
        assert_eq!(ack.transaction_control, None);
        assert_eq!(ack.status, AckStatus::Rejected);
    }
}
