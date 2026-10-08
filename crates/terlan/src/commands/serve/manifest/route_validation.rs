//! Browser package asset-kind validation.

/// Validates one browser package asset kind.
///
/// Inputs:
/// - `kind`: asset kind from the package manifest.
///
/// Output:
/// - `Ok(())` when the kind belongs to the current browser package contract.
///
/// Transformation:
/// - Rejects unknown manifest asset categories before the server treats them as
///   static files.
pub(super) fn validate_asset_kind(kind: &str) -> Result<(), String> {
    match kind {
        "javascript-module"
        | "javascript-source-map"
        | "asset-file"
        | "asset-css"
        | "asset-markdown"
        | "static-asset"
        | "css" => Ok(()),
        other => Err(format!(
            "error[serve_package]: unsupported browser package asset kind `{other}`"
        )),
    }
}
