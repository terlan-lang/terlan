use super::*;
use crate::commands::process_runner::run_command_with_timeout;
use crate::commands::test::discovery::TestKind;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn native_declarations_execute_inferred_regex_bindings_on_fresh_and_cached_images() {
    let dir = crate::support::test_fs::TestDirectory::new("native_declarations", "regex_execution");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std/regex/RegexTest.terl");
    let state = CliState {
        out_dir: dir.path().join("out"),
        cache_dir: Some(dir.path().join("cache")),
        native_policy: NativePolicy::NativeBoundaryOptional,
        ..CliState::default()
    };
    let command = || CliCommand {
        verb: Some("test".into()),
        args: vec![
            source.to_string_lossy().into_owned(),
            "--target".into(),
            "terlan-vm".into(),
            "--name".into(),
            "matching_line_numbers_preserves_one_based_source_order".into(),
            "--name".into(),
            "compile_reports_invalid_pattern".into(),
        ],
    };
    assert_eq!(run(command(), state.clone()), ExitCode::SUCCESS);
    assert_eq!(run(command(), state), ExitCode::SUCCESS);
}

/// Returns whether a Terlan source contains public executable functions.
///
/// Inputs:
/// - `source`: Terlan module source text.
///
/// Output:
/// - `true` when a line looks like a public function or receiver method.
///
/// Transformation:
/// - Uses line starts to distinguish executable declarations from public type
///   declarations without parsing the full module in this unit test.
fn has_executable_public_function(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("pub (") || {
            trimmed.starts_with("pub ")
                && !trimmed.starts_with("pub mut ")
                && !trimmed.starts_with("pub type ")
                && !trimmed.starts_with("pub opaque type ")
                && !trimmed.starts_with("pub struct ")
                && !trimmed.starts_with("pub trait ")
                && trimmed.contains('(')
        }
    })
}

/// Verifies only receiver-local `mut` marks public mutable receiver methods.
#[test]
fn executable_public_function_scan_rejects_pub_mut_receiver_spelling() {
    assert!(has_executable_public_function(
        "pub (mut values: Vector[T]) push(value: T): Unit ->\n    Unit.\n"
    ));
    assert!(!has_executable_public_function(
        "pub mut (values: Vector[T]) push(value: T): Unit ->\n    Unit.\n"
    ));
}

/// Verifies test manifest JSON serialization.
///
/// Inputs:
/// - Synthetic discovered test metadata and a temporary output path.
///
/// Output:
/// - Assertions over decoded JSON fields.
///
/// Transformation:
/// - Writes a manifest file, decodes it through `serde_json`, then removes
///   the temporary file.
#[test]
fn write_test_manifest_records_source_target_and_spans() {
    let path = std::env::temp_dir().join(format!(
        "terlan_test_manifest_unit_{}_{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    write_test_manifest(
        &path,
        "tests/SampleTest.terl",
        "tests.SampleTest",
        "erlang",
        "erlang",
        &[DiscoveredTest {
            name: "sample".to_string(),
            kind: TestKind::Test,
            span_start: 12,
            span_end: 34,
            literal_bool_result: Some(true),
        }],
    )
    .expect("write manifest");

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("manifest text"))
            .expect("manifest json");
    let _ = fs::remove_file(&path);

    assert_eq!(json["source_path"], "tests/SampleTest.terl");
    assert_eq!(json["module_name"], "tests.SampleTest");
    assert_eq!(json["target"], "erlang");
    assert_eq!(json["target_profile"], "erlang");
    assert_eq!(json["tests"][0]["name"], "sample");
    assert_eq!(json["tests"][0]["kind"], "test");
    assert_eq!(json["tests"][0]["span_start"], 12);
    assert_eq!(json["tests"][0]["span_end"], 34);
}

/// Verifies test result manifest JSON serialization.
///
/// Inputs:
/// - Synthetic execution report and a temporary output path.
///
/// Output:
/// - Assertions over decoded JSON fields.
///
/// Transformation:
/// - Writes a result manifest file, decodes it through `serde_json`, then
///   removes the temporary file.
#[test]
fn write_test_result_manifest_records_outcomes_and_spans() {
    let path = std::env::temp_dir().join(format!(
        "terlan_test_result_manifest_unit_{}_{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let report = TestRunReport {
        native_image_digest: None,
        passed: 1,
        failed: 1,
        covered_callables: Default::default(),
        results: vec![
            TestRunResult {
                entered_native_callables: Default::default(),
                name: "passes".to_string(),
                kind: TestKind::Test,
                status: TestRunStatus::Passed,
                message: None,
                execution_nanoseconds: 11,
                benchmark_samples: None,
                benchmark_min_nanoseconds: None,
                benchmark_p95_nanoseconds: None,
                span_start: 10,
                span_end: 20,
            },
            TestRunResult {
                entered_native_callables: Default::default(),
                name: "fails".to_string(),
                kind: TestKind::Test,
                status: TestRunStatus::Failed,
                message: Some("assertion returned false".to_string()),
                execution_nanoseconds: 13,
                benchmark_samples: None,
                benchmark_min_nanoseconds: None,
                benchmark_p95_nanoseconds: None,
                span_start: 30,
                span_end: 40,
            },
        ],
    };
    write_test_result_manifest(
        &path,
        "tests/SampleTest.terl",
        "tests.SampleTest",
        "erlang",
        "erlang",
        &report,
        None,
    )
    .expect("write result manifest");

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("manifest text"))
            .expect("manifest json");
    let _ = fs::remove_file(&path);

    assert_eq!(json["source_path"], "tests/SampleTest.terl");
    assert_eq!(json["passed"], 1);
    assert_eq!(json["failed"], 1);
    assert_eq!(json["tests"][0]["name"], "passes");
    assert_eq!(json["tests"][0]["kind"], "test");
    assert_eq!(json["tests"][0]["status"], "passed");
    assert!(json["tests"][0]["message"].is_null());
    assert_eq!(json["tests"][0]["execution_nanoseconds"], 11);
    assert!(json["tests"][0]["benchmark_samples"].is_null());
    assert_eq!(json["tests"][1]["name"], "fails");
    assert_eq!(json["tests"][1]["status"], "failed");
    assert_eq!(json["tests"][1]["message"], "assertion returned false");
    assert_eq!(json["tests"][1]["execution_nanoseconds"], 13);
    assert_eq!(json["tests"][1]["span_start"], 30);
}

