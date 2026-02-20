//! String interning for memory optimization
//!
//! This module provides string interning capabilities to reduce memory usage
//! when dealing with repeated strings like model names and session IDs.

use once_cell::sync::Lazy;
use std::sync::RwLock;
use string_interner::{DefaultBackend, DefaultSymbol, StringInterner};

/// Global string interner for model names
static MODEL_INTERNER: Lazy<RwLock<StringInterner<DefaultBackend>>> =
    Lazy::new(|| RwLock::new(StringInterner::default()));

/// Global string interner for session IDs
static SESSION_INTERNER: Lazy<RwLock<StringInterner<DefaultBackend>>> =
    Lazy::new(|| RwLock::new(StringInterner::default()));

/// Interned model name
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternedModel(DefaultSymbol);

impl InternedModel {
    /// Create or get an interned model name
    pub fn new(model: &str) -> Self {
        let mut interner = MODEL_INTERNER.write().unwrap();
        let symbol = interner.get_or_intern(model);
        Self(symbol)
    }

    /// Get the string value
    pub fn as_str(&self) -> String {
        let interner = MODEL_INTERNER.read().unwrap();
        interner.resolve(self.0).unwrap_or("unknown").to_string()
    }
}

/// Interned session ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternedSession(DefaultSymbol);

impl InternedSession {
    /// Create or get an interned session ID
    pub fn new(session: &str) -> Self {
        let mut interner = SESSION_INTERNER.write().unwrap();
        let symbol = interner.get_or_intern(session);
        Self(symbol)
    }

    /// Get the string value
    pub fn as_str(&self) -> String {
        let interner = SESSION_INTERNER.read().unwrap();
        interner.resolve(self.0).unwrap_or("unknown").to_string()
    }
}

/// Statistics about string interning
pub struct InternerStats {
    pub model_count: usize,
    pub session_count: usize,
    pub model_memory_saved: usize,
    pub session_memory_saved: usize,
}

impl InternerStats {
    /// Get current interner statistics
    pub fn current() -> Self {
        let model_interner = MODEL_INTERNER.read().unwrap();
        let session_interner = SESSION_INTERNER.read().unwrap();

        // Estimate memory savings (rough calculation)
        let avg_model_len = 20; // Average model name length
        let avg_session_len = 36; // Average session ID length

        let model_count = model_interner.len();
        let session_count = session_interner.len();

        // Memory saved = (number of duplicates) * (average string size)
        // This is a rough estimate
        let model_memory_saved = model_count.saturating_sub(10) * avg_model_len;
        let session_memory_saved = session_count.saturating_sub(100) * avg_session_len;

        Self {
            model_count,
            session_count,
            model_memory_saved,
            session_memory_saved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_interning() {
        let model1 = InternedModel::new("claude-3-opus");
        let model2 = InternedModel::new("claude-3-opus");
        let model3 = InternedModel::new("claude-3-sonnet");

        // Same strings should have same symbol
        assert_eq!(model1, model2);
        assert_ne!(model1, model3);

        // Should be able to retrieve strings
        assert_eq!(model1.as_str(), "claude-3-opus");
        assert_eq!(model3.as_str(), "claude-3-sonnet");
    }

    #[test]
    fn test_session_interning() {
        let session1 = InternedSession::new("session-123");
        let session2 = InternedSession::new("session-123");
        let session3 = InternedSession::new("session-456");

        assert_eq!(session1, session2);
        assert_ne!(session1, session3);

        assert_eq!(session1.as_str(), "session-123");
        assert_eq!(session3.as_str(), "session-456");
    }

    #[test]
    fn test_interner_stats() {
        // Create some interned strings
        for i in 0..5 {
            InternedModel::new(&format!("model-{i}"));
            InternedSession::new(&format!("session-{i}"));
        }

        let stats = InternerStats::current();
        assert!(stats.model_count >= 5);
        assert!(stats.session_count >= 5);
    }
}
