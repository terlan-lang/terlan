use super::*;
use crate::commands::process_runner::run_command_with_timeout;
use crate::commands::test::discovery::TestKind;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        passed: 1,
        failed: 1,
        results: vec![
            TestRunResult {
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
/// - A pass-only report with explicit validation messages and original spans.
///
/// Transformation:
/// - Converts discovered source tests into runner result entries without
///   executing target code.
#[test]
fn validation_pass_report_marks_all_tests_as_validated() {
    let report = validation_pass_report(&[DiscoveredTest {
        name: "smoke".to_string(),
        kind: TestKind::Test,
        span_start: 7,
        span_end: 19,
        literal_bool_result: Some(true),
    }]);

    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(report.results[0].name, "smoke");
    assert_eq!(report.results[0].status, TestRunStatus::Passed);
    assert_eq!(
        report.results[0].message.as_deref(),
        Some("validated without runtime execution")
    );
    assert_eq!(report.results[0].span_start, 7);
    assert_eq!(report.results[0].span_end, 19);
}

/// Verifies exact test selection keeps only the named test.
///
/// Inputs:
/// - Synthetic discovered tests and selector `second`.
///
/// Output:
/// - A one-element selected test list.
///
/// Transformation:
/// - Applies the same exact-name filter used by `terlc test --name`.
#[test]
fn select_tests_keeps_exact_selected_test() {
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
        Some("second"),
        "tests/SampleTest.terl",
        TestKind::Test,
    )
    .expect("selected tests");

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "second");
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
        Some("missing"),
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
/// - Assertions over command success and decoded manifest fields.
///
/// Transformation:
/// - Runs the public test command entry point with `--target js`, then checks
///   that validation-only metadata is serialized with the JS target identity.
#[test]
fn run_js_tests_writes_validation_manifests() {
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
        "module tests.js.ManifestTest.\n\n@test\npub smoke(): Bool ->\n    true.\n",
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
    assert_eq!(exit_code, ExitCode::SUCCESS);

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
    assert_eq!(results["passed"], 1);
    assert_eq!(results["failed"], 0);
    assert_eq!(results["tests"][0]["status"], "passed");
    assert_eq!(
        results["tests"][0]["message"],
        "validated without runtime execution"
    );
}

/// Verifies scalar Terlan VM tests execute from a native image.
///
/// Inputs:
/// - A temporary `*Test.terl` module with one passing boolean `@test`.
///
/// Output:
/// - Successful command exit code.
///
/// Transformation:
/// - Runs `terlc test --target terlan-vm` through the public command entry
///   point and verifies that it emits a `.tvm` image without transitional
///   JSON.
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

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![root.join("tests").to_string_lossy().into_owned()],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);

    assert_eq!(exit_code, ExitCode::SUCCESS);
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