/// Verifies validation-only JS reports preserve test metadata.
///
/// Inputs:
/// - Synthetic discovered test metadata.
///
/// Output:
/// - A not-executed report with validation messages and original spans.
///
/// Transformation:
/// - Converts discovered source tests into runner result entries without
///   executing target code.
#[test]
fn validation_report_does_not_count_compilation_as_execution() {
    let report = validation_report(&[DiscoveredTest {
        name: "smoke".to_string(),
        kind: TestKind::Test,
        span_start: 7,
        span_end: 19,
        literal_bool_result: Some(true),
    }]);

    assert_eq!(report.passed, 0);
    assert_eq!(report.failed, 0);
    assert!(!report.is_success());
    assert_eq!(report.results[0].name, "smoke");
    assert_eq!(report.results[0].status, TestRunStatus::NotExecuted);
    assert_eq!(
        report.results[0].message.as_deref(),
        Some("validated without runtime execution")
    );
    assert_eq!(report.results[0].span_start, 7);
    assert_eq!(report.results[0].span_end, 19);
}

/// Verifies repeated exact test selection keeps the complete named subset.
///
/// Inputs:
/// - Synthetic discovered tests and selectors `first` and `second`.
///
/// Output:
/// - A two-element selected test list in declaration order.
///
/// Transformation:
/// - Applies the same exact-name filter used by `terlc test --name`.
#[test]
fn select_tests_keeps_exact_selected_tests() {
    let selected = select_tests(
        vec![
            DiscoveredTest {
                name: "first".to_string(),
                kind: TestKind::Test,
                span_start: 1,
                span_end: 2,
                literal_bool_result: Some(true),
            },
            DiscoveredTest {
                name: "second".to_string(),
                kind: TestKind::Test,
                span_start: 3,
                span_end: 4,
                literal_bool_result: Some(true),
            },
        ],
        &["first".to_string(), "second".to_string()],
        "tests/SampleTest.terl",
        TestKind::Test,
    )
    .expect("selected tests");

    assert_eq!(
        selected
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

/// Verifies missing exact test selection produces a clear diagnostic.
///
/// Inputs:
/// - Synthetic discovered tests and missing selector.
///
/// Output:
/// - Stable missing-test diagnostic.
///
/// Transformation:
/// - Applies the same exact-name filter used by `terlc test --name`.
#[test]
fn select_tests_rejects_missing_test_name() {
    let error = select_tests(
        vec![DiscoveredTest {
            name: "present".to_string(),
            kind: TestKind::Test,
            span_start: 1,
            span_end: 2,
            literal_bool_result: Some(true),
        }],
        &["present".to_string(), "missing".to_string()],
        "tests/SampleTest.terl",
        TestKind::Test,
    )
    .expect_err("missing selector");

    assert_eq!(
        error,
        "no @test declaration named `missing` found in tests/SampleTest.terl"
    );
}

/// Verifies JS validation writes source and result manifests.
///
/// Inputs:
/// - A temporary JS-compatible Terlan test module and command-local manifest
///   output flags.
///
/// Output:
/// - A nonzero command exit and manifest entries marked not executed.
///
/// Transformation:
/// - Runs the public test command entry point with `--target js`, then checks
///   that validation-only metadata is serialized with the JS target identity.
#[test]
fn run_js_tests_writes_incomplete_manifests_and_fails_closed() {
    let root = std::env::temp_dir().join(format!(
        "terlan_js_test_manifest_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp js test dir");
    let source_path = root.join("ManifestTest.terl");
    let manifest_path = root.join("test-manifest.json");
    let result_path = root.join("test-results.json");
    fs::write(
        &source_path,
        "module tests.js.ManifestTest.\n\n@test\npub smoke(): Bool ->\n    false.\n",
    )
    .expect("write js validation test source");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--target".to_string(),
                "js".to_string(),
                "--emit-test-manifest".to_string(),
                manifest_path.to_string_lossy().into_owned(),
                "--emit-test-result-manifest".to_string(),
                result_path.to_string_lossy().into_owned(),
            ],
        },
        CliState {
            target_profile: TargetProfile::JsShared,
            ..CliState::default()
        },
    );
    assert_eq!(exit_code, ExitCode::from(1));

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("manifest text"))
            .expect("manifest json");
    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result_path).expect("result text"))
            .expect("result json");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(manifest["module_name"], "tests.js.ManifestTest");
    assert_eq!(manifest["target"], "js");
    assert_eq!(manifest["target_profile"], "js.shared");
    assert_eq!(manifest["tests"][0]["name"], "smoke");
    assert_eq!(results["target"], "js");
    assert_eq!(results["target_profile"], "js.shared");
    assert_eq!(results["passed"], 0);
    assert_eq!(results["failed"], 0);
    assert_eq!(results["not_executed"], 1);
    assert_eq!(results["tests"][0]["status"], "not_executed");
    assert!(results["native_callable_evidence"].is_null());
    assert_eq!(
        results["tests"][0]["entered_native_callables"],
        serde_json::json!([])
    );
    assert_eq!(
        results["tests"][0]["message"],
        "validated without runtime execution"
    );
}

