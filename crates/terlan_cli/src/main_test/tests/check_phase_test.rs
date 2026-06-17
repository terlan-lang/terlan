use super::*;

/// Verifies directory module-layout mismatches stop before cache emission and
/// are recorded in phase manifests.
///
/// Inputs:
/// - A directory-mode source file whose path implies `app.User` while its
///   declaration says `app.Profile`.
///
/// Output:
/// - Test passes when `terlc check <dir> --emit-phase-manifest <dir>` fails,
///   emits no interface cache artifacts, and records `module_layout_error` in
///   the resolve phase.
///
/// Transformation:
/// - Runs directory checking through the public CLI command, then inspects cache
///   and manifest artifacts to prove layout validation happens before interface
///   cache emission and before typecheck/CoreIR phases.
#[test]
fn run_check_dir_rejects_module_layout_mismatch() {
    let dir = make_temp_dir("check_dir_module_layout_mismatch");
    let app_dir = dir.join("app");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::write(
        app_dir.join("User.terl"),
        "module app.Profile.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("write mismatched module source");

    let cache = dir.join("cache");
    let manifest_dir = dir.join("manifests");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                dir.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest_dir.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache.clone()),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::from(1));
    assert!(
        !cache.join("app.Profile.typi").exists(),
        "layout mismatch should stop before interface cache emission"
    );
    assert!(
        !cache.join("app.Profile.typi.deps").exists(),
        "layout mismatch should not emit interface dependency cache"
    );

    let manifest_text = fs::read_to_string(manifest_dir.join("app.Profile.phase-manifest.json"))
        .expect("read layout mismatch phase manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse layout mismatch phase manifest");
    assert_eq!(manifest["module"], "app.Profile");
    let phases = manifest["phases"].as_array().expect("phase entries");
    let resolve_phase = phases
        .iter()
        .find(|phase| phase["name"] == "resolve")
        .expect("resolve phase");
    assert_eq!(resolve_phase["status"], "error");
    assert_eq!(
        resolve_phase["diagnostics"][0]["code"],
        "module_layout_error"
    );
    assert!(resolve_phase["diagnostics"][0]["message"]
        .as_str()
        .expect("diagnostic message")
        .contains("does not match source path"));
}

/// Verifies raw macro primary expressions are rejected during syntax-phase
/// checking.
///
/// Inputs:
/// - A temporary directory containing a module that uses unsupported raw macro
///   syntax in a function body.
///
/// Output:
/// - Test success when directory `check` returns exit code `1`.
///
/// Transformation:
/// - Runs the public check command against the directory and asserts the syntax
///   phase blocks unsupported raw macro usage.
#[test]
fn run_check_dir_rejects_raw_macro_in_syntax_phase() {
    let dir = make_temp_dir("check_dir_raw_macro_rejected");
    fs::write(
        dir.join("macro_user.terl"),
        "module macro_user.\n\npub value(): Int ->\n    sql{select * from users}.\n",
    )
    .expect("write raw macro source");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));
}

/// Verifies opted-out raw declaration forms remain rejected by directory check.
///
/// Inputs:
/// - A temporary directory containing a source file with removed `protocol`
///   raw declaration syntax.
///
/// Output:
/// - Test success when directory `check` returns exit code `1`.
///
/// Transformation:
/// - Runs the public check command and asserts unsupported raw declaration
///   syntax cannot pass the current source contract.
#[test]
fn run_check_dir_rejects_unsupported_raw_declaration_kind() {
    let dir = make_temp_dir("check_dir_unsupported_raw_declaration");
    fs::write(
        dir.join("unsupported_target.terl"),
        "module unsupported_target.\nprotocol removed_form { raw }.\n",
    )
    .expect("write unsupported raw declaration source");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));
}

