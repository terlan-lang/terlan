use std::fs;
use std::time::UNIX_EPOCH;

use super::*;

const SECURITY_HEADER_ROW: &str = "module_prefix\tcapability\tblocking_policy\tcancellation_policy\ttimeout_policy\tworker_placement\tresource_policy\terror_policy\tnotes\n";

/// Verifies the security manifest parser accepts a valid policy row.
#[test]
fn native_boundary_security_parser_accepts_valid_rows() {
    let rows = parse_native_boundary_security_manifest(&format!(
        "{SECURITY_HEADER_ROW}std.data.Json\tjson\tnonblocking\tnot-required\tnone\tdirect\tvalue-only\ttyped-result\tvalue\n"
    ))
    .expect("parse policy");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].module_prefix, "std.data.Json");
    assert_eq!(rows[0].capability, "json");
}

/// Verifies unsupported enum values are rejected.
#[test]
fn native_boundary_security_rejects_unsupported_policy_values() {
    let rules = vec![policy("std.data.Json").with_blocking("maybe")];

    let diagnostics = check_native_boundary_security(&rules, &[]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("blocking_policy")),
        "expected unsupported policy diagnostic, got {diagnostics:?}"
    );
}

/// Verifies every Rust-backed operation must have a matching policy.
#[test]
fn native_boundary_security_rejects_uncovered_operations() {
    let rules = vec![policy("std.data.Json")];
    let operations = vec![RustBackedOperation {
        module: "std.db.Postgres".to_string(),
        operation: "std.db.postgres.query".to_string(),
    }];

    let diagnostics = check_native_boundary_security(&rules, &operations);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("std.db.Postgres")),
        "expected missing operation policy diagnostic, got {diagnostics:?}"
    );
}

/// Verifies WebSocket operations are covered by the HTTP WebSocket policy.
#[test]
fn native_boundary_security_accepts_websocket_policy_coverage() {
    let rules = vec![policy("std.http.WebSocket").with_capability("http.websocket")];
    let operations = vec![
        RustBackedOperation {
            module: "std.http.WebSocket".to_string(),
            operation: "std.http.websocket.text".to_string(),
        },
        RustBackedOperation {
            module: "std.http.WebSocket".to_string(),
            operation: "std.http.websocket.endpoint".to_string(),
        },
    ];

    let diagnostics = check_native_boundary_security(&rules, &operations);

    assert!(
        diagnostics.is_empty(),
        "expected WebSocket operations to be covered, got {diagnostics:?}"
    );
}

/// Verifies blocking native calls cannot execute directly.
#[test]
fn native_boundary_security_rejects_blocking_direct_execution() {
    let rules = vec![policy("std.db.Postgres")
        .with_blocking("blocking")
        .with_worker("direct")
        .with_timeout("required")
        .with_resource("owned-resource-handle")];

    let diagnostics = check_native_boundary_security(&rules, &[]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("direct execution")),
        "expected blocking direct diagnostic, got {diagnostics:?}"
    );
}

/// Verifies unsafe raw-boundary wording is rejected.
#[test]
fn native_boundary_security_rejects_raw_boundary_wording() {
    let rules = vec![policy("std.native.collections.Vector").with_notes("raw pointer handle")];

    let diagnostics = check_native_boundary_security(&rules, &[]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("raw pointer")),
        "expected raw pointer diagnostic, got {diagnostics:?}"
    );
}

/// Verifies Postgres must use worker-owned resource semantics.
#[test]
fn native_boundary_security_requires_postgres_resource_and_timeout_policy() {
    let rules = vec![policy("std.db.Postgres")
        .with_blocking("blocking")
        .with_worker("native-worker")
        .with_timeout("none")
        .with_resource("value-only")];

    let diagnostics = check_native_boundary_security(&rules, &[]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("timeout policy")),
        "expected Postgres timeout diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("owned-resource-handle")),
        "expected Postgres resource diagnostic, got {diagnostics:?}"
    );
}

/// Verifies the full manifest gate expands policy rows against operations.
#[test]
fn native_boundary_security_gate_accepts_matching_manifests() {
    let root = make_quality_temp_dir("native_boundary_security_gate");
    fs::create_dir_all(root.join("std")).expect("create std");
    fs::write(
        root.join(RUST_BACKED_MANIFEST_PATH),
        "module\tsource\tcrate\toperation\tfunction\tarity\nstd.data.Json\tstd/data/Json.terl\tserde_json\tstd.data.json.null\tnull\t0\n",
    )
    .expect("write rust-backed manifest");
    fs::write(
        root.join(SECURITY_MANIFEST_PATH),
        format!(
            "{SECURITY_HEADER_ROW}std.data.Json\tjson\tnonblocking\tnot-required\tnone\tdirect\tvalue-only\ttyped-result\tvalue\n"
        ),
    )
    .expect("write security manifest");

    let summary = run_native_boundary_security(&root).expect("run gate");

    assert_eq!(summary.operation_count, 1);
    assert_eq!(summary.policy_rule_count, 1);
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Constructs a valid default NativeBoundary policy rule.
fn policy(module_prefix: &str) -> NativeBoundaryPolicyRule {
    NativeBoundaryPolicyRule {
        module_prefix: module_prefix.to_string(),
        capability: "capability".to_string(),
        blocking_policy: "nonblocking".to_string(),
        cancellation_policy: "not-required".to_string(),
        timeout_policy: "none".to_string(),
        worker_placement: "direct".to_string(),
        resource_policy: "value-only".to_string(),
        error_policy: "typed-result".to_string(),
        notes: "notes".to_string(),
    }
}

impl NativeBoundaryPolicyRule {
    /// Returns this policy with a different capability.
    fn with_capability(mut self, value: &str) -> Self {
        self.capability = value.to_string();
        self
    }

    /// Returns this policy with a different blocking policy.
    fn with_blocking(mut self, value: &str) -> Self {
        self.blocking_policy = value.to_string();
        self
    }

    /// Returns this policy with a different worker placement.
    fn with_worker(mut self, value: &str) -> Self {
        self.worker_placement = value.to_string();
        self
    }

    /// Returns this policy with a different timeout policy.
    fn with_timeout(mut self, value: &str) -> Self {
        self.timeout_policy = value.to_string();
        self
    }

    /// Returns this policy with a different resource policy.
    fn with_resource(mut self, value: &str) -> Self {
        self.resource_policy = value.to_string();
        self
    }

    /// Returns this policy with different notes.
    fn with_notes(mut self, value: &str) -> Self {
        self.notes = value.to_string();
        self
    }
}

/// Creates a unique temporary directory for quality unit tests.
fn make_quality_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_quality_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create quality temp dir");
    path
}
