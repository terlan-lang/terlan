use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the NativeBoundary security manifest gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBoundarySecuritySummary {
    pub operation_count: usize,
    pub policy_rule_count: usize,
}

/// Rust-backed std operation from the backend manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RustBackedOperation {
    module: String,
    operation: String,
}

/// Prefix policy applied to NativeBoundary operations.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeBoundaryPolicyRule {
    module_prefix: String,
    capability: String,
    blocking_policy: String,
    cancellation_policy: String,
    timeout_policy: String,
    worker_placement: String,
    resource_policy: String,
    error_policy: String,
    notes: String,
}

const RUST_BACKED_MANIFEST_PATH: &str = "std/RUST_BACKED_MANIFEST.tsv";
const SECURITY_MANIFEST_PATH: &str = "std/NATIVE_BOUNDARY_SECURITY.tsv";

const RUST_BACKED_HEADER: &[&str] = &[
    "module",
    "source",
    "crate",
    "operation",
    "function",
    "arity",
];

const SECURITY_HEADER: &[&str] = &[
    "module_prefix",
    "capability",
    "blocking_policy",
    "cancellation_policy",
    "timeout_policy",
    "worker_placement",
    "resource_policy",
    "error_policy",
    "notes",
];

const BLOCKING_POLICIES: &[&str] = &["nonblocking", "blocking", "may-block"];
const CANCELLATION_POLICIES: &[&str] =
    &["not-required", "cancel-on-owner-exit", "ignore-stale-reply"];
const TIMEOUT_POLICIES: &[&str] = &["none", "required", "caller-provided"];
const WORKER_PLACEMENTS: &[&str] = &["direct", "native-worker", "sandboxed-worker"];
const RESOURCE_POLICIES: &[&str] = &["value-only", "owned-resource-handle"];
const ERROR_POLICIES: &[&str] = &["typed-result"];

const FORBIDDEN_BOUNDARY_WORDS: &[&str] = &[
    "raw pointer",
    "raw_pointer",
    "foreign runtime",
    "unchecked native handle",
    "unchecked handle",
    "untyped native error",
    "untyped error",
    "NIF",
    "nif",
    "BEAM",
    "Erlang",
    "OTP",
];

/// Runs the NativeBoundary security manifest gate.
///
/// Inputs:
/// - Repository root containing std manifests.
///
/// Output:
/// - Success summary when every Rust-backed std operation has a valid security
///   policy.
/// - Stable diagnostics for missing policies, unsupported values, or unsafe
///   boundary wording.
///
/// Transformation:
/// - Expands prefix policy rules against the Rust-backed operation manifest so
///   new native operations cannot cross into Terlan without an explicit
///   capability and lifecycle contract.
pub fn run_native_boundary_security(root: &Path) -> QualityResult<NativeBoundarySecuritySummary> {
    let operations = read_rust_backed_manifest(root)?;
    let rules = read_native_boundary_security_manifest(root)?;
    let diagnostics = check_native_boundary_security(&rules, &operations);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(NativeBoundarySecuritySummary {
        operation_count: operations.len(),
        policy_rule_count: rules.len(),
    })
}

/// Reads the Rust-backed operation manifest from disk.
fn read_rust_backed_manifest(root: &Path) -> QualityResult<Vec<RustBackedOperation>> {
    let text = read_text(root, RUST_BACKED_MANIFEST_PATH)?;
    parse_rust_backed_manifest(&text)
}

/// Reads the NativeBoundary security policy manifest from disk.
fn read_native_boundary_security_manifest(
    root: &Path,
) -> QualityResult<Vec<NativeBoundaryPolicyRule>> {
    let text = read_text(root, SECURITY_MANIFEST_PATH)?;
    parse_native_boundary_security_manifest(&text)
}

