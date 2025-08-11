//! Model name formatting module for ccstat
//!
//! This module provides functions to format model names for display,
//! converting long model identifiers to short, readable names.

/// Format a model name for display
///
/// Converts full model names to short versions:
/// - `claude-opus-4-20250514` → `Opus 4`
/// - `claude-opus-4-1-20250805` → `Opus 4.1`
/// - `claude-sonnet-4-20250514` → `Sonnet 4`
/// - `claude-3-opus` → `Opus 3`
/// - `claude-3.5-sonnet` → `Sonnet 3.5`
///
/// # Arguments
///
/// * `model_name` - The full model name to format
/// * `use_full_name` - If true, returns the original name unchanged
///
/// # Examples
///
/// ```
/// use ccstat::model_formatter::format_model_name;
///
/// assert_eq!(format_model_name("claude-opus-4-20250514", false), "Opus 4");
/// assert_eq!(format_model_name("claude-opus-4-20250514", true), "claude-opus-4-20250514");
/// ```
pub fn format_model_name(model_name: &str, use_full_name: bool) -> String {
    if use_full_name {
        return model_name.to_string();
    }

    let lower_name = model_name.to_lowercase();

    // Extract the model family (opus, sonnet, haiku)
    let family = if lower_name.contains("opus") {
        "Opus"
    } else if lower_name.contains("sonnet") {
        "Sonnet"
    } else if lower_name.contains("haiku") {
        "Haiku"
    } else {
        // If no known family, return the full name for clarity
        return model_name.to_string();
    };

    // Extract version number
    let version = extract_version(&lower_name);

    if let Some(v) = version {
        format!("{} {}", family, v)
    } else {
        family.to_string()
    }
}

/// Format a list of model names
///
/// Formats each model name and joins them with a separator
///
/// # Arguments
///
/// * `models` - List of model names to format
/// * `use_full_name` - If true, returns original names unchanged
/// * `separator` - String to use for joining the formatted names
///
/// # Examples
///
/// ```
/// use ccstat::model_formatter::format_model_list;
///
/// let models = vec!["claude-opus-4-20250514".to_string(), "claude-sonnet-4-20250514".to_string()];
/// assert_eq!(format_model_list(&models, false, ", "), "Opus 4, Sonnet 4");
/// ```
pub fn format_model_list(models: &[String], use_full_name: bool, separator: &str) -> String {
    models
        .iter()
        .map(|m| format_model_name(m, use_full_name))
        .collect::<Vec<_>>()
        .join(separator)
}

