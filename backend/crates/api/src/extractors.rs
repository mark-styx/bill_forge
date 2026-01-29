//! Request extractors for authentication and tenant context

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use billforge_core::{Error, Module, TenantContext, UserContext};

/// Extractor for authenticated user context
pub struct AuthUser(pub UserContext);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Get authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError(Error::Unauthenticated))?;

        // Extract bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError(Error::InvalidToken("Invalid authorization header".to_string())))?;

        // Validate token and get user context
        let user_context = app_state.auth.validate_token(token).await?;

        Ok(Self(user_context))
    }
}

/// Extractor for tenant context (requires authentication)
pub struct TenantCtx(pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for TenantCtx
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        
        // First get authenticated user
        let auth_user = AuthUser::from_request_parts(parts, state).await?;
        
        // Get tenant context
        let tenant_context = app_state
            .auth
            .get_tenant_context(&auth_user.0.tenant_id)
            .await?;

        Ok(Self(tenant_context))
    }
}

/// Extractor that requires a specific module to be enabled
pub struct RequireModule<const M: u8>;

impl<const M: u8> RequireModule<M> {
    pub fn module() -> Module {
        match M {
            0 => Module::InvoiceCapture,
            1 => Module::InvoiceProcessing,
            2 => Module::VendorManagement,
            3 => Module::Reporting,
            _ => Module::InvoiceCapture,
        }
    }
}

/// Extractor for Invoice Capture module access
pub struct InvoiceCaptureAccess(pub UserContext, pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for InvoiceCaptureAccess
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;

        if !tenant.has_module(Module::InvoiceCapture) {
            return Err(ApiError(Error::ModuleNotAvailable("Invoice Capture".to_string())));
        }

        Ok(Self(user, tenant))
    }
}

/// Extractor for Invoice Processing module access
pub struct InvoiceProcessingAccess(pub UserContext, pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for InvoiceProcessingAccess
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;

        if !tenant.has_module(Module::InvoiceProcessing) {
            return Err(ApiError(Error::ModuleNotAvailable("Invoice Processing".to_string())));
        }

        Ok(Self(user, tenant))
    }
}

/// Extractor for Vendor Management module access
pub struct VendorMgmtAccess(pub UserContext, pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for VendorMgmtAccess
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;

        if !tenant.has_module(Module::VendorManagement) {
            return Err(ApiError(Error::ModuleNotAvailable("Vendor Management".to_string())));
        }

        Ok(Self(user, tenant))
    }
}

/// Extractor for Reporting module access
pub struct ReportingAccess(pub UserContext, pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for ReportingAccess
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TenantCtx(tenant) = TenantCtx::from_request_parts(parts, state).await?;
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;

        if !tenant.has_module(Module::Reporting) {
            return Err(ApiError(Error::ModuleNotAvailable("Reporting".to_string())));
        }

        Ok(Self(user, tenant))
    }
}
