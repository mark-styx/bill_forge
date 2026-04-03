//! Sage Intacct session-based authentication
//!
//! Sage Intacct uses XML Web Services with session-based auth:
//! 1. Send `getAPISession` request with sender credentials + company login
//! 2. Receive `sessionid` in response
//! 3. Use `sessionid` for all subsequent API calls
//! Sessions typically last ~15 minutes.

use anyhow::{Context, Result};
use tracing;

/// Sage Intacct Web Services API endpoint
const SAGE_INTACCT_ENDPOINT: &str = "https://api.intacct.com/ia/xml/xmlgw.phtml";

/// Sage Intacct authentication configuration
#[derive(Debug, Clone)]
pub struct SageIntacctAuthConfig {
    /// Web Services sender ID (provided by Sage)
    pub sender_id: String,
    /// Web Services sender password
    pub sender_password: String,
    /// Company ID
    pub company_id: String,
    /// Entity ID (for multi-entity companies, optional)
    pub entity_id: Option<String>,
    /// User ID for company login
    pub user_id: String,
    /// User password for company login
    pub user_password: String,
}

/// Sage Intacct authentication client
pub struct SageIntacctAuth {
    config: SageIntacctAuthConfig,
    http_client: reqwest::Client,
}

/// Session response from Sage Intacct
#[derive(Debug, Clone)]
pub struct SageIntacctSession {
    /// Session ID for API calls
    pub session_id: String,
    /// API endpoint (may differ from default)
    pub endpoint: String,
    /// Company ID
    pub company_id: String,
    /// Entity ID (if multi-entity)
    pub entity_id: Option<String>,
}

impl SageIntacctAuth {
    /// Create a new Sage Intacct auth client
    pub fn new(config: SageIntacctAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Get API endpoint
    pub fn endpoint() -> &'static str {
        SAGE_INTACCT_ENDPOINT
    }

    /// Establish a new API session
    pub async fn get_session(&self) -> Result<SageIntacctSession> {
        let entity_login = self
            .config
            .entity_id
            .as_ref()
            .map(|id| format!("<locationid>{}</locationid>", id))
            .unwrap_or_default();

        let request_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<request>
  <control>
    <senderid>{sender_id}</senderid>
    <password>{sender_password}</password>
    <controlid>{control_id}</controlid>
    <uniqueid>false</uniqueid>
    <dtdversion>3.0</dtdversion>
    <includewhitespace>false</includewhitespace>
  </control>
  <operation>
    <authentication>
      <login>
        <userid>{user_id}</userid>
        <companyid>{company_id}</companyid>
        <password>{user_password}</password>
        {entity_login}
      </login>
    </authentication>
    <content>
      <function controlid="get_session">
        <getAPISession />
      </function>
    </content>
  </operation>
</request>"#,
            sender_id = self.config.sender_id,
            sender_password = self.config.sender_password,
            control_id = uuid::Uuid::new_v4(),
            user_id = self.config.user_id,
            company_id = self.config.company_id,
            user_password = self.config.user_password,
            entity_login = entity_login,
        );

        let response = self
            .http_client
            .post(SAGE_INTACCT_ENDPOINT)
            .header("Content-Type", "application/xml")
            .body(request_xml)
            .send()
            .await
            .context("Failed to send session request to Sage Intacct")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Sage Intacct session request failed: {}", error_text);
        }

        let response_text = response
            .text()
            .await
            .context("Failed to read Sage Intacct session response")?;

        // Parse session ID from XML response
        let session_id = extract_xml_value(&response_text, "sessionid")
            .context("Failed to extract session ID from Sage Intacct response")?;

        let endpoint = extract_xml_value(&response_text, "endpoint")
            .unwrap_or_else(|_| SAGE_INTACCT_ENDPOINT.to_string());

        tracing::info!(
            company_id = %self.config.company_id,
            "Sage Intacct session established"
        );

        Ok(SageIntacctSession {
            session_id,
            endpoint,
            company_id: self.config.company_id.clone(),
            entity_id: self.config.entity_id.clone(),
        })
    }

    /// Test connection by establishing a session
    pub async fn test_connection(&self) -> Result<bool> {
        match self.get_session().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(error = %e, "Sage Intacct connection test failed");
                Ok(false)
            }
        }
    }
}

/// Extract a value from an XML response by tag name.
/// Simple extraction without full XML parsing for lightweight use.
fn extract_xml_value(xml: &str, tag: &str) -> Result<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = xml
        .find(&open_tag)
        .map(|i| i + open_tag.len())
        .ok_or_else(|| anyhow::anyhow!("Tag <{}> not found in response", tag))?;

    let end = xml[start..]
        .find(&close_tag)
        .map(|i| start + i)
        .ok_or_else(|| anyhow::anyhow!("Closing tag </{}> not found in response", tag))?;

    Ok(xml[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_xml_value() {
        let xml = r#"<response><sessionid>abc123</sessionid><endpoint>https://api.intacct.com</endpoint></response>"#;
        assert_eq!(extract_xml_value(xml, "sessionid").unwrap(), "abc123");
        assert_eq!(
            extract_xml_value(xml, "endpoint").unwrap(),
            "https://api.intacct.com"
        );
    }

    #[test]
    fn test_extract_xml_value_missing_tag() {
        let xml = "<response><other>value</other></response>";
        assert!(extract_xml_value(xml, "sessionid").is_err());
    }
}