/// Verifies scalar Terlan VM tests execute from a native image.
///
/// Inputs:
/// - A temporary `*Test.terl` module with multiple passing boolean `@test`
///   declarations.
///
/// Output:
/// - Successful command exit code.
///
/// Transformation:
/// - Runs two repeated exact-name selectors through the public command entry
///   point and verifies that their shared native closure emits one `.tvm`
///   image without transitional JSON.
#[test]
fn run_terlan_vm_tests_executes_bool_test() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_exec_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp vm test dir");
    let source_path = root.join("VmSmokeTest.terl");
    fs::write(
        &source_path,
        r#"module tests.vm.VmSmokeTest.

pub add_one(value: Int): Int ->
    value + 1.

pub actual_values(): List[Int] ->
    [add_one(1)].

pub expected_values(): List[Int] ->
    [2].

@test
pub smoke(): Bool ->
    1 + 2 == 3.

@test
pub float_smoke(): Bool ->
    1.5 + 2.25 > 3.0.

@test
pub managed_smoke(): Bool ->
    actual_values() == expected_values().
"#,
    )
    .expect("write vm test source");

    let out_dir = root.join("build");
    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--target".to_string(),
                "terlan-vm".to_string(),
                "--name".to_string(),
                "smoke".to_string(),
                "--name".to_string(),
                "managed_smoke".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            cache_dir: Some(root.join("cache")),
            ..CliState::default()
        },
    );
    let image = out_dir.join("test-aot/tests_vm_VmSmokeTest/vm/tests_vm_VmSmokeTest.tvm");
    assert!(
        image.is_file(),
        "missing native test image: {}",
        image.display()
    );
    assert!(
        !image.with_extension("tvm.json").exists(),
        "native test execution emitted transitional JSON"
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

#[test]
fn run_wasm_tests_executes_scalar_contract_from_source() {
    let root = std::env::temp_dir().join(format!(
        "terlan_wasm_test_exec_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp Wasm test dir");
    let source_path = root.join("AbiTest.terl");
    fs::write(
        &source_path,
        "module tests.wasm.AbiTest.\n\nimport std.wasm.Abi.{F32, F64, I32, I64}.\n\npub identity_i32(value: I32): I32 -> value.\npub identity_i64(value: I64): I64 -> value.\npub identity_f32(value: F32): F32 -> value.\npub identity_f64(value: F64): F64 -> value.\n\n@test\npub scalar_contract_executes(): Bool -> 20 + 22 == 42.\n",
    )
    .expect("write Wasm test source");
    let state = CliState {
        out_dir: root.join("build"),
        ..CliState::default()
    };

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--target".to_string(),
                "wasm".to_string(),
            ],
        },
        state,
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Verifies bare `terlc test <file>` uses the Terlan VM by default.
///
/// Inputs:
/// - A temporary `*Test.terl` module with one VM-supported boolean `@test`.
///
/// Output:
/// - Successful command exit code without passing `--target`.
///
/// Transformation:
/// - Runs the public test command entry point through the default target
///   selection, proving the implicit lane is the compiler-owned VM rather than
///   Vm.
#[test]
fn run_test_defaults_to_terlan_vm_execution() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_default_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp default vm test dir");
    let source_path = root.join("DefaultVmTest.terl");
    fs::write(
        &source_path,
        "module tests.vm.DefaultVmTest.\n\n@test\npub smoke(): Bool ->\n    2 + 2 == 4.\n",
    )
    .expect("write default vm test source");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![source_path.to_string_lossy().into_owned()],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Verifies project-directory tests use the VM default lane.
///
/// Inputs:
/// - A temporary Terlan project with `terlan.toml`, one `src` module, and one
///   `tests` module importing that source-root module.
///
/// Output:
/// - Successful command exit code without passing `--target`.
///
/// Transformation:
/// - Runs the public `terlc test <project>/tests` entry point so project
///   source-root preparation and directory test execution both happen on the
///   compiler-owned VM default lane.
#[test]
fn run_project_directory_tests_default_to_vm_and_prepare_source_roots() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_project_test_default_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src/app")).expect("create project source dir");
    fs::create_dir_all(root.join("tests/app")).expect("create project test dir");
    fs::write(
        root.join("terlan.toml"),
        r#"[package]
name = "app"
version = "0.0.0"

[build]
source_roots = ["src"]
"#,
    )
    .expect("write project manifest");
    fs::write(
        root.join("src/app/Math.terl"),
        concat!(
            "module app.Math.\n\n",
            "import std.vm.Bytes.\n\n",
            "pub add(x: Int, y: Int): Int ->\n",
            "    x + y.\n\n",
            "pub values(): List[Int] ->\n",
            "    [4, 5].\n\n",
            "pub second(): Int ->\n",
            "    values()[1].\n\n",
            "pub bytes_second(): Int ->\n",
            "    Bytes.from_list(values()).to_list()[1].\n",
        ),
    )
    .expect("write project source");
    fs::write(
        root.join("tests/app/MathTest.terl"),
        concat!(
            "module app.MathTest.\n\n",
            "import app.Math.\n\n",
            "@test\n",
            "pub project_source_import_is_available(): Bool ->\n",
            "    Math.add(2, 3) == 5.\n\n",
            "@test\n",
            "pub project_source_list_index_is_available(): Bool ->\n",
            "    Math.second() == 5.\n\n",
            "@test\n",
            "pub project_source_bytes_round_trip_is_available(): Bool ->\n",
            "    Math.bytes_second() == 5.\n",
        ),
    )
    .expect("write project test");
    fs::write(
        root.join("tests/app/SecondTest.terl"),
        concat!(
            "module app.SecondTest.\n\n",
            "import app.Math.\n\n",
            "@test\n",
            "pub second_module_uses_shared_source(): Bool ->\n",
            "    Math.add(20, 22) == 42.\n",
        ),
    )
    .expect("write second project test");

    let command = || CliCommand {
        verb: Some("test".to_string()),
        args: vec![root.join("tests").to_string_lossy().into_owned()],
    };
    let state = CliState {
        out_dir: root.join("out"),
        cache_dir: Some(root.join(".terlan")),
        ..CliState::default()
    };
    let exit_code = run(command(), state.clone());
    let suite_workspaces = fs::read_dir(root.join("out/test-aot"))
        .expect("shared suite output")
        .count();
    assert_eq!(suite_workspaces, 1, "directory tests must build one suite");

    fs::remove_dir_all(root.join("out/test-aot")).expect("remove suite output");
    let cached_exit_code = run(command(), state.clone());
    assert!(
        !root.join("out/test-aot").exists(),
        "unchanged test modules must not rebuild a native suite"
    );
    fs::write(
        root.join("tests/app/SecondTest.terl"),
        concat!(
            "module app.SecondTest.\n\n",
            "import app.Math.\n\n",
            "@test\n",
            "pub second_module_uses_shared_source(): Bool ->\n",
            "    Math.add(21, 21) == 42.\n",
        ),
    )
    .expect("change one test module");
    let changed_exit_code = run(command(), state);
    assert!(
        root.join("out/test-aot").is_dir(),
        "a changed test module must rebuild the suite"
    );
    assert_eq!(
        fs::read_dir(root.join(".terlan/test-results/terlan-vm"))
            .expect("test result cache")
            .count(),
        3,
        "changing one test must preserve the sibling module result"
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
    assert_eq!(cached_exit_code, ExitCode::SUCCESS);
    assert_eq!(changed_exit_code, ExitCode::SUCCESS);
}

/// Tests inside a source root have one application owner, even with overlapping roots.
#[test]
fn run_project_test_inside_source_root_compiles_active_module_once() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_source_owner_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos(),
    ));
    fs::create_dir_all(root.join("src/app")).expect("source root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\"src\", \"src/.\"]\n",
    )
    .expect("project manifest");
    fs::write(
        root.join("src/app/Math.terl"),
        "module app.Math.\npub answer(): Int -> 42.\n",
    )
    .expect("support source");
    let test = root.join("src/app/MathTest.terl");
    fs::write(
        &test,
        "module app.MathTest.\nimport app.Math.\n@test\npub answer_is_correct(): Bool -> Math.answer() == 42.\n",
    )
    .expect("test source");
    let result = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![test.to_string_lossy().into_owned()],
        },
        CliState {
            out_dir: root.join("out"),
            ..CliState::default()
        },
    );
    fs::remove_dir_all(root).expect("remove private project and AOT workspace");
    assert_eq!(result, ExitCode::SUCCESS);
}

