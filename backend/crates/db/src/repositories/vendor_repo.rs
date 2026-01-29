//! Vendor repository implementation

use crate::manager::DatabaseManager;
use async_trait::async_trait;
use billforge_core::{
    domain::*,
    traits::VendorRepository,
    types::*,
    Error, Result,
};
use chrono::Utc;
use rusqlite::params;
use std::sync::Arc;
use uuid::Uuid;

pub struct VendorRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl VendorRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }
}

#[async_trait]
impl VendorRepository for VendorRepositoryImpl {
    async fn create(&self, tenant_id: &TenantId, input: CreateVendorInput) -> Result<Vendor> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = VendorId::new();
        let now = Utc::now();

        conn.execute(
            r#"INSERT INTO vendors (
                id, name, legal_name, vendor_type, status, email, phone, website,
                address_line1, address_line2, address_city, address_state,
                address_postal_code, address_country, tax_id, tax_id_type,
                payment_terms, default_payment_method, vendor_code,
                default_gl_code, default_department, notes, tags,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                id.0.to_string(),
                input.name,
                input.legal_name,
                format!("{:?}", input.vendor_type).to_lowercase(),
                "active",
                input.email,
                input.phone,
                input.website,
                input.address.as_ref().map(|a| a.line1.clone()),
                input.address.as_ref().and_then(|a| a.line2.clone()),
                input.address.as_ref().map(|a| a.city.clone()),
                input.address.as_ref().and_then(|a| a.state.clone()),
                input.address.as_ref().map(|a| a.postal_code.clone()),
                input.address.as_ref().map(|a| a.country.clone()),
                input.tax_id,
                input.tax_id_type.map(|t| format!("{:?}", t).to_lowercase()),
                input.payment_terms,
                input.default_payment_method.map(|p| format!("{:?}", p).to_lowercase()),
                input.vendor_code,
                input.default_gl_code,
                input.default_department,
                input.notes,
                serde_json::to_string(&input.tags).unwrap(),
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to create vendor: {}", e)))?;

        Ok(Vendor {
            id,
            tenant_id: tenant_id.clone(),
            name: input.name,
            legal_name: input.legal_name,
            vendor_type: input.vendor_type,
            status: VendorStatus::Active,
            email: input.email,
            phone: input.phone,
            website: input.website,
            address: input.address,
            tax_id: input.tax_id,
            tax_id_type: input.tax_id_type,
            w9_on_file: false,
            w9_received_date: None,
            payment_terms: input.payment_terms,
            default_payment_method: input.default_payment_method,
            bank_account: None,
            vendor_code: input.vendor_code,
            default_gl_code: input.default_gl_code,
            default_department: input.default_department,
            primary_contact: None,
            contacts: Vec::new(),
            notes: input.notes,
            tags: input.tags,
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: &VendorId) -> Result<Option<Vendor>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, legal_name, vendor_type, status, email, phone, website,
                          address_line1, address_line2, address_city, address_state,
                          address_postal_code, address_country, tax_id, tax_id_type,
                          w9_on_file, w9_received_date, payment_terms, default_payment_method,
                          vendor_code, default_gl_code, default_department, notes, tags,
                          created_at, updated_at
                   FROM vendors WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let vendor = stmt
            .query_row(params![id.0.to_string()], |row| {
                Ok(self.map_vendor_row(row, tenant_id.clone()))
            })
            .ok();

        match vendor {
            Some(v) => Ok(Some(v?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filters: &VendorFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<Vendor>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Build query with filters
        let mut conditions = Vec::new();

        if let Some(status) = &filters.status {
            conditions.push(format!(
                "status = '{}'",
                format!("{:?}", status).to_lowercase()
            ));
        }

        if let Some(vendor_type) = &filters.vendor_type {
            conditions.push(format!(
                "vendor_type = '{}'",
                format!("{:?}", vendor_type).to_lowercase()
            ));
        }

        if let Some(search) = &filters.search {
            conditions.push(format!(
                "(name LIKE '%{}%' OR legal_name LIKE '%{}%' OR email LIKE '%{}%')",
                search, search, search
            ));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Get total count
        let count_sql = format!("SELECT COUNT(*) FROM vendors {}", where_clause);
        let total_items: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .map_err(|e| Error::Database(format!("Failed to count vendors: {}", e)))?;

        // Get paginated results
        let query_sql = format!(
            r#"SELECT id, name, legal_name, vendor_type, status, email, phone, website,
                      address_line1, address_line2, address_city, address_state,
                      address_postal_code, address_country, tax_id, tax_id_type,
                      w9_on_file, w9_received_date, payment_terms, default_payment_method,
                      vendor_code, default_gl_code, default_department, notes, tags,
                      created_at, updated_at
               FROM vendors {}
               ORDER BY name ASC
               LIMIT {} OFFSET {}"#,
            where_clause,
            pagination.per_page,
            pagination.offset()
        );

        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let vendors = stmt
            .query_map([], |row| Ok(self.map_vendor_row(row, tenant_clone.clone())))
            .map_err(|e| Error::Database(format!("Failed to list vendors: {}", e)))?;

        let mut results = Vec::new();
        for vendor in vendors {
            results.push(vendor.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(PaginatedResponse {
            data: results,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total_items as u64,
                total_pages: ((total_items as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &VendorId,
        input: UpdateVendorInput,
    ) -> Result<Vendor> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut set_clauses = vec!["updated_at = ?".to_string()];
        let mut params_vec: Vec<String> = vec![Utc::now().to_rfc3339()];

        if let Some(name) = &input.name {
            set_clauses.push("name = ?".to_string());
            params_vec.push(name.clone());
        }

        if let Some(status) = &input.status {
            set_clauses.push("status = ?".to_string());
            params_vec.push(format!("{:?}", status).to_lowercase());
        }

        let sql = format!(
            "UPDATE vendors SET {} WHERE id = ?",
            set_clauses.join(", ")
        );

        params_vec.push(id.0.to_string());

        conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))
            .map_err(|e| Error::Database(format!("Failed to update vendor: {}", e)))?;

        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Vendor".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &VendorId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "DELETE FROM vendors WHERE id = ?",
            params![id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete vendor: {}", e)))?;

        Ok(())
    }

    async fn find_by_name(&self, tenant_id: &TenantId, name: &str) -> Result<Option<Vendor>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, legal_name, vendor_type, status, email, phone, website,
                          address_line1, address_line2, address_city, address_state,
                          address_postal_code, address_country, tax_id, tax_id_type,
                          w9_on_file, w9_received_date, payment_terms, default_payment_method,
                          vendor_code, default_gl_code, default_department, notes, tags,
                          created_at, updated_at
                   FROM vendors WHERE name LIKE ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let vendor = stmt
            .query_row(params![name], |row| {
                Ok(self.map_vendor_row(row, tenant_id.clone()))
            })
            .ok();

        match vendor {
            Some(v) => Ok(Some(v?)),
            None => Ok(None),
        }
    }

    async fn add_contact(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        contact: VendorContact,
    ) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            r#"INSERT INTO vendor_contacts (id, vendor_id, name, title, email, phone, is_primary)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
            params![
                contact.id.to_string(),
                vendor_id.0.to_string(),
                contact.name,
                contact.title,
                contact.email,
                contact.phone,
                contact.is_primary,
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to add contact: {}", e)))?;

        Ok(())
    }

    async fn remove_contact(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        contact_id: Uuid,
    ) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "DELETE FROM vendor_contacts WHERE id = ? AND vendor_id = ?",
            params![contact_id.to_string(), vendor_id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to remove contact: {}", e)))?;

        Ok(())
    }
}

impl VendorRepositoryImpl {
    fn map_vendor_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<Vendor> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;

        Ok(Vendor {
            id: VendorId(Uuid::parse_str(&id_str).unwrap()),
            tenant_id,
            name: row.get(1).map_err(|e| Error::Database(e.to_string()))?,
            legal_name: row.get(2).map_err(|e| Error::Database(e.to_string()))?,
            vendor_type: VendorType::Business,
            status: VendorStatus::Active,
            email: row.get(5).map_err(|e| Error::Database(e.to_string()))?,
            phone: row.get(6).map_err(|e| Error::Database(e.to_string()))?,
            website: row.get(7).map_err(|e| Error::Database(e.to_string()))?,
            address: None,
            tax_id: row.get(14).map_err(|e| Error::Database(e.to_string()))?,
            tax_id_type: None,
            w9_on_file: row.get(16).map_err(|e| Error::Database(e.to_string()))?,
            w9_received_date: None,
            payment_terms: row.get(18).map_err(|e| Error::Database(e.to_string()))?,
            default_payment_method: None,
            bank_account: None,
            vendor_code: row.get(20).map_err(|e| Error::Database(e.to_string()))?,
            default_gl_code: row.get(21).map_err(|e| Error::Database(e.to_string()))?,
            default_department: row.get(22).map_err(|e| Error::Database(e.to_string()))?,
            primary_contact: None,
            contacts: Vec::new(),
            notes: row.get(23).map_err(|e| Error::Database(e.to_string()))?,
            tags: Vec::new(),
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
}
