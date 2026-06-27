use std::path::Path;

use super::model::ProjectNativeRust;

/// Finalizes optional Rust native helper metadata.
///
/// Inputs:
/// - Optional fields collected while parsing `[native.rust]`.
///
/// Output:
/// - `Ok(None)` when no native Rust section was present.
/// - `Ok(Some(ProjectNativeRust))` when every required field is present.
/// - `Err(String)` when the section is partial or contains empty fields.
///
/// Transformation:
/// - Turns free-form manifest fields into the stable helper-discovery contract
///   serialized by package build metadata.
pub(super) fn finish_native_rust(
    path: &Path,
    crate_name: Option<String>,
    native_path: Option<String>,
    helper: Option<String>,
    helper_env: Option<String>,
    features: Option<Vec<String>>,
) -> Result<Option<ProjectNativeRust>, String> {
    if crate_name.is_none()
        && native_path.is_none()
        && helper.is_none()
        && helper_env.is_none()
        && features.is_none()
    {
        return Ok(None);
    }

    let crate_name = required_native_rust_field(path, "crate", crate_name)?;
    let native_path = required_native_rust_field(path, "path", native_path)?;
    let helper = required_native_rust_field(path, "helper", helper)?;
    let helper_env = required_native_rust_field(path, "helper_env", helper_env)?;
    let features = features.unwrap_or_default();
    if features.iter().any(|feature| feature.trim().is_empty()) {
        return Err(format!(
            "{}: [native.rust] features cannot contain empty entries",
            path.display()
        ));
    }

    Ok(Some(ProjectNativeRust {
        crate_name,
        path: native_path,
        helper,
        helper_env,
        features,
    }))
}

/// Validates one required `[native.rust]` field.
///
/// Inputs:
/// - `path`: manifest path for diagnostics.
/// - `field`: required field name.
/// - `value`: parsed optional field value.
///
/// Output:
/// - Non-empty field value or a stable diagnostic.
///
/// Transformation:
/// - Rejects missing and empty strings before package metadata can advertise an
///   unusable native helper contract.
fn required_native_rust_field(
    path: &Path,
    field: &str,
    value: Option<String>,
) -> Result<String, String> {
    let value = value.ok_or_else(|| {
        format!(
            "{}: [native.rust] requires `{}` when the section is present",
            path.display(),
            field
        )
    })?;
    if value.trim().is_empty() {
        return Err(format!(
            "{}: [native.rust] `{}` cannot be empty",
            path.display(),
            field
        ));
    }
    Ok(value)
}
