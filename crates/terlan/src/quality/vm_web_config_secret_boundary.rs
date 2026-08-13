use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-config-secret-boundary-report.json";

const REQUIRED_CONFIG_SYNTAX_ANCHORS: &[&str] = &[
    "SyntaxConfigEntryOutput",
    "SyntaxConfigValueOutput",
    "is_config_declaration_kind",
    "parse_config_entries",
    "ConfigEntryParser",
];

const REQUIRED_CONFIG_CONTRACT_ANCHORS: &[&str] = &[
    "check_config_declarations_syntax_output",
    "config metadata entries",
    "target-specific validators must opt in",
    "has_structured_config_entries",
];

const REQUIRED_TLS_STD_ANCHORS: &[&str] = &[
    "pub type Mode = Auto | Manual | Internal.",
    "pub type Provider = LetsEncrypt | ZeroSsl.",
    "pub type Config =",
    "passphrase_env: Option[String]",
    "pub auto(domains: List[String], email: String): Config",
    "pub manual(cert: String, key: String): Config",
    "pub internal(server_name: String): Config",
];

const REQUIRED_GENERATED_TLS_CONFIG_ANCHORS: &[&str] = &[
    "pub type Config =",
    "passphrase_env: Option[String]",
    "pub auto(domains: List[String], email: String): Config.",
    "pub manual(cert: String, key: String): Config.",
    "pub internal(server_name: String): Config.",
];

const REQUIRED_POSTGRES_STD_ANCHORS: &[&str] = &[
    "pub type Config = {url: String}.",
    "pub connect(config: Config): Result[Pool, Error]",
    "pub query(target: Pool | Connection, sql: String, params: List[Json])",
    "pub transaction[T](pool: Pool, body: (Connection) -> Result[T, Error])",
];

const REQUIRED_PORT_STD_ANCHORS: &[&str] = &[
    "pub struct EnvVar",
    "pub struct Command",
    "environment: List[EnvVar]",
    "pub env(key: String, value: String): EnvVar",
];

const REQUIRED_SECRET_STD_ANCHORS: &[&str] = &[
    "pub struct Secret",
    "pub new(value: String): Secret",
    "pub redacted(): String",
    "pub diagnostic(secret: Secret): String",
    "pub editor_hover(secret: Secret): String",
    "pub generated_doc(secret: Secret): String",
    "pub panic_error(secret: Secret): String",
    "pub support_bundle(secret: Secret): String",
    "pub to_string(secret: Secret): String",
    "pub trace(secret: Secret): String",
];

const REQUIRED_MANIFEST_ANCHORS: &[&str] = &[
    "ProjectServerTlsBuilder",
    "validate_non_empty_values",
    "validate_auto_mode",
    "validate_manual_mode",
    "validate_internal_mode",
    "parse_server_profile",
    "passphrase_env",
];

const REQUIRED_PROJECT_MANIFEST_ANCHORS: &[&str] = &[
    "unsupported [target.wasm] key",
    "unsupported section",
    "validate_server_profile_defaults",
];

const REQUIRED_VM_TLS_ANCHORS: &[&str] = &[
    "vm_tls_plan_from_project_tls",
    "VmTlsPlan",
    "passphrase_env: tls.passphrase_env.clone()",
    "validate_vm_tls_plan",
];

const REQUIRED_DEPLOY_TLS_SECRET_ANCHORS: &[&str] = &[
    "fn plan_server_tls",
    "passphrase_env: tls.passphrase_env.clone()",
];

const REQUIRED_RUNTIME_TLS_SECRET_ANCHORS: &[&str] =
    &["manual_runtime_tls_config", "tls.passphrase_env.is_some()"];

const REQUIRED_VM_RUNTIME_TLS_SECRET_ANCHORS: &[&str] = &[
    "build_manual_server_config",
    "plan.passphrase_env.is_some()",
];

const REQUIRED_NATIVE_HELPER_SECRET_ANCHORS: &[&str] = &[
    "helper_env: native.helper_env.clone()",
    "fn apply_native_helper_envs",
    "fn discover_native_helper_envs",
    "fn push_native_helper_env",
    "std::env::var_os(&native.helper_env).is_some()",
    "bindings.push((native.helper_env.clone(), helper_path))",
];

const REQUIRED_COMPOSE_ANCHORS: &[&str] = &[
    "validate_docker_compose_file",
    "docker-compose-types",
    "validate_postgres_environment",
    "POSTGRES_DB",
    "POSTGRES_USER",
    "POSTGRES_PASSWORD",
    "docker compose",
];