/// Verifies focused project tests compile only their transitive import closure.
#[test]
fn focused_project_test_skips_unreachable_source_modules() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_focused_test_closure_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos(),
    ));
    fs::create_dir_all(root.join("src/app")).expect("source root");
    fs::create_dir_all(root.join("tests/app")).expect("test root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("project manifest");
    fs::write(
        root.join("src/app/Leaf.terl"),
        "module app.Leaf.\npub answer(): Int -> 42.\n",
    )
    .expect("transitive source");
    fs::write(
        root.join("src/app/Used.terl"),
        "module app.Used.\nimport app.Leaf.\npub answer(): Int -> Leaf.answer().\n",
    )
    .expect("direct source");
    fs::write(
        root.join("src/app/Unused.terl"),
        "module app.Unused.\npub broken(): Int -> \"not an int\".\n",
    )
    .expect("unreachable invalid source");
    let test = root.join("tests/app/FocusedTest.terl");
    fs::write(
        &test,
        concat!(
            "module app.FocusedTest.\n",
            "import app.Used.\n\n",
            "@test\n",
            "pub imported_closure_executes(): Bool -> Used.answer() == 42.\n",
        ),
    )
    .expect("focused test");
    let second_test = root.join("tests/app/SecondFocusedTest.terl");
    fs::write(
        &second_test,
        concat!(
            "module app.SecondFocusedTest.\n",
            "import app.Used.\n\n",
            "@test\n",
            "pub answer_is_still_correct(): Bool -> Used.answer() == 42.\n",
        ),
    )
    .expect("second focused test");

    let result = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                test.to_string_lossy().into_owned(),
                second_test.to_string_lossy().into_owned(),
            ],
        },
        CliState {
            out_dir: root.join("out"),
            cache_dir: Some(root.join(".terlan")),
            ..CliState::default()
        },
    );
    fs::remove_dir_all(root).expect("remove focused test fixture");
    assert_eq!(result, ExitCode::SUCCESS);
}

