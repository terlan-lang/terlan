use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::AuditError;

const STRUCTURAL_FIELDS: [&str; 10] = [
    "parse_failures",
    "source_boundary_parse_failures",
    "cross_tree_path_attributes",
    "thin_binary_diagnostics",
    "feature_module_diagnostics",
    "exact_normalized_duplicates",
    "equal_shape_candidates",
    "same_name_candidates",
    "crate_references",
    "source_files",
];

pub(super) fn write_structural_input(path: &Path, report: &Value) -> Result<(), AuditError> {
    let mut projection = json!({
        "schema": "terlan.rust-structural-input.v1",
    });
    let object = projection
        .as_object_mut()
        .ok_or_else(|| AuditError::Message("cannot construct Rust structural input".to_owned()))?;
    for field in STRUCTURAL_FIELDS {
        let value = report
            .get(field)
            .ok_or_else(|| AuditError::Message(format!("Rust boundary audit lacks `{field}`")))?;
        object.insert(field.to_owned(), value.clone());
    }
    let encoded = serde_json::to_vec(&projection).map_err(|error| {
        AuditError::Message(format!("cannot encode Rust structural input: {error}"))
    })?;
    fs::write(path, encoded).map_err(|error| {
        AuditError::Message(format!(
            "cannot write Rust structural input `{}`: {error}",
            path.display()
        ))
    })
}
