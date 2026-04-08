//! Sage Intacct API client
//!
//! Uses the XML Web Services API with session-based authentication.
//! Key operations:
//! - readByQuery: Read vendors, GL accounts, AP documents
//! - create: Create AP bills (purchasing transactions)
//! - readMore: Paginate through large result sets
//!
//! Reference: https://developer.intacct.com/api/

use crate::auth::SageIntacctSession;
use crate::types::*;
use anyhow::{Context, Result};
use billforge_core::http_retry::{self, RetryConfig};
use tokio::time::sleep;

/// Sage Intacct API client
pub struct SageIntacctClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Active session
    session: SageIntacctSession,
}

impl SageIntacctClient {
    /// Create a new Sage Intacct API client with an active session
    pub fn new(session: SageIntacctSession) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            session,
        }
    }

    /// Build the XML request envelope with session authentication
    fn build_request(&self, control_id: &str, function_xml: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<request>
  <control>
    <senderid>billforge</senderid>
    <password></password>
    <controlid>{control_id}</controlid>
    <uniqueid>false</uniqueid>
    <dtdversion>3.0</dtdversion>
    <includewhitespace>false</includewhitespace>
  </control>
  <operation>
    <authentication>
      <sessionid>{session_id}</sessionid>
    </authentication>
    <content>
      {function_xml}
    </content>
  </operation>
</request>"#,
            control_id = control_id,
            session_id = self.session.session_id,
            function_xml = function_xml,
        )
    }

    /// Send an XML request with retry logic for 429/5xx errors.
    async fn send_request(&self, request_xml: &str) -> Result<String> {
        let config = RetryConfig::default();
        let mut attempt = 0u32;
        let body_owned = request_xml.to_string();

        loop {
            let result = self
                .http_client
                .post(&self.session.endpoint)
                .header("Content-Type", "application/xml")
                .body(body_owned.clone())
                .send()
                .await;

            let response = match result {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt == 0 {
                        tracing::warn!(attempt, error = %err, "Sage Intacct transport error, retrying once");
                        attempt += 1;
                        continue;
                    }
                    anyhow::bail!("Sage Intacct transport error after retry: {}", err);
                }
            };

            let status = response.status();
            let status_code = status.as_u16();

            if status.is_success() {
                let body = response
                    .text()
                    .await
                    .context("Failed to read Sage Intacct response")?;
                return Ok(body);
            }

            if http_retry::is_retryable_status(status_code) {
                let retry_after = http_retry::parse_retry_after(
                    response.headers().get("Retry-After").and_then(|v| v.to_str().ok()),
                );
                attempt += 1;
                if attempt >= config.max_retries {
                    let body = response.text().await.unwrap_or_default();
                    anyhow::bail!("Sage Intacct API failed after {} retries ({}): {}", attempt, status_code, body);
                }
                let backoff = http_retry::compute_backoff(&config, attempt, retry_after);
                tracing::warn!(attempt, status_code, ?backoff, "Sage Intacct retryable error, backing off");
                sleep(backoff).await;
                continue;
            }

            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Sage Intacct API request failed (HTTP {}): {}", status, body);
        }
    }

    // ──────────────────────────── Vendor Operations ────────────────────────────

    /// Query vendors from Sage Intacct
    pub async fn query_vendors(
        &self,
        page_size: i32,
        filter: Option<&str>,
    ) -> Result<SageQueryResult<SageVendor>> {
        let query = filter.unwrap_or("STATUS = 'T'"); // T = Active in Sage Intacct
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readByQuery>
    <object>VENDOR</object>
    <fields>RECORDNO,VENDORID,NAME,STATUS,VENDTYPE,DISPLAYCONTACT.EMAIL1,DISPLAYCONTACT.PHONE1,DISPLAYCONTACT.CONTACTNAME,DEFAULT_LEAD_ACCT_NO,TAXID,FORM1099TYPE,PAYMENTTERM,CURRENCY</fields>
    <query>{query}</query>
    <pagesize>{page_size}</pagesize>
  </readByQuery>
</function>"#,
            control_id = control_id,
            query = query,
            page_size = page_size,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        parse_vendor_query_response(&response_text)
    }

    /// Read more results from a paginated query
    pub async fn read_more_vendors(&self, result_id: &str) -> Result<SageQueryResult<SageVendor>> {
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readMore>
    <resultId>{result_id}</resultId>
  </readMore>
</function>"#,
            control_id = control_id,
            result_id = result_id,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        parse_vendor_query_response(&response_text)
    }

    /// Get a single vendor by ID
    pub async fn get_vendor(&self, vendor_id: &str) -> Result<SageVendor> {
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <read>
    <object>VENDOR</object>
    <keys>{vendor_id}</keys>
    <fields>RECORDNO,VENDORID,NAME,STATUS,VENDTYPE,DISPLAYCONTACT.EMAIL1,DISPLAYCONTACT.PHONE1,DISPLAYCONTACT.CONTACTNAME,DEFAULT_LEAD_ACCT_NO,TAXID,FORM1099TYPE,PAYMENTTERM,CURRENCY</fields>
  </read>
</function>"#,
            control_id = control_id,
            vendor_id = vendor_id,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        let result = parse_vendor_query_response(&response_text)?;
        result
            .records
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Vendor {} not found", vendor_id))
    }

    // ──────────────────────────── GL Account Operations ────────────────────────────

    /// Query GL accounts from Sage Intacct
    pub async fn query_gl_accounts(
        &self,
        page_size: i32,
        filter: Option<&str>,
    ) -> Result<SageQueryResult<SageGLAccount>> {
        let query = filter.unwrap_or("STATUS = 'active'");
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readByQuery>
    <object>GLACCOUNT</object>
    <fields>ACCOUNTNO,TITLE,ACCOUNTTYPE,NORMALBALANCE,CATEGORY,STATUS,DEPARTMENTID,LOCATIONID</fields>
    <query>{query}</query>
    <pagesize>{page_size}</pagesize>
  </readByQuery>
</function>"#,
            control_id = control_id,
            query = query,
            page_size = page_size,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        parse_gl_account_query_response(&response_text)
    }

    // ──────────────────────────── AP Bill Operations ────────────────────────────

    /// Create an AP bill (vendor invoice) in Sage Intacct
    pub async fn create_ap_bill(&self, bill: &SageAPBill) -> Result<SageOperationResult> {
        let control_id = uuid::Uuid::new_v4().to_string();

        let mut lines_xml = String::new();
        for line in &bill.lines {
            lines_xml.push_str(&format!(
                r#"        <potransitem>
          <accountno>{gl_account_no}</accountno>
          <amount>{amount}</amount>
          {memo}
          {department}
          {location}
          {project}
        </potransitem>
"#,
                gl_account_no = line.gl_account_no,
                amount = line.amount,
                memo = line.memo.as_ref().map(|m| format!("<memo>{}</memo>", m)).unwrap_or_default(),
                department = line.department_id.as_ref().map(|d| format!("<departmentid>{}</departmentid>", d)).unwrap_or_default(),
                location = line.location_id.as_ref().map(|l| format!("<locationid>{}</locationid>", l)).unwrap_or_default(),
                project = line.project_id.as_ref().map(|p| format!("<projectid>{}</projectid>", p)).unwrap_or_default(),
            ));
        }

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <create_potransaction>
    <transactiontype>Vendor Invoice</transactiontype>
    <datecreated>
      <year>{year}</year>
      <month>{month}</month>
      <day>{day}</day>
    </datecreated>
    <vendorid>{vendor_id}</vendorid>
    {doc_number}
    {ref_number}
    {description}
    {due_date}
    {location}
    {department}
    <potransitems>
{lines_xml}    </potransitems>
  </create_potransaction>
</function>"#,
            control_id = control_id,
            year = bill.date_created.format("%Y"),
            month = bill.date_created.format("%m"),
            day = bill.date_created.format("%d"),
            vendor_id = bill.vendor_id,
            doc_number = bill.document_number.as_ref()
                .map(|d| format!("<documentno>{}</documentno>", d))
                .unwrap_or_default(),
            ref_number = bill.reference_number.as_ref()
                .map(|r| format!("<referenceno>{}</referenceno>", r))
                .unwrap_or_default(),
            description = bill.description.as_ref()
                .map(|d| format!("<description>{}</description>", d))
                .unwrap_or_default(),
            due_date = bill.date_due.map(|d| format!(
                "<datedue><year>{}</year><month>{}</month><day>{}</day></datedue>",
                d.format("%Y"), d.format("%m"), d.format("%d")
            )).unwrap_or_default(),
            location = bill.location_id.as_ref()
                .map(|l| format!("<locationid>{}</locationid>", l))
                .unwrap_or_default(),
            department = bill.department_id.as_ref()
                .map(|d| format!("<departmentid>{}</departmentid>", d))
                .unwrap_or_default(),
            lines_xml = lines_xml,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        parse_operation_result(&response_text, &control_id)
    }

    /// Query existing AP bills / purchasing documents
    pub async fn query_ap_bills(
        &self,
        page_size: i32,
        filter: Option<&str>,
    ) -> Result<SageQueryResult<SageAPBill>> {
        let query = filter.unwrap_or("TRANSACTIONTYPE = 'Vendor Invoice'");
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readByQuery>
    <object>PODOCUMENT</object>
    <fields>RECORDNO,TRANSACTIONTYPE,VENDORID,DATECREATED,DATEDUE,DOCNUMBER,REFERENCENO,DESCRIPTION,CURRENCY,STATE,TOTALAMOUNT,LOCATIONID,DEPARTMENTID</fields>
    <query>{query}</query>
    <pagesize>{page_size}</pagesize>
  </readByQuery>
</function>"#,
            control_id = control_id,
            query = query,
            page_size = page_size,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        parse_ap_bill_query_response(&response_text)
    }

    /// Get company info / entity list
    pub async fn get_company_info(&self) -> Result<serde_json::Value> {
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readByQuery>
    <object>COMPANY</object>
    <fields>NAME,COMPANYID,COUNTRY,CURRENCY</fields>
    <query></query>
    <pagesize>1</pagesize>
  </readByQuery>
</function>"#,
            control_id = control_id,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        // Return raw response as JSON for flexibility
        Ok(serde_json::json!({
            "raw_response": response_text,
            "company_id": self.session.company_id,
            "entity_id": self.session.entity_id,
        }))
    }

    /// List available entities (for multi-entity companies)
    pub async fn list_entities(&self) -> Result<Vec<serde_json::Value>> {
        let control_id = uuid::Uuid::new_v4().to_string();

        let function_xml = format!(
            r#"<function controlid="{control_id}">
  <readByQuery>
    <object>LOCATIONENTITY</object>
    <fields>LOCATIONID,NAME,STATUS,PARENTID,CURRENCY</fields>
    <query>STATUS = 'active'</query>
    <pagesize>100</pagesize>
  </readByQuery>
</function>"#,
            control_id = control_id,
        );

        let request_xml = self.build_request(&control_id, &function_xml);
        let response_text = self.send_request(&request_xml).await?;

        // Return raw parsed entities
        Ok(vec![serde_json::json!({
            "raw_response": response_text,
        })])
    }
}