/// Parses Rust-backed operation manifest TSV text.
fn parse_rust_backed_manifest(text: &str) -> QualityResult<Vec<RustBackedOperation>> {
    let mut rows = uncommented_tsv_rows(text);
    let Some(header) = rows.next() else {
        return Err(format!("{RUST_BACKED_MANIFEST_PATH}: missing header"));
    };
    validate_header(RUST_BACKED_MANIFEST_PATH, &header, RUST_BACKED_HEADER)?;

    let mut operations = Vec::new();
    for (line, fields) in rows {
        if fields.len() != RUST_BACKED_HEADER.len() {
            return Err(format!(
                "{RUST_BACKED_MANIFEST_PATH}:{line}: expected {} columns, found {}",
                RUST_BACKED_HEADER.len(),
                fields.len()
            ));
        }
        operations.push(RustBackedOperation {
            module: fields[0].to_string(),
            operation: fields[3].to_string(),
        });
    }
    Ok(operations)
}

/// Parses NativeBoundary security policy TSV text.
fn parse_native_boundary_security_manifest(
    text: &str,
) -> QualityResult<Vec<NativeBoundaryPolicyRule>> {
    let mut rows = uncommented_tsv_rows(text);
    let Some(header) = rows.next() else {
        return Err(format!("{SECURITY_MANIFEST_PATH}: missing header"));
    };
    validate_header(SECURITY_MANIFEST_PATH, &header, SECURITY_HEADER)?;

    let mut rules = Vec::new();
    for (line, fields) in rows {
        if fields.len() != SECURITY_HEADER.len() {
            return Err(format!(
                "{SECURITY_MANIFEST_PATH}:{line}: expected {} columns, found {}",
                SECURITY_HEADER.len(),
                fields.len()
            ));
        }
        rules.push(NativeBoundaryPolicyRule {
            module_prefix: fields[0].to_string(),
            capability: fields[1].to_string(),
            blocking_policy: fields[2].to_string(),
            cancellation_policy: fields[3].to_string(),
            timeout_policy: fields[4].to_string(),
            worker_placement: fields[5].to_string(),
            resource_policy: fields[6].to_string(),
            error_policy: fields[7].to_string(),
            notes: fields[8].to_string(),
        });
    }
    Ok(rules)
}

/// Returns non-comment TSV rows with one-based source line numbers.
fn uncommented_tsv_rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some((index + 1, line.split('\t').collect()))
        }
    })
}

/// Validates one TSV header row exactly.
fn validate_header(path: &str, row: &(usize, Vec<&str>), expected: &[&str]) -> QualityResult<()> {
    if row.1 == expected {
        Ok(())
    } else {
        Err(format!(
            "{path}:{}: expected header `{}`, found `{}`",
            row.0,
            expected.join("\t"),
            row.1.join("\t")
        ))
    }
}

/// Checks NativeBoundary policies against all Rust-backed operations.
fn check_native_boundary_security(
    rules: &[NativeBoundaryPolicyRule],
    operations: &[RustBackedOperation],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_policy_rules(rules));
    diagnostics.extend(validate_operation_coverage(rules, operations));
    diagnostics
}

/// Validates all security manifest rows.
fn validate_policy_rules(rules: &[NativeBoundaryPolicyRule]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen_prefixes = BTreeSet::new();
    for rule in rules {
        if !seen_prefixes.insert(rule.module_prefix.clone()) {
            diagnostics.push(format!(
                "`{}` has duplicate NativeBoundary security policy rows",
                rule.module_prefix
            ));
        }
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "blocking_policy",
            &rule.blocking_policy,
            BLOCKING_POLICIES,
        );
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "cancellation_policy",
            &rule.cancellation_policy,
            CANCELLATION_POLICIES,
        );
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "timeout_policy",
            &rule.timeout_policy,
            TIMEOUT_POLICIES,
        );
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "worker_placement",
            &rule.worker_placement,
            WORKER_PLACEMENTS,
        );
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "resource_policy",
            &rule.resource_policy,
            RESOURCE_POLICIES,
        );
        validate_enum(
            &mut diagnostics,
            &rule.module_prefix,
            "error_policy",
            &rule.error_policy,
            ERROR_POLICIES,
        );
        validate_policy_text(rule, &mut diagnostics);
        validate_policy_contract(rule, &mut diagnostics);
    }
    diagnostics
}

