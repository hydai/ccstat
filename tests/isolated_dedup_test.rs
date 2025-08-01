use ccstat::types::{RawJsonlEntry, UsageEntry};
use std::collections::HashSet;

#[test]
fn test_deduplication_logic() {
    // Create test entries with duplicate IDs
    let json1 = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"id":"msg_123","model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"requestId":"req_456","costUSD":0.123}"#;
    let json2 = r#"{"timestamp":"2024-01-01T00:01:00Z","type":"assistant","message":{"id":"msg_123","model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"requestId":"req_456","costUSD":0.123}"#;
    let json3 = r#"{"timestamp":"2024-01-01T00:02:00Z","type":"assistant","message":{"id":"msg_789","model":"claude-3-sonnet","usage":{"input_tokens":200,"output_tokens":100}},"requestId":"req_012","costUSD":0.456}"#;
    
    let raw1: RawJsonlEntry = serde_json::from_str(json1).unwrap();
    let raw2: RawJsonlEntry = serde_json::from_str(json2).unwrap();
    let raw3: RawJsonlEntry = serde_json::from_str(json3).unwrap();
    
    // Test dedup key generation
    assert_eq!(UsageEntry::dedup_key(&raw1), Some("msg_123-req_456".to_string()));
    assert_eq!(UsageEntry::dedup_key(&raw2), Some("msg_123-req_456".to_string()));
    assert_eq!(UsageEntry::dedup_key(&raw3), Some("msg_789-req_012".to_string()));
    
    // Simulate deduplication
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    
    for raw in vec![raw1, raw2, raw3] {
        if let Some(key) = UsageEntry::dedup_key(&raw) {
            if !seen.contains(&key) {
                seen.insert(key);
                if let Some(entry) = UsageEntry::from_raw(raw) {
                    entries.push(entry);
                }
            }
        }
    }
    
    // Should have 2 unique entries
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].total_cost, Some(0.123));
    assert_eq!(entries[1].total_cost, Some(0.456));
}

#[test]
fn test_cost_field_compatibility() {
    // Test camelCase costUSD
    let json1 = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"costUSD":0.123}"#;
    let raw1: RawJsonlEntry = serde_json::from_str(json1).unwrap();
    let entry1 = UsageEntry::from_raw(raw1).unwrap();
    assert_eq!(entry1.total_cost, Some(0.123));
    
    // Test snake_case cost_usd
    let json2 = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"cost_usd":0.456}"#;
    let raw2: RawJsonlEntry = serde_json::from_str(json2).unwrap();
    let entry2 = UsageEntry::from_raw(raw2).unwrap();
    assert_eq!(entry2.total_cost, Some(0.456));
    
    // Test both fields present (should prefer costUSD)
    let json3 = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"cost_usd":0.456,"costUSD":0.789}"#;
    let raw3: RawJsonlEntry = serde_json::from_str(json3).unwrap();
    let entry3 = UsageEntry::from_raw(raw3).unwrap();
    assert_eq!(entry3.total_cost, Some(0.789));
}

#[test]
fn test_skip_api_error_messages() {
    let json = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"isApiErrorMessage":true}"#;
    let raw: RawJsonlEntry = serde_json::from_str(json).unwrap();
    assert!(UsageEntry::from_raw(raw).is_none());
}

#[test]
fn test_optional_session_id() {
    // Test with no session ID (should generate one)
    let json = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let raw: RawJsonlEntry = serde_json::from_str(json).unwrap();
    let entry = UsageEntry::from_raw(raw).unwrap();
    assert!(entry.session_id.as_str().starts_with("generated-"));
}

#[test]
fn test_skip_non_assistant_entries() {
    let json = r#"{"timestamp":"2024-01-01T00:00:00Z","type":"user","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let raw: RawJsonlEntry = serde_json::from_str(json).unwrap();
    assert!(UsageEntry::from_raw(raw).is_none());
}