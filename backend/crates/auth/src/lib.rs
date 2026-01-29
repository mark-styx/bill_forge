//! BillForge Authentication & Authorization
//!
//! Provides JWT-based authentication with tenant isolation.

mod jwt;
mod password;
mod service;

#[cfg(test)]
mod tests;

pub use jwt::{Claims, JwtConfig, JwtService};
pub use password::PasswordService;
pub use service::{AuthResponse, AuthService, LoginInput, RegisterInput, UserInfo, TenantInfo, TenantSettingsInfo};
