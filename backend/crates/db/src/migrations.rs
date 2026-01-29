//! Database migration utilities

use billforge_core::{Error, Result};

/// Migration version tracking
pub struct MigrationRunner {
    applied_migrations: Vec<String>,
}

impl MigrationRunner {
    pub fn new() -> Self {
        Self {
            applied_migrations: Vec::new(),
        }
    }

    /// Check if a migration has been applied
    pub fn is_applied(&self, name: &str) -> bool {
        self.applied_migrations.contains(&name.to_string())
    }

    /// Record a migration as applied
    pub fn mark_applied(&mut self, name: &str) {
        self.applied_migrations.push(name.to_string());
    }
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self::new()
    }
}