const REQUIRED_TEST_ANCHORS: &[&str] = &[
    "project_manifest_rejects_server_tls_without_mode",
    "project_manifest_rejects_server_tls_auto_without_domains",
    "project_manifest_rejects_server_tls_manual_without_key",
    "project_manifest_parses_server_production_profile",
    "project_manifest_rejects_production_internal_server_tls",
    "project_manifest_rejects_partial_native_rust_helper_metadata",
    "validate_project_compose_rejects_malformed_yaml",
    "validate_project_compose_rejects_missing_postgres_service",
    "validate_project_compose_rejects_empty_map_form_postgres_environment",
    "project_tls_manual_converts_to_vm_tls_plan_without_dropping_fields",
    "tls_auto_config_records_acme_defaults",
    "connect_result_is_matchable",
    "secret_to_string_redacts_value",
    "secret_to_string_does_not_contain_value",
    "secret_redaction_marker_is_stable",
    "secret_diagnostic_redacts_value",
    "secret_editor_hover_redacts_value",
    "secret_generated_doc_redacts_value",
    "secret_panic_error_redacts_value",
    "secret_support_bundle_redacts_value",
    "secret_trace_redacts_value",
    "discover_native_helper_envs_reads_root_and_dependency_helpers",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-config-secret-boundary-check:",
    "vm-web-security-policy-check",
    "http-tls-check",
    "web-compose-check",
    "vm-web-config-secret-boundary",
];

const CONFIG_SCHEMAS: &[&str] = &[
    "syntax-output config declarations",
    "std.http.Tls.Config",
    "generated std.http.Tls config summary",
    "std.db.Postgres.Config",
    "std.vm.Port.Command environment",
    "project manifest [server.tls]",
    "project manifest [native.rust]",
    "Docker Compose postgres service",
];

const ENVIRONMENT_MATRIX: &[&str] = &[
    "dev",
    "test",
    "staging.rejectedUntilProfileSpecificSecretSources",
    "production",
    "local Docker dependency",
    "package defaults",
    "runtime-reload.rejectedUntilTypedDynamicConfigBoundary",
];

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REJECTED_CONFIGS: &[&str] = &[
    "missing server TLS mode",
    "auto TLS without domains",
    "manual TLS without key",
    "internal TLS with public ACME fields",
    "partial native Rust helper metadata",
    "malformed Docker Compose YAML",
    "missing Postgres service",
    "empty Postgres password",
    "unknown manifest section",
    "unsupported target key",
    "production profile with internal TLS",
];

const REDACTION_CHECKS: &[&str] = &[
    "TLS passphrase_env stores environment variable name only",
    "native helper_env stores environment variable name only",
    "Docker Compose password is validated but not exported in quality report",
    "std.core.Secret default display path redacts sensitive value",
    "std.core.Secret diagnostic path redacts sensitive value",
    "std.core.Secret generated-doc path redacts sensitive value",
    "std.core.Secret support-bundle path redacts sensitive value",
    "std.core.Secret editor-hover path redacts sensitive value",
    "std.core.Secret trace path redacts sensitive value",
    "std.core.Secret panic/error path redacts sensitive value",
];

const SECRET_USAGE_CHECKS: &[&str] = &[
    "TLS passphrase_env is copied from manifest to VM TLS plan",
    "TLS passphrase_env is copied from manifest to deploy plan",
    "TLS passphrase_env is rejected by local runtime until encrypted keys are supported",
    "TLS passphrase_env is rejected by VM runtime until encrypted keys are supported",
    "native helper_env is copied into package metadata",
    "native helper_env is applied to child process environments",
    "native helper_env honors existing parent process overrides",
    "native helper_env has root and dependency discovery coverage",
];

const PACKAGE_DEFAULT_DECISIONS: &[&str] = &[
    "auto TLS defaults to Let's Encrypt only through typed provider metadata",
    "manual TLS requires explicit certificate and key paths",
    "internal TLS requires local trust metadata",
    "Postgres std config starts with explicit url field only",
];

const DOCKER_DEPENDENCY_DECISIONS: &[&str] = &[
    "only project-root docker-compose.yml/compose.yml files are inspected",
    "only postgres service is started",
    "postgres image is required",
    "healthcheck is required",
    "public Postgres port binding is rejected",
    "POSTGRES_DB/USER/PASSWORD must be non-empty",
];

const RUNTIME_RELOAD_DECISIONS: &[&str] = &[
    "config metadata entries are preserved but not silently consumed",
    "target-specific validators must opt in before behavior depends on config",
    "dynamic config reload remains rejected until typed runtime boundary exists",
];