/// Verifies focused native imports with nested generic results stay bounded.
///
/// Inputs:
/// - A native-declaration provider, one reachable domain module returning
///   `Result[Option[Batch], Failure]`, and one unreachable invalid module.
/// - A focused test importing the native operation and every nested nominal
///   payload type explicitly.
///
/// Output:
/// - Successful focused test execution without compiling the unreachable
///   source module.
///
/// Transformation:
/// - Exercises the package-native/type-inference shape that formerly retained
///   and cloned a complete project graph, while the invalid sentinel proves
///   closure compilation remains limited to the selected test's imports.
#[test]
fn focused_native_import_nested_generic_test_uses_reachable_closure() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_focused_native_nested_generic_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos(),
    ));
    fs::create_dir_all(root.join("src/app")).expect("application source root");
    fs::create_dir_all(root.join("src/native_probe")).expect("native source root");
    fs::create_dir_all(root.join("tests/app")).expect("focused test root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("project manifest");
    fs::write(
        root.join("src/native_probe/Frame.terl"),
        concat!(
            "module native_probe.Frame.\n\n",
            "pub opaque type Frame.\n\n",
            "@compiler.native {native_probe.frame.dispose}\n",
            "pub (_frame: Frame) dispose(): Unit ->\n",
            "    native.\n",
        ),
    )
    .expect("native declaration provider");
    fs::write(
        root.join("src/app/Batch.terl"),
        concat!(
            "module app.Batch.\n\n",
            "import std.core.Option.{Some}.\n",
            "import std.core.Result.{Ok}.\n\n",
            "import type std.collections.List.{List}.\n",
            "import type std.core.Option.{Option}.\n",
            "import type std.core.Result.{Result}.\n",
            "import type native_probe.Frame.{Frame}.\n\n",
            "pub struct Bar {\n",
            "    sequence: Int,\n",
            "}.\n\n",
            "pub struct Batch {\n",
            "    completed_bars: List[Bar],\n",
            "}.\n\n",
            "pub (batch: Batch) completed_bars(): List[Bar] ->\n",
            "    batch.completed_bars.\n\n",
            "pub type Failure = Atom[\"failed\"].\n\n",
            "pub inspect(_frame: Frame): Result[Option[Batch], Failure] ->\n",
            "    Ok(Some(Batch { completed_bars: [Bar { sequence: 7 }] })).\n\n",
            "pub sample(): Result[Option[Batch], Failure] ->\n",
            "    Ok(Some(Batch { completed_bars: [Bar { sequence: 7 }] })).\n",
        ),
    )
    .expect("nested generic domain source");
    fs::write(
        root.join("src/app/Unused.terl"),
        "module app.Unused.\npub broken(): Int -> \"unreachable\".\n",
    )
    .expect("unreachable invalid source");
    let test = root.join("tests/app/FocusedNativeTest.terl");
    fs::write(
        &test,
        concat!(
            "module app.FocusedNativeTest.\n\n",
            "import app.Batch.{sample}.\n",
            "import native_probe.Frame.{dispose}.\n",
            "import std.core.Option.{Some}.\n",
            "import std.core.Result.{Ok}.\n\n",
            "import type app.Batch.{Bar, Batch, Failure}.\n",
            "import type native_probe.Frame.{Frame}.\n",
            "import type std.collections.List.{List}.\n",
            "import type std.core.Option.{Option}.\n",
            "import type std.core.Result.{Result}.\n\n",
            "@test\n",
            "pub nested_result_preserves_nominal_payload(): Bool ->\n",
            "    when {\n",
            "        Ok(Some(batch)) <- sample();\n",
            "        [bar] <- batch.completed_bars()\n",
            "    } else {\n",
            "        _ -> false\n",
            "    };\n",
            "    bar.sequence == 7.\n",
        ),
    )
    .expect("focused native test");

    let result = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![test.to_string_lossy().into_owned()],
        },
        CliState {
            out_dir: root.join("out"),
            cache_dir: Some(root.join(".terlan")),
            ..CliState::default()
        },
    );
    fs::remove_dir_all(root).expect("remove focused native fixture");
    assert_eq!(result, ExitCode::SUCCESS);
}

