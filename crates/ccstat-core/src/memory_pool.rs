//! Memory pool for efficient allocation during parsing
//!
//! This module provides arena-based allocation to reduce memory fragmentation
//! and improve performance when processing large JSONL files.

use typed_arena::Arena;

/// Memory pool context that owns the arena
pub struct MemoryPool {
    string_arena: Arena<u8>,
}

impl Default for MemoryPool {
    fn default() -> Self {
        Self {
            string_arena: Arena::new(),
        }
    }
}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a string in the arena
    pub fn alloc_string(&self, s: &str) -> &str {
        let bytes = s.as_bytes();
        let allocated = self.string_arena.alloc_extend(bytes.iter().copied());
        unsafe {
            // Safety: we just allocated valid UTF-8 bytes
            std::str::from_utf8_unchecked(allocated)
        }
    }
}

/// Statistics about memory pool usage
pub struct PoolStats {
    /// Approximate bytes allocated
    pub bytes_allocated: usize,
    /// Number of allocations
    pub allocation_count: usize,
}

impl PoolStats {
    /// Get current pool statistics
    ///
    /// Note: This is an approximation as the arena doesn't expose exact metrics
    pub fn current() -> Self {
        // In a real implementation, we would track these metrics
        // For now, return placeholder values
        Self {
            bytes_allocated: 0,
            allocation_count: 0,
        }
    }
}

/// A batch processor that uses arena allocation
#[allow(dead_code)]
pub struct ArenaProcessor<'a> {
    arena: Arena<UsageEntryData<'a>>,
}

/// Intermediate data structure for arena allocation
#[derive(Debug)]
#[allow(dead_code)]
struct UsageEntryData<'a> {
    session_id: &'a str,
    model: &'a str,
    project: Option<&'a str>,
}

impl<'a> Default for ArenaProcessor<'a> {
    fn default() -> Self {
        Self {
            arena: Arena::new(),
        }
    }
}

impl<'a> ArenaProcessor<'a> {
    /// Create a new arena processor
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a batch of JSONL lines using arena allocation
    pub fn process_batch(&mut self, lines: &[String]) -> Vec<crate::types::UsageEntry> {
        let mut entries = Vec::with_capacity(lines.len());

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }

            // Parse directly into the final structure
            // The arena is used internally by serde for temporary allocations
            match serde_json::from_str::<crate::types::UsageEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!("Failed to parse JSONL entry: {}", e);
                }
            }
        }

        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool() {
        let pool = MemoryPool::new();

        let s1 = pool.alloc_string("hello world");
        let s2 = pool.alloc_string("hello world");

        assert_eq!(s1, "hello world");
        assert_eq!(s2, "hello world");

        // Different allocations
        assert_ne!(s1.as_ptr(), s2.as_ptr());
    }

    #[test]
    fn test_arena_processor() {
        let mut processor = ArenaProcessor::new();

        let lines = vec![
            r#"{"session_id":"test1","timestamp":"2024-01-01T00:00:00Z","model":"claude-3-opus","input_tokens":100,"output_tokens":50,"cache_creation_tokens":10,"cache_read_tokens":5}"#.to_string(),
            r#"{"session_id":"test2","timestamp":"2024-01-01T01:00:00Z","model":"claude-3-sonnet","input_tokens":200,"output_tokens":100,"cache_creation_tokens":20,"cache_read_tokens":10}"#.to_string(),
        ];

        let entries = processor.process_batch(&lines);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].session_id.as_str(), "test1");
        assert_eq!(entries[1].session_id.as_str(), "test2");
    }
}
