//! Source debug identity shared by mobile bridge and route metadata.
//!
//! Inputs:
//! - Module/function names and source spans produced by compiler collection
//!   passes.
//!
//! Outputs:
//! - Stable debug identity records for generated mobile metadata.
//!
//! Transformation:
//! - Preserves enough source information for native shell replies to be mapped
//!   back to Terlan code without owning native runtime behavior here.

/// Source identity attached to generated mobile metadata entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileSourceIdentity {
    pub(crate) module_path: String,
    pub(crate) function_name: String,
    pub(crate) span: MobileSourceSpan,
}

/// One source span in a Terlan file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileSourceSpan {
    pub(crate) file: String,
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
}

/// Generated debug identity metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileDebugIdentityMetadata {
    pub(crate) module_path: String,
    pub(crate) function_name: String,
    pub(crate) file: String,
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
    pub(crate) debug_key: String,
}

/// Validation diagnostic for source debug identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileDebugIdentityDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generates debug identity metadata for one source identity.
///
/// Inputs:
/// - `identity`: compiler-collected module/function/source span identity.
///
/// Output:
/// - Stable metadata when the identity is complete and span coordinates are
///   valid.
/// - Stable diagnostics for incomplete identity.
///
/// Transformation:
/// - Validates source coordinates and builds one compact debug key.
pub(crate) fn generate_mobile_debug_identity_metadata(
    identity: &MobileSourceIdentity,
) -> Result<MobileDebugIdentityMetadata, Vec<MobileDebugIdentityDiagnostic>> {
    validate_mobile_source_identity(identity)?;
    Ok(MobileDebugIdentityMetadata {
        module_path: identity.module_path.clone(),
        function_name: identity.function_name.clone(),
        file: identity.span.file.clone(),
        start_line: identity.span.start_line,
        start_column: identity.span.start_column,
        end_line: identity.span.end_line,
        end_column: identity.span.end_column,
        debug_key: mobile_debug_key(identity),
    })
}

/// Validates one mobile source identity.
pub(crate) fn validate_mobile_source_identity(
    identity: &MobileSourceIdentity,
) -> Result<(), Vec<MobileDebugIdentityDiagnostic>> {
    let mut diagnostics = Vec::new();
    if identity.module_path.trim().is_empty() {
        diagnostics.push(diagnostic(
            "mobile_debug_identity_empty_module",
            "mobile debug identity module path must not be empty",
        ));
    }
    if identity.function_name.trim().is_empty() {
        diagnostics.push(diagnostic(
            "mobile_debug_identity_empty_function",
            "mobile debug identity function name must not be empty",
        ));
    }
    if identity.span.file.trim().is_empty() {
        diagnostics.push(diagnostic(
            "mobile_debug_identity_empty_file",
            "mobile debug identity file must not be empty",
        ));
    }
    if identity.span.start_line == 0
        || identity.span.start_column == 0
        || identity.span.end_line == 0
        || identity.span.end_column == 0
    {
        diagnostics.push(diagnostic(
            "mobile_debug_identity_zero_coordinate",
            "mobile debug identity source coordinates are one-based",
        ));
    }
    if (identity.span.end_line, identity.span.end_column)
        < (identity.span.start_line, identity.span.start_column)
    {
        diagnostics.push(diagnostic(
            "mobile_debug_identity_inverted_span",
            "mobile debug identity end position must not precede start position",
        ));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Builds a compact debug key for native-shell correlation.
pub(crate) fn mobile_debug_key(identity: &MobileSourceIdentity) -> String {
    format!(
        "{}.{}@{}:{}:{}-{}:{}",
        identity.module_path,
        identity.function_name,
        identity.span.file,
        identity.span.start_line,
        identity.span.start_column,
        identity.span.end_line,
        identity.span.end_column
    )
}

/// Builds a stable debug identity diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> MobileDebugIdentityDiagnostic {
    MobileDebugIdentityDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_debug_identity_test.rs"]
mod mobile_debug_identity_test;
