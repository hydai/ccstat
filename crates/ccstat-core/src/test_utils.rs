//! Shared test utilities for unit tests
//!
//! This module provides common test helpers for unit tests within the crate.
//!
//! Note: Integration tests (in tests/) cannot access this module because it's
//! marked with #[cfg(test)]. Integration tests have their own copy in
//! tests/common/mod.rs. This duplication is intentional and necessary due to
//! Rust's compilation model where integration tests are separate binaries.

use once_cell::sync::Lazy;
use std::env;

// Global mutex to serialize environment variable modifications in tests
pub static ENV_MUTEX: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

/// RAII guard for environment variable manipulation in tests
///
/// This guard ensures that environment variables are always restored
/// to their original state, even if a panic occurs during the test.
pub struct EnvVarGuard {
    vars: Vec<(String, Option<String>)>,
}

impl EnvVarGuard {
    /// Create a new environment variable guard
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    /// Set an environment variable and save its original value for restoration
    pub fn set(&mut self, key: &str, value: &str) {
        let original = env::var(key).ok();
        self.vars.push((key.to_string(), original));
        // Note: env::set_var is unsafe in Rust 1.82+ due to thread-safety concerns
        unsafe {
            env::set_var(key, value);
        }
    }

    /// Remove an environment variable and save its original value for restoration
    #[allow(dead_code)]
    pub fn remove(&mut self, key: &str) {
        let original = env::var(key).ok();
        self.vars.push((key.to_string(), original));
        // Note: env::remove_var is unsafe in Rust 1.82+ due to thread-safety concerns
        unsafe {
            env::remove_var(key);
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // Restore all environment variables in reverse order
        for (key, value) in self.vars.iter().rev() {
            unsafe {
                match value {
                    Some(v) => env::set_var(key, v),
                    None => env::remove_var(key),
                }
            }
        }
    }
}

impl Default for EnvVarGuard {
    fn default() -> Self {
        Self::new()
    }
}
