use sha2::{Digest, Sha256};

use crate::terlan_syntax::{format_interface_source_module, format_source_module};

/// Content hash for one generated binding file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GeneratedBindingFileHash {
    pub(super) path: String,
    pub(super) sha256: String,
}

/// Canonicalizes generated Terlan source before hashing or writing it.
pub(super) fn canonicalize(path: &str, contents: String) -> Result<String, String> {
    let formatted = if path.ends_with(".terli") {
        format_interface_source_module(&contents)
    } else if path.ends_with(".terl") {
        format_source_module(&contents)
    } else {
        return Ok(contents);
    };
    formatted.map_err(|error| {
        let offending = contents
            .get(error.span.start..error.span.end)
            .unwrap_or_default();
        let line_number = contents[..error.span.start.min(contents.len())]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let line = contents
            .lines()
            .nth(line_number.saturating_sub(1))
            .unwrap_or_default()
            .trim();
        format!(
            "ts_bindgen.generated_format_failed: `{path}`: {} at line {line_number}, bytes {}..{} near `{offending}` in `{line}`",
            error.message, error.span.start, error.span.end,
        )
    })
}

/// Computes lowercase SHA-256 hex for generated contents.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
