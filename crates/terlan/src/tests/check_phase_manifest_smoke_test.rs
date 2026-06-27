use super::*;

/// Verifies config metadata entries are visible but non-semantic in 0.0.1.
///
/// Inputs:
/// - A temporary single-file Terlan module containing a `target` config
///   declaration with structured metadata entries and one simple function.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds, records a
///   warning in the typecheck phase, and still lowers the function to CoreIR.
///
/// Transformation:
/// - Runs the generic formal compiler path and confirms config entries are
///   preserved as source metadata instead of being silently treated as backend
///   semantics.
#[test]
fn run_check_single_file_warns_for_unconsumed_config_entries_in_phase_manifest() {
    let dir = make_temp_dir("check_single_file_config_entries_warn");
    let source = dir.join("config_entries.terl");
    fs::write(
        &source,
        "module config_entries.\n\ntarget erlang {\n  otp_application: true;\n  features: [sockets]\n}.\n\npub value(): Int ->\n  1.\n",
    )
    .expect("write config entry source");
    let manifest = dir.join("config_entries.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""code":"type_warning""#));
    assert!(manifest_text.contains("config metadata entries for `target erlang`"));
    assert!(manifest_text.contains("preserved but not semantically consumed"));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies a declaration-only check emits a no-expressions Core manifest.
///
/// Inputs:
/// - A temporary single-file Terlan module containing only a public type alias.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   manifest reports CoreIR readiness as `no-expressions` with one typed
///   CoreType payload and no expression or pattern payloads.
///
/// Transformation:
/// - Runs the CLI check command through the formal pipeline and validates the
///   emitted phase-manifest JSON.
#[test]
fn run_check_single_file_type_only_emits_no_expressions_manifest() {
    let dir = make_temp_dir("check_single_file_no_expressions_manifest");
    let source = dir.join("type_only.terl");
    fs::write(&source, "module type_only.\n\npub type UserId = Int.\n")
        .expect("write type-only source");
    let manifest = dir.join("type_only.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_ne!(
        manifest_json["core_ir_hash"]
            .as_u64()
            .expect("core ir hash"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["readiness"]
            .as_str()
            .expect("core proof readiness"),
        "no-expressions"
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_type"]
            .as_u64()
            .expect("typed CoreType count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["summary_only_type"]
            .as_u64()
            .expect("summary-only CoreType count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_expr"]
            .as_u64()
            .expect("typed CoreExpr count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_pattern"]
            .as_u64()
            .expect("typed CorePattern count"),
        0
    );
}

/// Verifies declaration-only summary type debt reaches phase manifests.
///
/// Inputs:
/// - A temporary single-file Terlan module containing only a public struct
///   declaration whose body is not yet modeled as typed CoreType.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   manifest reports CoreIR readiness as `no-expressions` with one typed
///   CoreType payload.
///
/// Transformation:
/// - Runs the CLI check command through the formal pipeline and validates that
///   the struct declaration no longer creates summary-only type debt.
#[test]
fn run_check_single_file_struct_only_emits_typed_struct_body_manifest() {
    let dir = make_temp_dir("check_single_file_summary_type_debt_manifest");
    let source = dir.join("struct_only.terl");
    fs::write(
        &source,
        "module struct_only.\n\npub struct Point {\n    x: Int,\n    y: Int\n}.\n",
    )
    .expect("write struct-only source");
    let manifest = dir.join("struct_only.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_ne!(
        manifest_json["core_ir_hash"]
            .as_u64()
            .expect("core ir hash"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["readiness"]
            .as_str()
            .expect("core proof readiness"),
        "no-expressions"
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_type"]
            .as_u64()
            .expect("typed CoreType count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["summary_only_type"]
            .as_u64()
            .expect("summary-only CoreType count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_expr"]
            .as_u64()
            .expect("typed CoreExpr count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_pattern"]
            .as_u64()
            .expect("typed CorePattern count"),
        0
    );
}

/// Verifies lambda freshness obligations reach phase manifests.
///
/// Inputs:
/// - A temporary single-file Terlan module whose public function returns an
///   anonymous function expression with one runtime parameter binding.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   manifest partitions expression preservation evidence into one
///   no-runtime-binding child and one runtime-binding lambda root.
///
/// Transformation:
/// - Runs the CLI check command through the formal pipeline and validates the
///   freshness buckets future Lean proof export will need for lambda
///   substitution evidence.
#[test]
fn run_check_single_file_lambda_emits_runtime_binding_freshness_manifest() {
    let dir = make_temp_dir("check_single_file_lambda_freshness_manifest");
    let source = dir.join("lambda_freshness.terl");
    fs::write(
        &source,
        "module lambda_freshness.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
    )
    .expect("write lambda freshness source");
    let manifest = dir.join("lambda_freshness.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["readiness"]
            .as_str()
            .expect("core proof readiness"),
        "lean-covered"
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_expr"]
            .as_u64()
            .expect("typed CoreExpr count"),
        2
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr"]
            .as_u64()
            .expect("checked-preservation expression count"),
        2
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
            .as_u64()
            .expect("no-runtime-bindings expression count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr_runtime_bindings_required"]
            .as_u64()
            .expect("runtime-bindings-required expression count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_pattern"]
            .as_u64()
            .expect("checked-preservation pattern count"),
        0
    );
}
