//! OpenAPI documentation for the BillForge API

use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// OpenAPI documentation for BillForge API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "BillForge API",
        version = "1.0.0",
        description = "BillForge - Modern Invoice Processing Platform API

## Overview

BillForge is a comprehensive accounts payable automation platform that helps businesses:
- Capture and digitize invoices with OCR
- Route invoices through customizable approval workflows
- Manage vendor relationships
- Generate financial reports and analytics

## Authentication

All API endpoints (except /health) require authentication using JWT Bearer tokens.

1. Obtain tokens via POST /api/auth/login
2. Include the access token in the Authorization header: `Authorization: Bearer <token>`
3. Refresh tokens before expiry using POST /api/auth/refresh

## Multi-Tenancy

BillForge is a multi-tenant system. Include tenant_id in authentication requests.
",
        contact(
            name = "BillForge Support",
            email = "support@billforge.app"
        ),
        license(
            name = "Proprietary"
        )
    ),
    servers(
        (url = "/api", description = "API Server")
    ),
    tags(
        (name = "Authentication", description = "User authentication and token management"),
        (name = "Invoices", description = "Invoice capture and management"),
        (name = "Vendors", description = "Vendor management"),
        (name = "Workflows", description = "Approval workflows, queues, and rules"),
        (name = "Reports", description = "Financial reporting and analytics"),
        (name = "Health", description = "System health and monitoring")
    ),
    components(
        schemas(
            LoginRequest,
            LoginResponse,
            RegisterRequest,
            RefreshRequest,
            UserInfo,
            Invoice,
            InvoiceList,
            Vendor,
            VendorList,
            HealthResponse,
            ErrorResponse,
            PaginationInfo,
        )
    )
)]
pub struct ApiDoc;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// Authentication Schemas
// ============================================================================

/// Login request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Tenant ID (UUID format)
    #[schema(example = "11111111-1111-1111-1111-111111111111")]
    pub tenant_id: String,
    /// User email address
    #[schema(example = "admin@sandbox.local")]
    pub email: String,
    /// User password
    #[schema(example = "sandbox123")]
    pub password: String,
}

/// Login response with JWT tokens
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    /// JWT access token for API authentication
    pub access_token: String,
    /// JWT refresh token for obtaining new access tokens
    pub refresh_token: String,
    /// Token type (always "Bearer")
    #[schema(example = "Bearer")]
    pub token_type: String,
    /// Access token expiration in seconds
    #[schema(example = 86400)]
    pub expires_in: u64,
}

/// User registration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// Tenant ID to register under
    pub tenant_id: String,
    /// User email address
    pub email: String,
    /// Password (min 8 chars, requires uppercase, lowercase, number)
    pub password: String,
    /// User display name
    pub name: String,
}

/// Token refresh request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// Refresh token from login response
    pub refresh_token: String,
}

/// Authenticated user information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserInfo {
    /// User ID (UUID)
    pub id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// User email
    pub email: String,
    /// User display name
    pub name: String,
    /// User roles (e.g., tenant_admin, ap_user, approver)
    pub roles: Vec<String>,
}

// ============================================================================
// Invoice Schemas
// ============================================================================

/// Invoice record
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Invoice {
    /// Invoice ID (UUID)
    pub id: String,
    /// Vendor ID (optional, UUID)
    pub vendor_id: Option<String>,
    /// Vendor name
    pub vendor_name: String,
    /// Invoice number
    #[schema(example = "INV-2024-0001")]
    pub invoice_number: String,
    /// Invoice date (YYYY-MM-DD)
    #[schema(example = "2024-01-15")]
    pub invoice_date: Option<String>,
    /// Due date (YYYY-MM-DD)
    #[schema(example = "2024-02-14")]
    pub due_date: Option<String>,
    /// Total amount in cents
    #[schema(example = 125000)]
    pub total_amount: i64,
    /// Currency code
    #[schema(example = "USD")]
    pub currency: String,
    /// Capture status (draft, pending_ocr, ready_for_review, reviewed, failed)
    pub capture_status: String,
    /// Processing status (submitted, pending_approval, approved, rejected, paid, voided)
    pub processing_status: String,
    /// Creation timestamp
    pub created_at: String,
}

/// Paginated list of invoices
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvoiceList {
    /// Invoice records
    pub data: Vec<Invoice>,
    /// Pagination information
    pub pagination: PaginationInfo,
}

// ============================================================================
// Vendor Schemas
// ============================================================================

/// Vendor record
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Vendor {
    /// Vendor ID (UUID)
    pub id: String,
    /// Vendor name
    #[schema(example = "Acme Corporation")]
    pub name: String,
    /// Vendor type (business, contractor, individual)
    pub vendor_type: String,
    /// Contact email
    pub email: Option<String>,
    /// Contact phone
    pub phone: Option<String>,
    /// Vendor status (active, inactive)
    pub status: String,
    /// Creation timestamp
    pub created_at: String,
}

/// Paginated list of vendors
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VendorList {
    /// Vendor records
    pub data: Vec<Vendor>,
    /// Pagination information
    pub pagination: PaginationInfo,
}

// ============================================================================
// Common Schemas
// ============================================================================

/// Health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Health status (healthy, degraded, unhealthy)
    #[schema(example = "healthy")]
    pub status: String,
    /// Service version
    #[schema(example = "1.0.0")]
    pub version: String,
}

/// API error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error type
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details (optional)
    pub details: Option<serde_json::Value>,
}

/// Pagination information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginationInfo {
    /// Current page number (1-indexed)
    #[schema(example = 1)]
    pub page: u32,
    /// Items per page
    #[schema(example = 25)]
    pub per_page: u32,
    /// Total number of items
    #[schema(example = 100)]
    pub total: u64,
    /// Total number of pages
    #[schema(example = 4)]
    pub total_pages: u32,
}

/// Create the Swagger UI router
pub fn swagger_ui() -> Router {
    SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}
