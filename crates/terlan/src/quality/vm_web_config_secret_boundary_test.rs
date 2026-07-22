use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_config_secret_boundary, validate_no_placeholder_report_entries};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-web-config-secret-boundary-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/compiler/syntax/syntax_output/config.rs",
            r#"
SyntaxConfigEntryOutput SyntaxConfigValueOutput is_config_declaration_kind
parse_config_entries ConfigEntryParser
"#,
        )?;
        self.write(
            "crates/terlan/src/validation/config_contract/mod.rs",
            r#"
check_config_declarations_syntax_output config metadata entries
target-specific validators must opt in has_structured_config_entries
"#,
        )?;
        self.write(
            "std/http/Tls.terl",
            r#"
pub type Mode = Auto | Manual | Internal.
pub type Provider = LetsEncrypt | ZeroSsl.
pub type Config = passphrase_env: Option[String]
pub auto(domains: List[String], email: String): Config
pub manual(cert: String, key: String): Config
pub internal(server_name: String): Config
"#,
        )?;
        self.write(
            "std/summaries/std.http.Tls.typi",
            r#"
pub type Config =
    {mode : Mode, domains : List[String], email : Option[String], primary_provider : Option[Provider], fallback_provider : Option[Provider], cert : Option[String], key : Option[String], passphrase_env : Option[String], ca : Option[String], server_name : Option[String], trust_local : Bool}.

pub auto(domains: List[String], email: String): Config.
pub internal(server_name: String): Config.
pub manual(cert: String, key: String): Config.
"#,
        )?;
        self.write(
            "std/db/Postgres.terl",
            r#"
pub type Config = {url: String}.
pub connect(config: Config): Result[Pool, Error]
pub query(pool: Pool, sql: String, params: List[Json])
pub transaction[T](pool: Pool, body: (Connection) -> Result[T, Error])
"#,
        )?;
        self.write(
            "std/vm/Port.terl",
            r#"
pub struct EnvVar pub struct Command environment: List[EnvVar]
pub env(key: String, value: String): EnvVar
"#,
        )?;
        self.write(
            "std/core/Secret.terl",
            r#"
pub struct Secret
pub new(value: String): Secret
pub redacted(): String
pub diagnostic(secret: Secret): String
pub editor_hover(secret: Secret): String
pub generated_doc(secret: Secret): String
pub panic_error(secret: Secret): String
pub support_bundle(secret: Secret): String
pub to_string(secret: Secret): String
pub trace(secret: Secret): String
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/project_manifest/config.rs",
            r#"
ProjectServerTlsBuilder validate_non_empty_values validate_auto_mode
validate_manual_mode validate_internal_mode parse_server_profile
passphrase_env
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/project_manifest.rs",
            r#"
unsupported [target.wasm] key unsupported section
validate_server_profile_defaults
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/project_manifest/vm_tls.rs",
            r#"
vm_tls_plan_from_project_tls VmTlsPlan passphrase_env: tls.passphrase_env.clone()
validate_vm_tls_plan
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/deploy/mod.rs",
            r#"
fn plan_server_tls passphrase_env: tls.passphrase_env.clone()
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
manual_runtime_tls_config tls.passphrase_env.is_some()
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls.rs",
            r#"
build_manual_server_config plan.passphrase_env.is_some()
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/metadata.rs",
            r#"
helper_env: native.helper_env.clone()
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/run/mod.rs",
            r#"
fn apply_native_helper_envs
fn discover_native_helper_envs
fn push_native_helper_env
std::env::var_os(&native.helper_env).is_some()
bindings.push((native.helper_env.clone(), helper_path))
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/compose_check.rs",
            r#"
validate_docker_compose_file docker-compose-types validate_postgres_environment
POSTGRES_DB POSTGRES_USER POSTGRES_PASSWORD docker compose
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/project_manifest_test.rs",
            r#"
project_manifest_rejects_server_tls_without_mode
project_manifest_rejects_server_tls_auto_without_domains
project_manifest_rejects_server_tls_manual_without_key
project_manifest_parses_server_production_profile
project_manifest_rejects_production_internal_server_tls
project_manifest_rejects_partial_native_rust_helper_metadata
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/project_manifest/vm_tls_test.rs",
            "project_tls_manual_converts_to_vm_tls_plan_without_dropping_fields",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/compose_test.rs",
            r#"
validate_project_compose_rejects_malformed_yaml
validate_project_compose_rejects_missing_postgres_service
validate_project_compose_rejects_empty_map_form_postgres_environment
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/run/run_test.rs",
            "discover_native_helper_envs_reads_root_and_dependency_helpers",
        )?;
        self.write(
            "std/http/TlsTest.terl",
            "tls_auto_config_records_acme_defaults",
        )?;
        self.write("std/db/PostgresTest.terl", "connect_result_is_matchable")?;
        self.write(
            "std/core/SecretTest.terl",
            r#"
secret_to_string_redacts_value
secret_to_string_does_not_contain_value
secret_redaction_marker_is_stable
secret_diagnostic_redacts_value
secret_editor_hover_redacts_value
secret_generated_doc_redacts_value
secret_panic_error_redacts_value
secret_support_bundle_redacts_value
secret_trace_redacts_value
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-web-config-secret-boundary-check: vm-web-security-policy-check
	$(MAKE) http-tls-check
	$(MAKE) web-compose-check
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_web_config_secret_boundary_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-config-secret-boundary
"#;

#[test]
fn vm_web_config_secret_boundary_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_config_secret_boundary(repo.root()).expect("quality check");

    assert_eq!(summary.config_schema_count, 8);
    assert_eq!(summary.rejected_config_count, 11);
    assert_eq!(summary.redaction_check_count, 10);
    assert_eq!(summary.secret_usage_check_count, 8);
    assert_eq!(summary.rejected_secret_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-config-secret-boundary-report-v1"));
    assert!(report.contains("std.http.Tls.Config"));
    assert!(report.contains("generated std.http.Tls config summary"));
    assert!(report.contains("TLS passphrase_env is copied from manifest to VM TLS plan"));
    assert!(report.contains("native helper_env is applied to child process environments"));
    assert!(report.contains("staging.rejectedUntilProfileSpecificSecretSources"));
    assert!(report.contains("runtime-reload.rejectedUntilTypedDynamicConfigBoundary"));
    assert!(report.contains("std.core.Secret default display path redacts sensitive value"));
    assert!(report.contains("std.core.Secret diagnostic path redacts sensitive value"));
    assert!(report.contains("std.core.Secret editor-hover path redacts sensitive value"));
    assert!(report.contains("std.core.Secret generated-doc path redacts sensitive value"));
    assert!(report.contains("std.core.Secret support-bundle path redacts sensitive value"));
    assert!(report.contains("std.core.Secret trace path redacts sensitive value"));
    assert!(report.contains("std.core.Secret panic/error path redacts sensitive value"));
    assert!(report.contains("production profile with internal TLS"));
    assert!(!report.contains("std.core.Secret non-loggable value"));
    assert!(!report.contains("production dev-default rejection"));
    assert!(!report.contains("diagnostic redaction proof"));
    assert!(!report.contains("editor-hover redaction proof"));
    assert!(!report.contains("generated-doc redaction proof"));
    assert!(!report.contains("trace redaction proof"));
    assert!(!report.contains("panic/error rendering redaction proof"));
    assert!(!report.contains("support-bundle redaction proof"));
    assert!(!report.contains("stale generated config detection"));
    assert!(!report.contains("unused secret detection"));
    assert!(!report.contains("placeholder"));
    assert!(report.contains("dynamic config reload remains rejected"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_placeholder_report_entries() {
    let diagnostics =
        validate_no_placeholder_report_entries("environment matrix", &["staging-placeholder"]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}

#[test]
fn vm_web_config_secret_boundary_rejects_missing_tls_std_anchor() {
    let repo = TestRepo::new("missing-tls-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/http/Tls.terl");
    let source = fs::read_to_string(&path).expect("tls source");
    repo.write(
        "std/http/Tls.terl",
        &source.replace("passphrase_env: Option[String]", ""),
    )
    .expect("rewrite TLS source");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("passphrase_env: Option[String]"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_stale_generated_tls_summary() {
    let repo = TestRepo::new("stale-generated-tls-summary").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/summaries/std.http.Tls.typi");
    let source = fs::read_to_string(&path).expect("generated TLS summary");
    repo.write(
        "std/summaries/std.http.Tls.typi",
        &source.replace("passphrase_env : Option[String]", ""),
    )
    .expect("rewrite generated TLS summary");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("generated TLS config summary"));
    assert!(error.contains("passphrase_env : Option[String]"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_unconsumed_native_helper_env() {
    let repo = TestRepo::new("unconsumed-native-helper-env").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/commands/run/mod.rs");
    let source = fs::read_to_string(&path).expect("run source");
    repo.write(
        "crates/terlan/src/commands/run/mod.rs",
        &source.replace(
            "bindings.push((native.helper_env.clone(), helper_path))",
            "",
        ),
    )
    .expect("rewrite run source");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("native helper runtime secret usage"));
    assert!(error.contains("bindings.push((native.helper_env.clone(), helper_path))"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_unconsumed_tls_passphrase_env() {
    let repo = TestRepo::new("unconsumed-tls-passphrase-env").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/tls.rs");
    let source = fs::read_to_string(&path).expect("VM TLS source");
    repo.write(
        "crates/terlan/src/runtime/vm/tls.rs",
        &source.replace("plan.passphrase_env.is_some()", ""),
    )
    .expect("rewrite VM TLS source");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VM runtime TLS secret usage"));
    assert!(error.contains("plan.passphrase_env.is_some()"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_missing_compose_anchor() {
    let repo = TestRepo::new("missing-compose-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/compose_check.rs");
    let source = fs::read_to_string(&path).expect("compose source");
    repo.write(
        "crates/terlan/src/commands/serve/compose_check.rs",
        &source.replace("POSTGRES_PASSWORD", ""),
    )
    .expect("rewrite compose source");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("POSTGRES_PASSWORD"));
}

#[test]
fn vm_web_config_secret_boundary_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) web-compose-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_config_secret_boundary(repo.root()).expect_err("gate should fail");

    assert!(error.contains("web-compose-check"));
}