/// Verifies project tests resolve local package dependencies through build semantics.
///
/// Inputs:
/// - A root project with one local path dependency.
/// - A root source module importing the dependency and a test importing the
///   root module.
///
/// Output:
/// - Successful VM test execution.
///
/// Transformation:
/// - Exercises dependency-first source preparation through `terlc test`,
///   proving package tests share the dependency closure used by build and run.
#[test]
fn run_project_tests_prepare_local_path_dependency_roots() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_project_dependency_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let app = root.join("app");
    let dependency = root.join("math_dependency");
    fs::create_dir_all(app.join("src/app")).expect("create app source dir");
    fs::create_dir_all(app.join("tests/app")).expect("create app test dir");
    fs::create_dir_all(dependency.join("src/math_dependency"))
        .expect("create dependency source dir");
    fs::write(
        app.join("terlan.toml"),
        r#"[package]
name = "app"
version = "0.0.0"

[build]
source_roots = ["src"]

[dependencies]
math_dependency = { path = "../math_dependency" }
"#,
    )
    .expect("write app manifest");
    fs::write(
        dependency.join("terlan.toml"),
        r#"[package]
name = "math-dependency"
version = "0.0.0"
namespace = "math_dependency"

[build]
source_roots = ["src"]
artifact = "library"
"#,
    )
    .expect("write dependency manifest");
    fs::write(
        dependency.join("src/math_dependency/Math.terl"),
        "module math_dependency.Math.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("write dependency source");
    fs::write(
        app.join("src/app/Calculator.terl"),
        "module app.Calculator.\n\nimport math_dependency.Math.{add}.\n\npub total(): Int ->\n    add(2, 3).\n",
    )
    .expect("write app source");
    fs::write(
        app.join("tests/app/CalculatorTest.terl"),
        "module app.CalculatorTest.\n\nimport app.Calculator.{total}.\n\n@test\npub dependency_is_available(): Bool ->\n    total() == 5.\n",
    )
    .expect("write app test");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![app.join("tests").to_string_lossy().into_owned()],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Verifies declaration coverage belongs to the selected package.
///
/// Inputs:
/// - An application with a local dependency containing one uncovered function.
/// - Package-local tests for both the application and dependency.
///
/// Output:
/// - The application reaches 100% coverage without owning dependency declarations.
/// - The dependency's own 100% gate detects its uncovered declaration.
///
/// Transformation:
/// - Keeps dependency roots in the compilation closure while restricting each
///   coverage denominator to that package's `[build].source_roots`.
#[test]
fn declaration_coverage_excludes_dependency_owned_sources() {
    let root = std::env::temp_dir().join(format!(
        "terlan_package_coverage_ownership_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let app = root.join("app");
    let dependency = root.join("math_dependency");
    fs::create_dir_all(app.join("src/app")).expect("create app source dir");
    fs::create_dir_all(app.join("tests/app")).expect("create app test dir");
    fs::create_dir_all(dependency.join("src/math_dependency"))
        .expect("create dependency source dir");
    fs::create_dir_all(dependency.join("tests/math_dependency"))
        .expect("create dependency test dir");
    fs::write(
        app.join("terlan.toml"),
        r#"[package]
name = "app"
version = "0.0.0"

[build]
source_roots = ["src"]

[dependencies]
math_dependency = { path = "../math_dependency" }
"#,
    )
    .expect("write app manifest");
    fs::write(
        dependency.join("terlan.toml"),
        r#"[package]
name = "math-dependency"
version = "0.0.0"
namespace = "math_dependency"

[build]
source_roots = ["src"]
artifact = "library"
"#,
    )
    .expect("write dependency manifest");
    fs::write(
        dependency.join("src/math_dependency/Math.terl"),
        "module math_dependency.Math.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n\npub uncovered_dependency(value: Int): Int ->\n    value + 1.\n\npub choose(covered: Bool): Int ->\n    if {\n        covered -> add(2, 3);\n        _ -> uncovered_dependency(4)\n    }.\n",
    )
    .expect("write dependency source");
    fs::write(
        app.join("src/app/Calculator.terl"),
        "module app.Calculator.\n\nimport math_dependency.Math.{add}.\n\npub total(): Int ->\n    add(2, 3).\n",
    )
    .expect("write app source");
    fs::write(
        app.join("tests/app/CalculatorTest.terl"),
        "module app.CalculatorTest.\n\nimport app.Calculator.{total}.\n\n@test\npub application_is_covered(): Bool ->\n    total() == 5.\n",
    )
    .expect("write app test");
    fs::write(
        dependency.join("tests/math_dependency/MathTest.terl"),
        "module math_dependency.MathTest.\n\nimport math_dependency.Math.{choose}.\n\n@test\npub dependency_add_is_covered(): Bool ->\n    choose(true) == 5.\n",
    )
    .expect("write dependency test");

    let coverage_args = |tests: PathBuf| CliCommand {
        verb: Some("test".to_string()),
        args: vec![
            tests.to_string_lossy().into_owned(),
            "--coverage".to_string(),
            "--coverage-threshold".to_string(),
            "100".to_string(),
        ],
    };
    let app_result = run(coverage_args(app.join("tests")), CliState::default());
    let dependency_result = run(coverage_args(dependency.join("tests")), CliState::default());
    let _ = fs::remove_dir_all(&root);

    assert_eq!(app_result, ExitCode::SUCCESS);
    assert_eq!(dependency_result, ExitCode::from(1));
}

/// Verifies a focused coverage gate can name its production ownership boundary.
///
/// Inputs:
/// - One subject module importing a helper module with an uncovered declaration.
/// - A focused test that executes every declaration in the subject module.
///
/// Output:
/// - Package-wide coverage detects the uncovered helper declaration.
/// - Explicit subject ownership reaches 100% without weakening execution.
///
/// Transformation:
/// - Keeps the helper in the compilation closure while restricting the
///   denominator to the production source paired with the focused test.
#[test]
fn focused_coverage_sources_exclude_unowned_transitive_declarations() {
    let root = std::env::temp_dir().join(format!(
        "terlan_focused_coverage_ownership_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src/app")).expect("create app source dir");
    fs::create_dir_all(root.join("tests/app")).expect("create app test dir");
    fs::write(
        root.join("terlan.toml"),
        r#"[package]
name = "app"
version = "0.0.0"

[build]
source_roots = ["src"]
"#,
    )
    .expect("write app manifest");
    fs::write(
        root.join("src/app/Support.terl"),
        "module app.Support.\n\npub increment(value: Int): Int ->\n    value + 1.\n\npub uncovered(value: Int): Int ->\n    value - 1.\n\npub choose(covered: Bool, value: Int): Int ->\n    if {\n        covered -> increment(value);\n        _ -> uncovered(value)\n    }.\n",
    )
    .expect("write support source");
    let subject = root.join("src/app/Subject.terl");
    fs::write(
        &subject,
        "module app.Subject.\n\nimport app.Support.{choose}.\n\npub answer(): Int ->\n    choose(true, 41).\n",
    )
    .expect("write subject source");
    let test = root.join("tests/app/SubjectTest.terl");
    fs::write(
        &test,
        "module app.SubjectTest.\n\nimport app.Subject.{answer}.\n\n@test\npub answer_is_covered(): Bool ->\n    answer() == 42.\n",
    )
    .expect("write subject test");

    let command = |coverage_source: Option<&std::path::Path>| {
        let mut args = vec![
            test.to_string_lossy().into_owned(),
            "--coverage".to_string(),
            "--coverage-threshold".to_string(),
            "100".to_string(),
        ];
        if let Some(source) = coverage_source {
            args.push("--coverage-source".to_string());
            args.push(source.to_string_lossy().into_owned());
        }
        CliCommand {
            verb: Some("test".to_string()),
            args,
        }
    };
    let package_result = run(command(None), CliState::default());
    let focused_result = run(command(Some(&subject)), CliState::default());
    let _ = fs::remove_dir_all(&root);

    assert_eq!(package_result, ExitCode::from(1));
    assert_eq!(focused_result, ExitCode::SUCCESS);
}

/// Verifies explicit multi-project paths share one command-level coverage gate.
///
/// The application test covers one dependency declaration and the dependency's
/// own test covers the other. Neither project image reaches 100% alone, while
/// their combined command evidence does.
#[test]
fn explicit_multi_project_paths_aggregate_coverage_before_threshold() {
    let root = std::env::temp_dir().join(format!(
        "terlan_multi_project_coverage_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let app = root.join("app");
    let dependency = root.join("dependency");
    for directory in [
        app.join("src/app"),
        app.join("tests/app"),
        dependency.join("src/dependency"),
        dependency.join("tests/dependency"),
    ] {
        fs::create_dir_all(directory).expect("create coverage fixture directory");
    }
    fs::write(
        app.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\"src\"]\n\n[dependencies]\ndependency = { path = \"../dependency\" }\n",
    )
    .expect("write app manifest");
    fs::write(
        dependency.join("terlan.toml"),
        "[package]\nname = \"dependency\"\nversion = \"0.0.0\"\nnamespace = \"dependency\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .expect("write dependency manifest");
    let dependency_source = dependency.join("src/dependency/Values.terl");
    fs::write(
        &dependency_source,
        "module dependency.Values.\n\npub first(): Int -> 1.\n\npub second(): Int -> 2.\n",
    )
    .expect("write dependency source");
    let app_source = app.join("src/app/Main.terl");
    fs::write(
        &app_source,
        "module app.Main.\n\nimport dependency.Values.{first}.\n\npub use_first(): Int -> first().\n",
    )
    .expect("write app source");
    let app_test = app.join("tests/app/MainTest.terl");
    fs::write(
        &app_test,
        "module app.MainTest.\n\nimport app.Main.{use_first}.\n\n@test\npub first_is_covered(): Bool -> use_first() == 1.\n",
    )
    .expect("write app test");
    let dependency_test = dependency.join("tests/dependency/ValuesTest.terl");
    fs::write(
        &dependency_test,
        "module dependency.ValuesTest.\n\nimport dependency.Values.{second}.\n\n@test\npub second_is_covered(): Bool -> second() == 2.\n",
    )
    .expect("write dependency test");

    let result = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                app_test.to_string_lossy().into_owned(),
                dependency_test.to_string_lossy().into_owned(),
                "--coverage".to_string(),
                "--coverage-source".to_string(),
                app_source.to_string_lossy().into_owned(),
                "--coverage-source".to_string(),
                dependency_source.to_string_lossy().into_owned(),
                "--coverage-threshold".to_string(),
                "100".to_string(),
            ],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(result, ExitCode::SUCCESS);
}

/// Verifies standalone test inputs own their declaration coverage boundaries.
///
/// Inputs:
/// - One test module and its directory without an ancestor `terlan.toml`.
///
/// Output:
/// - File-focused and directory-focused runs satisfy 100% coverage gates.
///
/// Transformation:
/// - Uses the selected file or directory as its own coverage root without
///   admitting declarations from imported standard-library modules.
#[test]
fn standalone_test_inputs_own_their_declaration_coverage() {
    let root = std::env::temp_dir().join(format!(
        "terlan_standalone_coverage_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create standalone coverage directory");
    let test = root.join("StandaloneCoverageTest.terl");
    fs::write(
        &test,
        "module tests.StandaloneCoverageTest.\n\npub increment(value: Int): Int ->\n    value + 1.\n\n@test\npub standalone_declarations_are_covered(): Bool ->\n    increment(1) == 2.\n",
    )
    .expect("write standalone coverage test");

    let coverage_args = |path: &std::path::Path| CliCommand {
        verb: Some("test".to_string()),
        args: vec![
            path.to_string_lossy().into_owned(),
            "--coverage".to_string(),
            "--coverage-threshold".to_string(),
            "100".to_string(),
        ],
    };
    let file_result = run(coverage_args(&test), CliState::default());
    let directory_result = run(coverage_args(&root), CliState::default());
    let _ = fs::remove_dir_all(&root);

    assert_eq!(file_result, ExitCode::SUCCESS);
    assert_eq!(directory_result, ExitCode::SUCCESS);
}

/// Verifies unrelated standalone directories remain separate native suites.
#[test]
fn explicit_standalone_paths_group_by_parent_directory() {
    let root = std::env::temp_dir().join(format!(
        "terlan_standalone_group_test_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let first_dir = root.join("first");
    let second_dir = root.join("second");
    fs::create_dir_all(&first_dir).expect("create first suite");
    fs::create_dir_all(&second_dir).expect("create second suite");
    let first = first_dir.join("DuplicateTest.terl");
    let second = second_dir.join("DuplicateTest.terl");
    let source = "module tests.DuplicateTest.\n\n@test\npub succeeds(): Bool -> true.\n";
    fs::write(&first, source).expect("write first test");
    fs::write(&second, source).expect("write second test");

    let result = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(result, ExitCode::SUCCESS);
}

/// Verifies Terlan VM tests report boolean failures as command failures.
///
/// Inputs:
/// - A temporary `*Test.terl` module with one failing boolean `@test`.
///
/// Output:
/// - Nonzero command exit code.
///
/// Transformation:
/// - Executes the VM test lane and treats a returned `false` value as a stable
///   assertion failure rather than a backend crash.
#[test]
fn run_terlan_vm_tests_fails_false_bool_test() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_fail_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp vm failing test dir");
    let source_path = root.join("VmFailTest.terl");
    fs::write(
        &source_path,
        "module tests.vm.VmFailTest.\n\n@test\npub fails(): Bool ->\n    false.\n",
    )
    .expect("write vm failing test source");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::from(1));
}

/// Verifies source-level function-head pattern misses fail through native control.
///
/// Inputs:
/// - A temporary `*Test.terl` module with a tuple-pattern function head called
///   with a scalar argument.
/// - A VM test-result manifest output path.
///
/// Output:
/// - Nonzero command exit and a failed test-result manifest entry containing
///   the stable AOT no-clause diagnostic.
///
/// Transformation:
/// - Runs `terlc test` through the public VM test command so runtime clause
///   dispatch failures from source `.terl` tests are recorded through the
///   native control-flow boundary after interpreter removal.
#[test]
fn run_test_reports_aot_function_head_pattern_miss_diagnostic() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_function_head_miss_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp function-head miss test dir");
    let source_path = root.join("PatternHeadMissTest.terl");
    let result_path = root.join("test-results.json");
    fs::write(
        &source_path,
        "module tests.vm.PatternHeadMissTest.\n\npub require_pair({0, right}: {Int, Int}): Bool ->\n    true.\n\n@test\npub pattern_head_miss_has_metadata(): Bool ->\n    require_pair({1, 2}).\n",
    )
    .expect("write function-head miss test source");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--emit-test-result-manifest".to_string(),
                result_path.to_string_lossy().into_owned(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(exit_code, ExitCode::from(1));

    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result_path).expect("result text"))
            .expect("result json");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(results["failed"], 1);
    assert_eq!(
        results["tests"][0]["name"],
        "pattern_head_miss_has_metadata"
    );
    assert_eq!(results["tests"][0]["status"], "failed");
    let message = results["tests"][0]["message"]
        .as_str()
        .expect("failure message");
    assert_eq!(message, "error[if_clause]: no native if condition matched");
}

/// Verifies Terlan VM tests write source and result manifests.
///
/// Inputs:
/// - A temporary VM-compatible test module and manifest output flags.
///
/// Output:
/// - Assertions over command success and decoded JSON target metadata.
///
/// Transformation:
/// - Runs the VM test lane through the public command entry point, then checks
///   that manifests identify the target as `terlan-vm` with `vm`.
#[test]
fn run_terlan_vm_tests_writes_runtime_manifests() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_manifest_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp vm manifest test dir");
    let source_path = root.join("VmManifestTest.terl");
    let manifest_path = root.join("test-manifest.json");
    let result_path = root.join("test-results.json");
    fs::write(
        &source_path,
        "module tests.vm.VmManifestTest.\n\n@test\npub smoke(): Bool ->\n    true.\n",
    )
    .expect("write vm manifest test source");

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![
                source_path.to_string_lossy().into_owned(),
                "--target".to_string(),
                "terlan-vm".to_string(),
                "--emit-test-manifest".to_string(),
                manifest_path.to_string_lossy().into_owned(),
                "--emit-test-result-manifest".to_string(),
                result_path.to_string_lossy().into_owned(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(exit_code, ExitCode::SUCCESS);

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("manifest text"))
            .expect("manifest json");
    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result_path).expect("result text"))
            .expect("result json");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(manifest["module_name"], "tests.vm.VmManifestTest");
    assert_eq!(manifest["target"], "terlan-vm");
    assert_eq!(manifest["target_profile"], "vm");
    assert_eq!(manifest["tests"][0]["name"], "smoke");
    assert_eq!(results["target"], "terlan-vm");
    assert_eq!(results["target_profile"], "vm");
    assert_eq!(results["passed"], 1);
    assert_eq!(results["failed"], 0);
    assert_eq!(results["tests"][0]["status"], "passed");
}

/// Verifies bounded command execution preserves successful child output.
///
/// Inputs:
/// - A shell command that exits quickly after writing to stdout.
///
/// Output:
/// - Successful output object containing the child stdout.
///
/// Transformation:
/// - Runs the helper with a generous timeout and asserts it behaves like
///   `Command::output` for normal processes.
#[test]
fn run_command_with_timeout_collects_successful_output() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("printf ready");

    let output = run_command_with_timeout(&mut command, "test-shell", Duration::from_secs(2))
        .expect("command output");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ready");
}

/// Verifies bounded command execution kills long-running children.
///
/// Inputs:
/// - A shell command that sleeps longer than the supplied timeout.
///
/// Output:
/// - Timeout diagnostic naming the command label.
///
/// Transformation:
/// - Runs the helper with a short timeout and asserts the caller gets a stable
///   error instead of an unbounded wait.
#[test]
fn run_command_with_timeout_reports_timeout() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("sleep 2");

    let message = run_command_with_timeout(&mut command, "test-shell", Duration::from_millis(50))
        .expect_err("timeout diagnostic");

    assert!(
        message.contains("test-shell timed out after 50 milliseconds"),
        "{message}"
    );
}