/// Validates a field against allowed enum values.
fn validate_enum(
    diagnostics: &mut Vec<String>,
    module_prefix: &str,
    field: &str,
    value: &str,
    allowed: &[&str],
) {
    if !allowed.contains(&value) {
        diagnostics.push(format!(
            "`{module_prefix}` has unsupported {field} `{value}`; expected one of {}",
            allowed.join(", ")
        ));
    }
}

/// Validates free-text policy fields do not weaken the boundary contract.
fn validate_policy_text(rule: &NativeBoundaryPolicyRule, diagnostics: &mut Vec<String>) {
    let mut fields = BTreeMap::new();
    fields.insert("capability", &rule.capability);
    fields.insert("notes", &rule.notes);
    for (field, value) in fields {
        for forbidden in FORBIDDEN_BOUNDARY_WORDS {
            if value.contains(forbidden) {
                diagnostics.push(format!(
                    "`{}` {field} must not use unsafe boundary wording `{forbidden}`",
                    rule.module_prefix
                ));
            }
        }
    }
}

/// Validates cross-field NativeBoundary policy constraints.
fn validate_policy_contract(rule: &NativeBoundaryPolicyRule, diagnostics: &mut Vec<String>) {
    if matches!(rule.blocking_policy.as_str(), "blocking" | "may-block")
        && rule.worker_placement == "direct"
    {
        diagnostics.push(format!(
            "`{}` is {} but is placed on direct execution; use a worker placement",
            rule.module_prefix, rule.blocking_policy
        ));
    }
    if rule.resource_policy == "owned-resource-handle" && rule.error_policy != "typed-result" {
        diagnostics.push(format!(
            "`{}` owns resources but does not return typed errors",
            rule.module_prefix
        ));
    }
    if rule.module_prefix == "std.db.Postgres" {
        if rule.worker_placement == "direct" {
            diagnostics.push("`std.db.Postgres` must execute through a native worker".to_string());
        }
        if rule.timeout_policy == "none" {
            diagnostics.push("`std.db.Postgres` must declare a timeout policy".to_string());
        }
        if rule.resource_policy != "owned-resource-handle" {
            diagnostics.push("`std.db.Postgres` must use owned-resource-handle policy".to_string());
        }
    }
    if rule.module_prefix == "std.native.collections.Vector"
        && rule.resource_policy != "owned-resource-handle"
    {
        diagnostics.push(
            "`std.native.collections.Vector` must use owned-resource-handle policy".to_string(),
        );
    }
}

/// Validates every operation has a matching policy rule.
fn validate_operation_coverage(
    rules: &[NativeBoundaryPolicyRule],
    operations: &[RustBackedOperation],
) -> Vec<String> {
    operations
        .iter()
        .filter_map(|operation| {
            matching_policy_rule(rules, &operation.module).map_or_else(
                || {
                    Some(format!(
                        "`{}` operation `{}` is missing from {SECURITY_MANIFEST_PATH}; add a prefix policy",
                        operation.module, operation.operation
                    ))
                },
                |_| None,
            )
        })
        .collect()
}

/// Finds the most specific NativeBoundary security rule for one module.
fn matching_policy_rule<'a>(
    rules: &'a [NativeBoundaryPolicyRule],
    module: &str,
) -> Option<&'a NativeBoundaryPolicyRule> {
    rules
        .iter()
        .filter(|rule| module_matches_prefix(module, &rule.module_prefix))
        .max_by_key(|rule| rule.module_prefix.len())
}

/// Returns whether a module is covered by a module-prefix rule.
fn module_matches_prefix(module: &str, prefix: &str) -> bool {
    module == prefix
        || module
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

/// Reads repository-relative text.
fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read file: {err}"))
}

/// Renders NativeBoundary security diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[native-boundary-security] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "native_boundary_security_test.rs"]
#[cfg(test)]
mod native_boundary_security_test;
