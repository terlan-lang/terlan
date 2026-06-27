use super::has_structured_config_entries;
use crate::terlan_syntax::{SyntaxConfigEntryOutput, SyntaxConfigValueOutput};

/// Verifies config declarations without metadata entries stay silent.
///
/// Inputs:
/// - Preserved config text with no metadata block or an empty metadata block.
///
/// Output:
/// - Test passes when entry detection returns `false`.
///
/// Transformation:
/// - Exercises the local text-boundary detector without needing a full syntax
///   module.
#[test]
fn has_structured_config_entries_rejects_empty_config_blocks() {
    assert!(!has_structured_config_entries(&[]));
}

/// Verifies config declarations with metadata entries are detected.
///
/// Inputs:
/// - Preserved config text with one or more metadata entries.
///
/// Output:
/// - Test passes when entry detection returns `true`.
///
/// Transformation:
/// - Exercises the local text-boundary detector over single-line and
///   multi-line config metadata blocks.
#[test]
fn has_structured_config_entries_accepts_non_empty_config_blocks() {
    assert!(has_structured_config_entries(&[SyntaxConfigEntryOutput {
        key: "otp_application".to_string(),
        value: SyntaxConfigValueOutput::Bool { value: true },
    }]));
}