// ──────────────────────────── XML Response Parsers ────────────────────────────

/// Parse vendor query response from XML
fn parse_vendor_query_response(xml: &str) -> Result<SageQueryResult<SageVendor>> {
    // Check for errors first
    if xml.contains("<status>failure</status>") {
        let error_msg = extract_error_from_xml(xml);
        anyhow::bail!("Sage Intacct query failed: {}", error_msg);
    }

    let count = extract_xml_int(xml, "count").unwrap_or(0);
    let total_count = extract_xml_int(xml, "totalcount").unwrap_or(count);
    let num_remaining = extract_xml_int(xml, "numremaining").unwrap_or(0);
    let result_id = extract_xml_string(xml, "resultId").ok();

    // Parse individual vendor records
    let mut vendors = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<VENDOR>") {
        let abs_start = search_from + start;
        if let Some(end) = xml[abs_start..].find("</VENDOR>") {
            let record_xml = &xml[abs_start..abs_start + end + 9];
            if let Ok(vendor) = parse_single_vendor(record_xml) {
                vendors.push(vendor);
            }
            search_from = abs_start + end + 9;
        } else {
            break;
        }
    }

    Ok(SageQueryResult {
        count,
        total_count,
        num_remaining,
        result_id,
        records: vendors,
    })
}

