use super::*;

/// Verifies multi-generator list comprehensions fail before semantic
/// phases.
///
/// Inputs:
/// - A temporary single-file Terlan module containing a list comprehension
///   with two generators and a requested phase-manifest output path.
///
/// Output:
/// - Test assertion only; the command must fail and write a phase manifest
///   with a parse diagnostic and skipped resolve/typecheck/CoreIR phases.
///
/// Transformation:
/// - Runs the command-level check path and confirms the parser-level A0.24
///   collection contract is visible in phase output before unsupported
///   comprehension shape can reach semantic lowering.
#[test]
fn run_check_single_file_rejects_multi_generator_list_comprehension_before_phase_manifest() {
    let dir = make_temp_dir("check_single_file_multi_generator_list_comprehension_rejected");
    let source = dir.join("multi_generator_list_comprehension.terl");
    fs::write(
            &source,
            "module multi_generator_list_comprehension.\n\npub value(values: List[Int], others: List[Int]): Dynamic ->\n  [value | value <- values, other <- others].\n",
        )
        .expect("write multi-generator list comprehension source");
    let manifest = dir.join("multi_generator_list_comprehension.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
    assert!(
        manifest_text.contains("multiple list comprehension generators are not supported"),
        "{manifest_text}"
    );
    assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
}

/// Verifies Erlang binary segment syntax fails at parse time.
///
/// Inputs:
/// - A temporary single-file Terlan module containing Erlang binary
///   segment syntax and a requested phase-manifest output path.
///
/// Output:
/// - Test assertion only; the command must fail and write a phase manifest
///   whose parse phase records the source-syntax diagnostic.
///
/// Transformation:
/// - Runs the command-level check path and confirms backend Erlang binary
///   syntax cannot enter Terlan typechecking or CoreIR.
#[test]
fn run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest() {
    let dir = make_temp_dir("check_single_file_binary_segment_lowering_rejected");
    let source = dir.join("binary_segment_lowering.terl");
    fs::write(
            &source,
            "module binary_segment_lowering.\n\npub byte(value: Int): Binary ->\n  <<value:8/integer-unsigned-big>>.\n",
        )
        .expect("write binary segment source");
    let manifest = dir.join("binary_segment_lowering.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    assert!(manifest_text.contains("Erlang binary literal syntax"));
}

/// Verifies unsupported subject-bearing annotations stop in the syntax
/// phase of the command-level `check` path.
///
/// Inputs:
/// - A temporary module containing an unambiguous annotation subject and a
///   requested phase-manifest output path.
///
/// Output:
/// - Test assertion only; the command must fail and write a phase manifest
///   with parse error plus skipped later phases.
///
/// Transformation:
/// - Runs `terlc check` through the normal command entrypoint and confirms
///   the A0.32 annotation-subject diagnostic is visible in phase output
///   and prevents resolution, typecheck, and CoreIR from running.
#[test]
fn run_check_single_file_rejects_annotation_subject_before_phase_manifest() {
    let dir = make_temp_dir("check_single_file_annotation_subject_rejected");
    let source = dir.join("annotation_subject.terl");
    fs::write(
        &source,
        "module annotation_subject.\n\n@doc \"User type\"\ntype User = Int.\n",
    )
    .expect("write annotation subject source");
    let manifest = dir.join("annotation_subject.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
    assert!(manifest_text.contains("annotation subjects are not supported in Terlan 0.0.1"));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
}

/// Verifies invalid annotation metadata stops before semantic phases.
///
/// Inputs:
/// - A temporary module declaring a user annotation schema with a required
///   `String` key and using the annotation without that key.
///
/// Output:
/// - Test assertion only; the command must fail and write a phase manifest
///   whose parse phase records the schema error while later phases remain
///   skipped.
///
/// Transformation:
/// - Runs `terlc check --emit-phase-manifest` through the command entrypoint
///   and proves typed annotation-schema validation happens before resolve,
///   typecheck, CoreIR lowering, or backend emission can observe the module.
#[test]
fn run_check_single_file_rejects_invalid_annotation_schema_usage_before_core_phase() {
    let dir = make_temp_dir("check_single_file_invalid_annotation_schema_usage");
    let source = dir.join("invalid_annotation_schema_usage.terl");
    fs::write(
        &source,
        r#"
module invalid_annotation_schema_usage.

annotation docs.example {
  applies_to: function;
  name: String { required: true };
}.

@docs.example {}
run(): Int -> 1.
"#,
    )
    .expect("write invalid annotation schema source");
    let manifest = dir.join("invalid_annotation_schema_usage.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
    assert!(manifest_text.contains("@docs.example missing required key `name`"));
    assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    assert!(manifest_text.contains(r#""core_ir_hash":0"#));
}

/// Verifies asset imports fail in the generic formal compile path before
/// backend emission.
///
/// Inputs:
/// - A temporary Terlan module with a CSS asset import and a simple
///   backend-supported function.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   CoreIR target-profile phase and records the unsupported asset-import
///   decision in the manifest.
///
/// Transformation:
/// - Runs the command-level check path and confirms parse/resolve/typecheck
///   can accept the syntax while CoreIR target-profile validation rejects
///   unresolved asset import resolution.
#[test]
fn run_check_single_file_rejects_asset_import_resolution_in_phase_manifest() {
    let dir = make_temp_dir("check_single_file_asset_import_rejected");
    let source = dir.join("asset_import.terl");
    fs::write(
            &source,
            "module asset_import.\n\nimport css \"./style.css\" as PageCss.\n\npub main(): Int ->\n    1.\n",
        )
        .expect("write asset import source");
    let manifest = dir.join("asset_import.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"error""#));
    assert!(manifest_text.contains("asset import resolution Css import `PageCss<-./style.css`"));
}

/// Verifies constructor declaration edge cases fail before backend phases.
///
/// Inputs:
/// - Temporary Terlan modules containing unsupported constructor
///   default/vararg/arity shapes plus requested phase-manifest paths.
///
/// Output:
/// - Test assertions only; each command run must fail and write a phase
///   manifest with the parse diagnostic and skipped CoreIR phase.
///
/// Transformation:
/// - Runs each constructor edge-case source through command-level
///   `terlc check --emit-phase-manifest` and confirms A0.32 constructor
///   diagnostics are visible before resolution, typecheck, or backend
///   emission can run.
#[test]
fn run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest() {
    let cases = [
            (
                "constructor_varargs_not_last",
                "constructor varargs parameter must be last",
                "module bad.constructor_varargs_not_last.\n\nconstructor Bad {\n  (...items: Int, last: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_default_not_trailing",
                "constructor default parameters must be trailing",
                "module bad.constructor_default_not_trailing.\n\nconstructor Bad {\n  (first: Int = 1, second: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_varargs_with_default",
                "constructor varargs parameters cannot have defaults",
                "module bad.constructor_varargs_with_default.\n\nconstructor Bad {\n  (...items: Int = []): Bad -> 1\n}.\n",
            ),
            (
                "constructor_duplicate_varargs_clauses",
                "constructor has ambiguous varargs clauses",
                "module bad.constructor_duplicate_varargs_clauses.\n\nconstructor Bad {\n  (...items: Int): Bad -> 1;\n  (first: Int, ...rest: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_overlapping_default_arity",
                "constructor has ambiguous arity clauses",
                "module bad.constructor_overlapping_default_arity.\n\nconstructor Bad {\n  (first: Int): Bad -> 1;\n  (first: Int, second: Int = 1): Bad -> 1\n}.\n",
            ),
        ];

    for (fixture_name, expected_message, source_text) in cases {
        let dir = make_temp_dir(&format!("check_single_file_{fixture_name}_rejected"));
        let source = dir.join(format!("{fixture_name}.terl"));
        fs::write(&source, source_text).expect("write constructor edge-case source");
        let manifest = dir.join(format!("{fixture_name}.phase-manifest.json"));

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

        assert_ne!(exit, ExitCode::SUCCESS, "{fixture_name}");
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(
            manifest_text.contains(r#""name":"parse","status":"error""#),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(expected_message),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(r#""name":"resolve","status":"skipped""#),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(r#""name":"core","status":"skipped""#),
            "{fixture_name}: {manifest_text}"
        );
    }
}

/// Verifies unsupported function declaration and clause shapes fail early.
///
/// Inputs:
/// - Temporary Terlan modules containing function varargs, mismatched
///   secondary clause names, and mismatched secondary clause arities.
///
/// Output:
/// - Test assertions only; each command run must fail and write a phase
///   manifest with the parse diagnostic and skipped CoreIR phase.
///
/// Transformation:
/// - Runs each unsupported function source through command-level
///   `terlc check --emit-phase-manifest` and confirms the A0.32
///   function/clauses decision is visible before semantic lowering or
///   backend emission.
#[test]
fn run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest() {
    let cases = [
        (
            "function_varargs_param",
            "function varargs parameters are not supported in Terlan 0.0.1",
            "module bad.function_varargs_param.\n\npub sum(...items: Int): Int ->\n  0.\n",
        ),
        (
            "function_clause_name_mismatch",
            "expected Dot",
            "module bad.function_clause_name_mismatch.\n\nvalue(0) -> 0;\nother(1) -> 1.\n",
        ),
        (
            "function_clause_arity_mismatch",
            "clause for value has arity 2, expected 1",
            "module bad.function_clause_arity_mismatch.\n\nvalue(0) -> 0;\nvalue(1, 2) -> 1.\n",
        ),
    ];

    for (fixture_name, expected_message, source_text) in cases {
        let dir = make_temp_dir(&format!("check_single_file_{fixture_name}_rejected"));
        let source = dir.join(format!("{fixture_name}.terl"));
        fs::write(&source, source_text).expect("write function edge-case source");
        let manifest = dir.join(format!("{fixture_name}.phase-manifest.json"));

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

        assert_ne!(exit, ExitCode::SUCCESS, "{fixture_name}");
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(
            manifest_text.contains(r#""name":"parse","status":"error""#),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(expected_message),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(r#""name":"resolve","status":"skipped""#),
            "{fixture_name}: {manifest_text}"
        );
        assert!(
            manifest_text.contains(r#""name":"core","status":"skipped""#),
            "{fixture_name}: {manifest_text}"
        );
    }
}

/// Verifies generic `check` rejects unresolved external template bodies.
///
/// Inputs:
/// - A temporary Terlan module that declares and instantiates an external
///   template whose source file is absent.
///
/// Output:
/// - Test assertions only; the command must fail and write a phase
///   manifest with a typecheck diagnostic and skipped CoreIR phase.
///
/// Transformation:
/// - Runs `terlc check --emit-phase-manifest` through the formal pipeline
///   and confirms template body resolution is validated before CoreIR or
///   backend emission unless a command owns template loading/rendering.
#[test]
fn run_check_single_file_rejects_unresolved_template_body_before_core_phase() {
    let dir = make_temp_dir("check_single_file_template_body_rejected");
    let source = dir.join("template_body.terl");
    fs::write(
            &source,
            "module template_body.\n\ntemplate Page from \"./templates/missing.terl.html\" {\n  title: Text\n}.\n\npub home(): Html[Never] ->\n  Page{ title = \"Home\" }.\n",
        )
        .expect("write unresolved template source");
    let manifest = dir.join("template_body.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
    assert!(manifest_text.contains("failed to read template"));
    assert!(manifest_text.contains("missing.terl.html"));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
}

/// Verifies raw macro primary expressions fail before semantic phases.
///
/// Inputs:
/// - A temporary single-file Terlan module containing a raw macro expression
///   as a function body and a requested phase-manifest output path.
///
/// Output:
/// - Test assertion only; the command must fail and write a phase manifest
///   with a macro-expansion diagnostic and skipped resolve/typecheck/CoreIR
///   phases.
///
/// Transformation:
/// - Runs the command-level check path and confirms raw macro syntax is
///   preserved by parsing but cannot leak into backend lowering without an
///   explicit macro-resolution implementation.
#[test]
fn run_check_single_file_rejects_raw_macro_primary_before_phase_manifest() {
    let dir = make_temp_dir("check_single_file_raw_macro_primary_rejected");
    let source = dir.join("raw_macro_primary.terl");
    fs::write(
        &source,
        "module raw_macro_primary.\n\npub query(): Dynamic ->\n  sql{select * from users}.\n",
    )
    .expect("write raw macro primary source");
    let manifest = dir.join("raw_macro_primary.phase-manifest.json");

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

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"macro_expansion","status":"error""#));
    assert!(
        manifest_text.contains("raw macro expression `sql` requires macro resolution"),
        "{manifest_text}"
    );
    assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
}
