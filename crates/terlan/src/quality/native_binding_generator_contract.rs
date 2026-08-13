use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

const CONTRACT_DOC: &str = "docs/runtime/NATIVE_BINDING_GENERATOR_CONTRACT.md";

const REQUIRED_TERMS: &[&str] = &[
    "curated wrapper surface",
    "CPython extensions",
    "pybind11",
    "Cython",
    "SWIG",
    "Terlan package",
    "NativeBoundary manifest",
    "Rust adapter",
    "cxx/bindgen/C ABI wrapper",
    "Rust adapter contract",
    "`cxx::bridge` module",
    "C header",
    "C ABI shim",
    "explicit binding manifest",
    "module identity",
    "function identity",
    "arity",
    "argument types",
    "return type",
    "blocking policy",
    "resource policy",
    "error mapping",
    "cleanup hooks",
    "ownership transfer",
    "thread-affinity rules",
    "Terlan module signatures",
    "opaque/resource handle types",
    "stale-handle metadata",
    "primitive conversions",
    "string conversions",
    "enum conversions",
    "struct conversions",
    "vector conversions",
    "`Option` conversions",
    "`Result` conversions",
    "typed error values",
    "conformance tests through the Rust adapter",
    "generated documentation with maintainer overrides",
];

const REQUIRED_REJECTION_TERMS: &[&str] = &[
    "arbitrary C++ templates",
    "inheritance-heavy APIs",
    "overloads without explicit names",
    "exceptions crossing the boundary",
    "raw pointers crossing into Terlan",
    "inferred lifetime ownership",
    "guessed thread-affinity rules",
    "unchecked native handles",
    "untyped errors",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "arbitrary c++ directly to source code is supported",
    "native headers are enough",
    "every native api is generated",
    "raw pointers may cross into terlan",
    "exceptions may cross the boundary",
    "thread affinity can be guessed",
    "mocking the native library is sufficient",
];

/// Summary produced by the native binding generator contract gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBindingGeneratorContractSummary {
    pub required_term_count: usize,
    pub rejection_term_count: usize,
}

/// Runs the native binding generator contract gate.
///
/// Inputs:
/// - Repository root containing `docs/runtime/NATIVE_BINDING_GENERATOR_CONTRACT.md`.
///
/// Output:
/// - Success summary when the binding-generator contract declares supported
///   inputs, rejected native shapes, generated outputs, and conformance rules.
/// - Stable diagnostics when the contract omits required terms or allows
///   unsafe native binding shortcuts.
///
/// Transformation:
/// - Treats the roadmap binding model as a checked golden contract before the
///   generator implementation is expanded.
pub fn run_native_binding_generator_contract(
    root: &Path,
) -> QualityResult<NativeBindingGeneratorContractSummary> {
    let text = fs::read_to_string(root.join(CONTRACT_DOC))
        .map_err(|err| format!("{CONTRACT_DOC}: failed to read contract: {err}"))?;
    let diagnostics = validate_native_binding_generator_contract_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(NativeBindingGeneratorContractSummary {
        required_term_count: REQUIRED_TERMS.len(),
        rejection_term_count: REQUIRED_REJECTION_TERMS.len(),
    })
}

/// Validates native binding generator contract text.
fn validate_native_binding_generator_contract_text(text: &str) -> Vec<String> {
    let normalized = normalize_text(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&normalize_text(term)) {
            diagnostics.push(format!(
                "missing native binding generator contract term `{term}`"
            ));
        }
    }
    for term in REQUIRED_REJECTION_TERMS {
        if !normalized.contains(&normalize_text(term)) {
            diagnostics.push(format!(
                "missing native binding generator rejection term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden native binding generator claim `{claim}`"
            ));
        }
    }
    diagnostics
}

/// Normalizes text for stable contract matching.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders native binding generator contract diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[native-binding-generator-contract] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "native_binding_generator_contract_test.rs"]
#[cfg(test)]
mod native_binding_generator_contract_test;