/// Parse a single vendor XML record
fn parse_single_vendor(xml: &str) -> Result<SageVendor> {
    Ok(SageVendor {
        record_no: extract_xml_string(xml, "RECORDNO").unwrap_or_default(),
        vendor_id: extract_xml_string(xml, "VENDORID").unwrap_or_default(),
        name: extract_xml_string(xml, "NAME").unwrap_or_default(),
        status: extract_xml_string(xml, "STATUS").unwrap_or_else(|_| "active".to_string()),
        vendor_type_id: extract_xml_string(xml, "VENDTYPE").ok(),
        default_expense_gl_account: extract_xml_string(xml, "DEFAULT_LEAD_ACCT_NO").ok(),
        tax_id: extract_xml_string(xml, "TAXID").ok(),
        form_1099_eligible: extract_xml_string(xml, "FORM1099TYPE")
            .map(|v| !v.is_empty() && v != "None")
            .unwrap_or(false),
        payment_term: extract_xml_string(xml, "PAYMENTTERM").ok(),
        currency: extract_xml_string(xml, "CURRENCY").ok(),
        display_contact: Some(SageContact {
            contact_name: extract_xml_string(xml, "DISPLAYCONTACT.CONTACTNAME").ok(),
            email: extract_xml_string(xml, "DISPLAYCONTACT.EMAIL1").ok(),
            phone: extract_xml_string(xml, "DISPLAYCONTACT.PHONE1").ok(),
            address: None,
        }),
    })
}