const REJECTED_SECRET_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm web config secret boundary summary.
pub struct VmWebConfigSecretBoundarySummary {
    pub config_schema_count: usize,
    pub rejected_config_count: usize,
    pub redaction_check_count: usize,
    pub secret_usage_check_count: usize,
    pub rejected_secret_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm web config secret boundary.
pub fn run_vm_web_config_secret_boundary(
    root: &Path,
) -> QualityResult<VmWebConfigSecretBoundarySummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/compiler/syntax/syntax_output/config.rs",
        REQUIRED_CONFIG_SYNTAX_ANCHORS,
        "structured config syntax output",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/validation/config_contract/mod.rs",
        REQUIRED_CONFIG_CONTRACT_ANCHORS,
        "config contract validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/Tls.terl",
        REQUIRED_TLS_STD_ANCHORS,
        "typed TLS stdlib config",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/summaries/std.http.Tls.typi",
        REQUIRED_GENERATED_TLS_CONFIG_ANCHORS,
        "generated TLS config summary",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/db/Postgres.terl",
        REQUIRED_POSTGRES_STD_ANCHORS,
        "typed Postgres stdlib config",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/vm/Port.terl",
        REQUIRED_PORT_STD_ANCHORS,
        "typed process environment config",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/core/Secret.terl",
        REQUIRED_SECRET_STD_ANCHORS,
        "non-loggable Secret stdlib value",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/project_manifest/config.rs",
        REQUIRED_MANIFEST_ANCHORS,
        "project manifest config validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/project_manifest/parser.rs",
        REQUIRED_PROJECT_MANIFEST_ANCHORS,
        "project manifest unknown-key validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/project_manifest/vm_tls.rs",
        REQUIRED_VM_TLS_ANCHORS,
        "VM TLS config bridge",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/deploy/mod.rs",
        REQUIRED_DEPLOY_TLS_SECRET_ANCHORS,
        "deploy TLS secret usage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls/acme_runtime.rs",
        REQUIRED_RUNTIME_TLS_SECRET_ANCHORS,
        "local TLS secret usage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/tls.rs",
        REQUIRED_VM_RUNTIME_TLS_SECRET_ANCHORS,
        "VM runtime TLS secret usage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/metadata.rs",
        &REQUIRED_NATIVE_HELPER_SECRET_ANCHORS[..1],
        "native helper metadata secret usage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/run/mod.rs",
        &REQUIRED_NATIVE_HELPER_SECRET_ANCHORS[1..],
        "native helper runtime secret usage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/dev_dependencies.rs",
        REQUIRED_COMPOSE_ANCHORS,
        "Docker dependency config validation",
    )?);
    diagnostics.extend(validate_required_test_terms(root)?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "environment matrix",
        ENVIRONMENT_MATRIX,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-web-config-secret-boundary",
            &diagnostics,
        ));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-web-config-secret-boundary-report-v1",
        "configSchemas": CONFIG_SCHEMAS,
        "environmentMatrix": ENVIRONMENT_MATRIX,
        "rejectedConfigs": REJECTED_CONFIGS,
        "redactionChecks": REDACTION_CHECKS,
        "secretUsageChecks": SECRET_USAGE_CHECKS,
        "packageDefaultDecisions": PACKAGE_DEFAULT_DECISIONS,
        "dockerDependencyDecisions": DOCKER_DEPENDENCY_DECISIONS,
        "runtimeReloadDecisions": RUNTIME_RELOAD_DECISIONS,
        "rejectedSecretPaths": REJECTED_SECRET_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize VM web config secret boundary report: {err}")
    })?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebConfigSecretBoundarySummary {
        config_schema_count: CONFIG_SCHEMAS.len(),
        rejected_config_count: REJECTED_CONFIGS.len(),
        redaction_check_count: REDACTION_CHECKS.len(),
        secret_usage_check_count: SECRET_USAGE_CHECKS.len(),
        rejected_secret_path_count: REJECTED_SECRET_PATHS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_required_test_terms(root: &Path) -> QualityResult<Vec<String>> {
    let files = [
        "crates/terlan/src/commands/build/project_manifest_test.rs",
        "crates/terlan/src/commands/build/project_manifest/vm_tls_test.rs",
        "crates/terlan/src/commands/serve/compose_test.rs",
        "crates/terlan/src/commands/run/run_test.rs",
        "std/http/TlsTest.terl",
        "std/db/PostgresTest.terl",
        "std/core/SecretTest.terl",
    ];
    let mut combined = String::new();
    for file in files {
        combined.push_str(&fs::read_to_string(root.join(file)).map_err(|err| {
            format!("{file}: failed to read combined config test inventory: {err}")
        })?);
        combined.push('\n');
    }
    Ok(REQUIRED_TEST_ANCHORS
        .iter()
        .filter(|term| !combined.contains(**term))
        .map(|term| format!("config tests: missing required anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile")).map_err(|err| {
        format!("Makefile: failed to read VM web config secret boundary gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM web config gate term `{term}`"))
        .collect())
}

fn validate_no_placeholder_report_entries(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM web config secret boundary {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_web_config_secret_boundary_test.rs"]
#[cfg(test)]
mod vm_web_config_secret_boundary_test;
