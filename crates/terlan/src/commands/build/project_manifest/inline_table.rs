use std::collections::BTreeMap;
use std::path::Path;

use super::split_key_value;
use super::strings::{parse_string, parse_string_array, split_array_items};

/// Parsed dependency inline-table field value.
///
/// Inputs:
/// - Produced from one manifest inline dependency table.
///
/// Output:
/// - A string field or string-array field admitted by the manifest subset.
///
/// Transformation:
/// - Keeps dependency field parsing typed enough for Rust feature lists without
///   expanding the full TOML grammar into the hand-written manifest reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProjectManifestInlineValue {
    String(String),
    StringArray(Vec<String>),
}

/// Returns a dependency inline-table string field.
///
/// Inputs:
/// - `fields`: parsed inline-table fields.
/// - `key`: field name expected to contain a string.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Field string value.
///
/// Transformation:
/// - Rejects string-array values where the dependency contract requires a
///   scalar string.
pub(super) fn expect_inline_string_field(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    key: &str,
    path: &Path,
    line_no: usize,
) -> Result<String, String> {
    match fields.get(key) {
        Some(ProjectManifestInlineValue::String(value)) => Ok(value.clone()),
        Some(ProjectManifestInlineValue::StringArray(_)) => Err(format!(
            "{}:{}: project dependency field `{}` must be a string",
            path.display(),
            line_no,
            key
        )),
        None => Err(format!(
            "{}:{}: project dependency field `{}` is missing",
            path.display(),
            line_no,
            key
        )),
    }
}

/// Returns a dependency inline-table string-array field.
///
/// Inputs:
/// - `fields`: parsed inline-table fields.
/// - `key`: field name expected to contain an array of strings.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Field string-array value.
///
/// Transformation:
/// - Rejects scalar values where the dependency contract requires a list.
pub(super) fn expect_inline_string_array_field(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    key: &str,
    path: &Path,
    line_no: usize,
) -> Result<Vec<String>, String> {
    match fields.get(key) {
        Some(ProjectManifestInlineValue::StringArray(value)) => Ok(value.clone()),
        Some(ProjectManifestInlineValue::String(_)) => Err(format!(
            "{}:{}: project dependency field `{}` must be an array of strings",
            path.display(),
            line_no,
            key
        )),
        None => Err(format!(
            "{}:{}: project dependency field `{}` is missing",
            path.display(),
            line_no,
            key
        )),
    }
}

/// Parses a one-line manifest inline table.
///
/// Inputs:
/// - `value`: trimmed inline-table source text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Ordered map of typed fields.
///
/// Transformation:
/// - Parses the reviewed `{ key = "value", features = ["..."] }` subset and
///   rejects duplicate keys, empty fields, and unsupported values.
pub(super) fn parse_inline_table(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<BTreeMap<String, ProjectManifestInlineValue>, String> {
    let inner = value
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project dependency value must be an inline table",
                path.display(),
                line_no
            )
        })?;
    let mut fields = BTreeMap::new();
    for item in split_array_items(inner, path, line_no)? {
        let (key, value) = split_key_value(item.trim(), path, line_no)?;
        if fields.contains_key(key) {
            return Err(format!(
                "{}:{}: duplicate project dependency field `{}`",
                path.display(),
                line_no,
                key
            ));
        }
        fields.insert(
            key.to_string(),
            parse_inline_value(value.trim(), path, line_no)?,
        );
    }
    if fields.is_empty() {
        return Err(format!(
            "{}:{}: project dependency inline table cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(fields)
}

/// Parses one inline-table value.
///
/// Inputs:
/// - `value`: trimmed field value source.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - String or string-array inline value.
///
/// Transformation:
/// - Keeps the dependency table grammar intentionally small while admitting
///   Rust feature lists for target-native package metadata.
fn parse_inline_value(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectManifestInlineValue, String> {
    if value.starts_with('[') {
        parse_string_array(value, path, line_no).map(ProjectManifestInlineValue::StringArray)
    } else {
        parse_string(value, path, line_no).map(ProjectManifestInlineValue::String)
    }
}