/// Extract version number from model name
fn extract_version(model_name: &str) -> Option<String> {
    // Try to find version patterns like:
    // claude-opus-4-1-... → 4.1
    // claude-opus-4-... → 4
    // claude-3-opus → 3
    // claude-3.5-sonnet → 3.5
    let parts: Vec<&str> = model_name.split('-').collect();

    for i in 0..parts.len() {
        let part = parts[i];

        // Check for version pattern like "3.5"
        if part.contains('.') {
            let version_parts: Vec<&str> = part.split('.').collect();
            if version_parts.len() == 2 && version_parts.iter().all(|&p| p.parse::<u64>().is_ok()) {
                return Some(part.to_string());
            }
            // If it contains a dot but is not a valid version, it can't be an integer version part either.
            continue;
        }

        // Check for integer version parts
        if part.parse::<u64>().is_ok() {
            // Found a number, check if there's a sub-version
            if i + 1 < parts.len() && parts[i + 1].parse::<u64>().is_ok() {
                let next_part = parts[i + 1];
                // If the next part is a date (8 digits), we've found the major version
                if next_part.len() == 8 {
                    return Some(part.to_string());
                }
                // Otherwise, it's a minor version (e.g., 4-1 or 4-10)
                return Some(format!("{}.{}", part, next_part));
            } else {
                // Just a single version number (either at end or followed by non-digit)
                return Some(part.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_opus_models() {
        assert_eq!(format_model_name("claude-opus-4-20250514", false), "Opus 4");
        assert_eq!(
            format_model_name("claude-opus-4-1-20250805", false),
            "Opus 4.1"
        );
        assert_eq!(
            format_model_name("claude-opus-4-10-20250805", false),
            "Opus 4.10"
        );
        assert_eq!(format_model_name("claude-3-opus", false), "Opus 3");
        assert_eq!(format_model_name("claude-3-opus-20240229", false), "Opus 3");
    }

    #[test]
    fn test_format_sonnet_models() {
        assert_eq!(
            format_model_name("claude-sonnet-4-20250514", false),
            "Sonnet 4"
        );
        assert_eq!(format_model_name("claude-3-sonnet", false), "Sonnet 3");
        assert_eq!(format_model_name("claude-3.5-sonnet", false), "Sonnet 3.5");
        assert_eq!(
            format_model_name("claude-3.5-sonnet-20241022", false),
            "Sonnet 3.5"
        );
    }

    #[test]
    fn test_format_haiku_models() {
        assert_eq!(format_model_name("claude-3-haiku", false), "Haiku 3");
        assert_eq!(
            format_model_name("claude-3-haiku-20240307", false),
            "Haiku 3"
        );
    }

    #[test]
    fn test_format_unknown_models() {
        assert_eq!(format_model_name("gpt-4", false), "gpt-4");
        assert_eq!(format_model_name("unknown-model", false), "unknown-model");
        assert_eq!(format_model_name("singleword", false), "singleword");
    }

    #[test]
    fn test_full_name_flag() {
        assert_eq!(
            format_model_name("claude-opus-4-20250514", true),
            "claude-opus-4-20250514"
        );
        assert_eq!(
            format_model_name("claude-sonnet-4-20250514", true),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_format_model_list() {
        let models = vec![
            "claude-opus-4-20250514".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        ];

        assert_eq!(format_model_list(&models, false, ", "), "Opus 4, Sonnet 4");
        assert_eq!(
            format_model_list(&models, true, ", "),
            "claude-opus-4-20250514, claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_format_model_edge_cases() {
        // Complex model names with decimal versions in different positions
        assert_eq!(
            format_model_name("claude-opus-3.5-sonnet", false),
            "Opus 3.5"
        );
        assert_eq!(
            format_model_name("claude-opus-4.1-sonnet", false),
            "Opus 4.1"
        );
        assert_eq!(format_model_name("claude-4.1-opus", false), "Opus 4.1");
        assert_eq!(
            format_model_name("claude-sonnet-3.5-20241022", false),
            "Sonnet 3.5"
        );

        // Models with invalid version patterns
        assert_eq!(format_model_name("claude-opus-v3.5a", false), "Opus");
        assert_eq!(format_model_name("claude-1.2.3-opus", false), "Opus");
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version("claude-opus-4-20250514"),
            Some("4".to_string())
        );
        assert_eq!(
            extract_version("claude-opus-4-1-20250805"),
            Some("4.1".to_string())
        );
        assert_eq!(
            extract_version("claude-opus-4-10-20250805"),
            Some("4.10".to_string())
        );
        assert_eq!(
            extract_version("claude-opus-4-99-20250805"),
            Some("4.99".to_string())
        );
        assert_eq!(extract_version("claude-3-opus"), Some("3".to_string()));
        assert_eq!(
            extract_version("claude-3.5-sonnet"),
            Some("3.5".to_string())
        );
        assert_eq!(extract_version("claude-haiku"), None);
    }

    #[test]
    fn test_extract_version_edge_cases() {
        // Complex model names with decimal versions
        assert_eq!(
            extract_version("claude-opus-3.5-sonnet"),
            Some("3.5".to_string())
        );
        assert_eq!(
            extract_version("claude-opus-4.1-sonnet"),
            Some("4.1".to_string())
        );

        // Single decimal version as a part
        assert_eq!(extract_version("claude-4.1-opus"), Some("4.1".to_string()));

        // Multiple dots should not be treated as version
        assert_eq!(extract_version("claude-1.2.3-opus"), None);

        // Non-numeric characters with dots
        assert_eq!(extract_version("claude-v3.5a-sonnet"), None);

        // Version after family name
        assert_eq!(
            extract_version("claude-sonnet-3.5-20241022"),
            Some("3.5".to_string())
        );

        // Empty version parts
        assert_eq!(extract_version("claude--opus"), None);

        // Malformed decimal versions (should be rejected)
        assert_eq!(extract_version("claude-.5-opus"), None);
        assert_eq!(extract_version("claude-5.-opus"), None);
        assert_eq!(extract_version("claude-.-opus"), None);
    }
}