/// Verifies derive expansion failures are reported in the phase manifest before
/// resolve, typecheck, or CoreIR phases run.
///
/// Inputs:
/// - A temporary single-file module whose struct derives an unknown trait.
/// - A requested phase-manifest output path.
///
/// Output:
/// - Test success when check fails, parse/macro phases are `ok`,
///   derive-expansion is `error`, and later phases are `skipped`.
///
/// Transformation:
/// - Runs single-file check with phase-manifest emission and inspects the
///   manifest text for the expected phase statuses.
#[test]
fn run_check_single_file_reports_derive_expansion_phase_error() {
    let dir = make_temp_dir("check_single_file_unknown_derive");
    let source = dir.join("derive_fail.terl");
    fs::write(
        &source,
        "module derive_fail.\n\npub struct User derives MissingShow {\n    value: Int\n}.\n",
    )
    .expect("write unknown derive source");
    let manifest = dir.join("derive_fail.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"macro_expansion","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"derive_expansion","status":"error""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
}

/// Verifies a successful single-file check emits a Core phase and debug trace.
///
/// Inputs:
/// - Temporary source file whose function body lowers into Lean-covered
///   CoreIR.
///
/// Output:
/// - Test assertion only; no repository fixtures are modified.
///
/// Transformation:
/// - Runs `terlc check --emit-phase-manifest`, parses the manifest JSON,
///   and checks both CoreIR proof counters and source-to-CoreIR debug
///   identity.
#[test]
fn run_check_single_file_success_emits_core_phase_ok() {
    let dir = make_temp_dir("check_single_file_core_phase_ok");
    let source = dir.join("core_ok.terl");
    fs::write(&source, "module core_ok.\n\npub value(): Int ->\n    1.\n")
        .expect("write core ok source");
    let manifest = dir.join("core_ok.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"macro_expansion","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"derive_expansion","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    let source_path_text = source.to_string_lossy();
    assert_eq!(
        manifest_json["debug_trace"]["module"]
            .as_str()
            .expect("debug trace module"),
        "core_ok"
    );
    assert_eq!(
        manifest_json["debug_trace"]["source_path"]
            .as_str()
            .expect("debug trace source path"),
        source_path_text
    );
    assert_eq!(
        manifest_json["debug_trace"]["core_ir_available"]
            .as_bool()
            .expect("debug trace CoreIR availability"),
        true
    );
    assert_eq!(
        manifest_json["debug_trace"]["generated_artifact_kind"]
            .as_str()
            .expect("debug trace generated artifact kind"),
        "none"
    );
    assert!(
        manifest_json["debug_trace"]["generated_artifact_name"].is_null(),
        "check manifests should not claim a generated backend artifact"
    );
    assert_ne!(
        manifest_json["core_ir_hash"]
            .as_u64()
            .expect("core ir hash"),
        0
    );
    assert_eq!(
        manifest_json["debug_trace"]["core_ir_hash"]
            .as_u64()
            .expect("debug trace CoreIR hash"),
        manifest_json["core_ir_hash"]
            .as_u64()
            .expect("top-level CoreIR hash")
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["readiness"]
            .as_str()
            .expect("core proof readiness"),
        "lean-covered"
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["lean_covered"]
            .as_u64()
            .expect("lean-covered proof count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["proof_model_required"]
            .as_u64()
            .expect("proof-model-required proof count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["pattern_lean_covered"]
            .as_u64()
            .expect("lean-covered pattern proof count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_expr"]
            .as_u64()
            .expect("typed CoreExpr count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["summary_only_expr"]
            .as_u64()
            .expect("summary-only expression count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr"]
            .as_u64()
            .expect("checked-preservation expression count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr_structural"]
            .as_u64()
            .expect("structural checked-preservation expression count"),
        1
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
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["typed_core_pattern"]
            .as_u64()
            .expect("typed CorePattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["summary_only_pattern"]
            .as_u64()
            .expect("summary-only pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_pattern"]
            .as_u64()
            .expect("checked-preservation pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_pattern_structural"]
            .as_u64()
            .expect("structural checked-preservation pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_pattern_no_runtime_bindings"]
            .as_u64()
            .expect("no-runtime-bindings pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]
            ["checked_preservation_pattern_runtime_bindings_required"]
            .as_u64()
            .expect("runtime-bindings-required pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
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
            .expect("summary-only type count"),
        0
    );
}