/// Parse GL account query response
fn parse_gl_account_query_response(xml: &str) -> Result<SageQueryResult<SageGLAccount>> {
    if xml.contains("<status>failure</status>") {
        let error_msg = extract_error_from_xml(xml);
        anyhow::bail!("Sage Intacct query failed: {}", error_msg);
    }

    let count = extract_xml_int(xml, "count").unwrap_or(0);
    let total_count = extract_xml_int(xml, "totalcount").unwrap_or(count);
    let num_remaining = extract_xml_int(xml, "numremaining").unwrap_or(0);
    let result_id = extract_xml_string(xml, "resultId").ok();

    let mut accounts = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<GLACCOUNT>") {
        let abs_start = search_from + start;
        if let Some(end) = xml[abs_start..].find("</GLACCOUNT>") {
            let record_xml = &xml[abs_start..abs_start + end + 12];
            accounts.push(SageGLAccount {
                account_no: extract_xml_string(record_xml, "ACCOUNTNO").unwrap_or_default(),
                title: extract_xml_string(record_xml, "TITLE").unwrap_or_default(),
                account_type: extract_xml_string(record_xml, "ACCOUNTTYPE").unwrap_or_default(),
                normal_balance: extract_xml_string(record_xml, "NORMALBALANCE").unwrap_or_default(),
                category: extract_xml_string(record_xml, "CATEGORY").ok(),
                status: extract_xml_string(record_xml, "STATUS").unwrap_or_else(|_| "active".to_string()),
                department: extract_xml_string(record_xml, "DEPARTMENTID").ok(),
                location: extract_xml_string(record_xml, "LOCATIONID").ok(),
            });
            search_from = abs_start + end + 12;
        } else {
            break;
        }
    }

    Ok(SageQueryResult {
        count,
        total_count,
        num_remaining,
        result_id,
        records: accounts,
    })
}

/// Parse AP bill query response
fn parse_ap_bill_query_response(xml: &str) -> Result<SageQueryResult<SageAPBill>> {
    if xml.contains("<status>failure</status>") {
        let error_msg = extract_error_from_xml(xml);
        anyhow::bail!("Sage Intacct query failed: {}", error_msg);
    }

    let count = extract_xml_int(xml, "count").unwrap_or(0);
    let total_count = extract_xml_int(xml, "totalcount").unwrap_or(count);
    let num_remaining = extract_xml_int(xml, "numremaining").unwrap_or(0);
    let result_id = extract_xml_string(xml, "resultId").ok();

    // AP bills require more complex parsing — for now return empty
    // Full implementation would parse PODOCUMENT records
    Ok(SageQueryResult {
        count,
        total_count,
        num_remaining,
        result_id,
        records: Vec::new(),
    })
}

/// Parse an operation result (create/update)
fn parse_operation_result(xml: &str, control_id: &str) -> Result<SageOperationResult> {
    let status = if xml.contains("<status>success</status>") {
        "success".to_string()
    } else {
        "failure".to_string()
    };

    let key = extract_xml_string(xml, "key").ok();

    let errors = if status == "failure" {
        vec![SageError {
            error_no: extract_xml_string(xml, "errorno").unwrap_or_default(),
            description: extract_xml_string(xml, "description").unwrap_or_default(),
            description2: extract_xml_string(xml, "description2").ok(),
            correction: extract_xml_string(xml, "correction").ok(),
        }]
    } else {
        Vec::new()
    };

    Ok(SageOperationResult {
        status,
        function: "create_potransaction".to_string(),
        control_id: control_id.to_string(),
        key,
        errors,
    })
}

// ──────────────────────────── XML Utilities ────────────────────────────

fn extract_xml_string(xml: &str, tag: &str) -> Result<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = xml
        .find(&open_tag)
        .map(|i| i + open_tag.len())
        .ok_or_else(|| anyhow::anyhow!("Tag <{}> not found", tag))?;

    let end = xml[start..]
        .find(&close_tag)
        .map(|i| start + i)
        .ok_or_else(|| anyhow::anyhow!("Closing tag </{}> not found", tag))?;

    let value = xml[start..end].trim().to_string();
    if value.is_empty() {
        anyhow::bail!("Tag <{}> is empty", tag);
    }
    Ok(value)
}

fn extract_xml_int(xml: &str, tag: &str) -> Result<i32> {
    extract_xml_string(xml, tag)?
        .parse()
        .context(format!("Failed to parse <{}> as integer", tag))
}

fn extract_error_from_xml(xml: &str) -> String {
    extract_xml_string(xml, "description")
        .or_else(|_| extract_xml_string(xml, "errormessage"))
        .unwrap_or_else(|_| "Unknown error".to_string())
}
