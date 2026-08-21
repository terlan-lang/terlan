use super::*;
use crate::{ColorChoice, DiagnosticFormat};
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds a command argument vector from string slices.
///
/// Inputs:
/// - `items`: borrowed argument strings.
///
/// Output:
/// - Owned `String` vector accepted by parser helpers.
///
/// Transformation:
/// - Clones each slice into owned CLI-like arguments.
fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn compiler_intrinsic_filter_removes_registered_placeholders_and_preserves_source_helpers() {
    let mut core = compile_native_filter_fixture(
        r#"
module std.core.Int.

@compiler.intrinsic
pub to_string(value: Int): String -> "".

@compiler.intrinsic
pub to_string_base(value: Int, base: Int): Dynamic -> Unit.

pub source_value(): Int -> 7.
"#,
    );

    remove_compiler_intrinsic_functions(&mut core);

    assert_eq!(
        core.functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        vec!["source_value"]
    );
}

#[test]
fn compiler_intrinsic_filter_keeps_unregistered_functions_with_matching_names() {
    let mut core = compile_native_filter_fixture(
        r#"
module std.fixture.IntLike.

pub to_string(value: Int): String -> "value".
pub to_string_base(value: Int, base: Int): Int -> value + base.
"#,
    );

    remove_compiler_intrinsic_functions(&mut core);

    assert_eq!(core.functions.len(), 2);
}

fn compile_native_filter_fixture(source: &str) -> CoreModule {
    crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "native_filter_fixture.terl",
        source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile native filter fixture")
    .core
}

#[test]
fn parse_test_args_accepts_default_terlan_vm_target() {
    let parsed = parse_test_args(&args(&["tests/sample.terl"])).expect("test args");
    assert_eq!(parsed.path, "tests/sample.terl");
    assert!(parsed.additional_paths.is_empty());
    assert_eq!(parsed.target, TestTarget::TerlanVm);
    assert!(parsed.test_names.is_empty());
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

/// Verifies no-argument `terlc test` targets the project test tree.
///
/// Inputs:
/// - Empty command-local argument vector.
///
/// Output:
/// - Parsed args with path `tests` and default Terlan VM target.
///
/// Transformation:
/// - Exercises the project-default CLI contract without touching the
///   filesystem.
#[test]
fn parse_test_args_defaults_to_tests_directory() {
    let parsed = parse_test_args(&[]).expect("test args");

    assert_eq!(parsed.path, "tests");
    assert!(parsed.additional_paths.is_empty());
    assert_eq!(parsed.target, TestTarget::TerlanVm);
    assert!(parsed.test_names.is_empty());
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

/// Verifies one test process can own several explicitly listed source roots.
///
/// Inputs:
/// - Three positional paths with a shared VM target.
///
/// Output:
/// - The first path remains the primary compatibility field and the remaining
///   paths retain their command-line order.
///
/// Transformation:
/// - Parses a batched test request without touching the filesystem.
#[test]
fn parse_test_args_accepts_multiple_source_paths() {
    let parsed = parse_test_args(&args(&[
        "std/system",
        "std/io/FileTest.terl",
        "std/data/JsonTest.terl",
    ]))
    .expect("batched test args");

    assert_eq!(parsed.path, "std/system");
    assert_eq!(
        parsed.additional_paths,
        ["std/io/FileTest.terl", "std/data/JsonTest.terl"]
    );
}

#[test]
fn parse_test_args_accepts_native_benchmark_controls() {
    let parsed = parse_test_args(&args(&[
        "tests/BenchmarkFrameworkTest.terl",
        "--bench",
        "--warmup",
        "2",
        "--samples",
        "17",
    ]))
    .expect("benchmark args");

    assert!(parsed.benchmark);
    assert_eq!(parsed.benchmark_warmup, 2);
    assert_eq!(parsed.benchmark_samples, 17);
}

#[test]
fn parse_test_args_rejects_benchmark_tuning_without_bench_selection() {
    let error = parse_test_args(&args(&[
        "tests/BenchmarkFrameworkTest.terl",
        "--samples",
        "5",
    ]))
    .expect_err("samples without --bench");

    assert_eq!(error, "--warmup and --samples require --bench");
}

#[test]
fn parse_test_args_rejects_benchmarks_on_validation_only_js_target() {
    let error = parse_test_args(&args(&[
        "tests/BenchmarkFrameworkTest.terl",
        "--bench",
        "--target",
        "js",
    ]))
    .expect_err("JS cannot produce runtime benchmark samples");

    assert_eq!(
        error,
        "@benchmark execution currently requires --target terlan-vm"
    );
}

#[test]
fn discovery_separates_test_and_benchmark_annotations() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module benchmark.discovery.\n\n@test\npub check(): Bool -> true.\n\n@benchmark\npub measure(): Bool -> true.\n",
    )
    .expect("syntax output");
    let discovered = discover_tests(&module).expect("discover executable cases");

    let tests = select_tests(
        discovered.clone(),
        &[],
        "BenchmarkTest.terl",
        crate::commands::test::discovery::TestKind::Test,
    )
    .expect("ordinary tests");
    let benchmarks = select_tests(
        discovered,
        &[],
        "BenchmarkTest.terl",
        crate::commands::test::discovery::TestKind::Benchmark,
    )
    .expect("benchmarks");

    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "check");
    assert_eq!(benchmarks.len(), 1);
    assert_eq!(benchmarks[0].name, "measure");
}

#[test]
fn parse_test_args_rejects_explicit_erlang_target() {
    let err = parse_test_args(&args(&["tests/sample.terl", "--target", "erlang"]))
        .expect_err("erlang target should be removed");

    assert_eq!(
        err,
        "test target `erlang` was removed from the public CLI; use `terlan-vm`"
    );
}

/// Verifies tests cannot select a removed evaluator or artifact runtime.
#[test]
fn parse_test_args_rejects_runtime_fallback_selection() {
    let error = parse_test_args(&args(&["tests/sample.terl", "--runtime", "interpreter"]))
        .expect_err("runtime fallback selector must be rejected");

    assert_eq!(error, "unsupported test option: --runtime");
}

/// Verifies parsing for the compiler-owned VM execution target.
///
/// Inputs:
/// - Synthetic CLI arguments with `--target terlan-vm`.
///
/// Output:
/// - Parsed args with the Terlan VM target selector.
///
/// Transformation:
/// - Parses command-local arguments without touching the filesystem.
#[test]
fn parse_test_args_accepts_explicit_terlan_vm_target() {
    let parsed = parse_test_args(&args(&["tests/SampleTest.terl", "--target", "terlan-vm"]))
        .expect("test args");

    assert_eq!(parsed.path, "tests/SampleTest.terl");
    assert_eq!(parsed.target, TestTarget::TerlanVm);
    assert!(parsed.test_names.is_empty());
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

/// Verifies parsing for the JavaScript validation target.
///
/// Inputs:
/// - Synthetic CLI arguments with `--target js`.
///
/// Output:
/// - Parsed args with the JS target selector.
///
/// Transformation:
/// - Parses command-local arguments without touching the filesystem.
#[test]
fn parse_test_args_accepts_explicit_js_target() {
    let parsed =
        parse_test_args(&args(&["std/js/StringTest.terl", "--target", "js"])).expect("test args");
    assert_eq!(parsed.path, "std/js/StringTest.terl");
    assert_eq!(parsed.target, TestTarget::Js);
    assert!(parsed.test_names.is_empty());
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

#[test]
fn parse_test_args_accepts_explicit_wasm_target() {
    let parsed =
        parse_test_args(&args(&["std/wasm/AbiTest.terl", "--target", "wasm"])).expect("test args");

    assert_eq!(parsed.path, "std/wasm/AbiTest.terl");
    assert_eq!(parsed.target, TestTarget::Wasm);
}

/// Verifies parsing for exact test-name selection.
///
/// Inputs:
/// - Synthetic CLI arguments with a source path and `--name`.
///
/// Output:
/// - Parsed args with the exact test function selector.
///
/// Transformation:
/// - Parses command-local arguments without touching the filesystem.
#[test]
fn parse_test_args_accepts_test_name_selector() {
    let parsed = parse_test_args(&args(&["tests/SampleTest.terl", "--name", "smoke_test"]))
        .expect("test args");

    assert_eq!(parsed.path, "tests/SampleTest.terl");
    assert_eq!(parsed.test_names, ["smoke_test"]);
}

/// Verifies distinct test-name selectors are accumulated.
///
/// Inputs:
/// - Synthetic CLI arguments with two `--name` flags.
///
/// Output:
/// - Assertions over the exact ordered selector set.
///
/// Transformation:
/// - Parses command-local arguments without recompiling the selected source.
#[test]
fn parse_test_args_accepts_repeated_test_name_selectors() {
    let parsed = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--name",
        "one",
        "--name",
        "two",
    ]))
    .expect("test args");

    assert_eq!(parsed.test_names, ["one", "two"]);
}

#[test]
fn parse_test_args_rejects_duplicate_test_name_selector() {
    let error = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--name",
        "same",
        "--name",
        "same",
    ]))
    .expect_err("duplicate selector");

    assert_eq!(error, "duplicate --name selector: same");
}

/// Verifies parsing for the opt-in test manifest flag.
///
/// Inputs:
/// - Synthetic CLI arguments with a source path and `--emit-test-manifest`.
///
/// Output:
/// - Assertions over parsed manifest path state.
///
/// Transformation:
/// - Parses command-local arguments without touching the filesystem.
#[test]
fn parse_test_args_accepts_test_manifest_path() {
    let parsed = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--emit-test-manifest",
        "target/sample.test-manifest.json",
    ]))
    .expect("test args");
    assert_eq!(parsed.path, "tests/SampleTest.terl");
    assert_eq!(
        parsed.emit_test_manifest,
        Some(PathBuf::from("target/sample.test-manifest.json"))
    );
}

/// Verifies one output manifest cannot ambiguously describe several roots.
///
/// Inputs:
/// - Two positional paths and one manifest destination.
///
/// Output:
/// - The stable single-source manifest diagnostic.
///
/// Transformation:
/// - Rejects the request before any source discovery or output creation.
#[test]
fn parse_test_args_rejects_manifest_output_for_multiple_paths() {
    let error = parse_test_args(&args(&[
        "tests/FirstTest.terl",
        "tests/SecondTest.terl",
        "--emit-test-manifest",
        "target/tests.json",
    ]))
    .expect_err("multiple manifest roots");

    assert_eq!(
        error,
        "test manifest output requires exactly one source path"
    );
}

/// Verifies duplicate manifest flags are rejected.
///
/// Inputs:
/// - Synthetic CLI arguments with two `--emit-test-manifest` flags.
///
/// Output:
/// - Assertion over the exact parser diagnostic.
///
/// Transformation:
/// - Parses command-local arguments and expects a duplicate-flag error.
#[test]
fn parse_test_args_rejects_duplicate_test_manifest_path() {
    let error = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--emit-test-manifest",
        "target/one.json",
        "--emit-test-manifest",
        "target/two.json",
    ]))
    .expect_err("error");
    assert_eq!(error, "duplicate --emit-test-manifest");
}

/// Verifies parsing for the opt-in test result manifest flag.
///
/// Inputs:
/// - Synthetic CLI arguments with a source path and
///   `--emit-test-result-manifest`.
///
/// Output:
/// - Assertions over parsed result-manifest path state.
///
/// Transformation:
/// - Parses command-local arguments without touching the filesystem.
#[test]
fn parse_test_args_accepts_test_result_manifest_path() {
    let parsed = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--emit-test-result-manifest",
        "target/sample.test-results.json",
    ]))
    .expect("test args");
    assert_eq!(parsed.path, "tests/SampleTest.terl");
    assert_eq!(
        parsed.emit_test_result_manifest,
        Some(PathBuf::from("target/sample.test-results.json"))
    );
}

/// Verifies duplicate result manifest flags are rejected.
///
/// Inputs:
/// - Synthetic CLI arguments with two `--emit-test-result-manifest` flags.
///
/// Output:
/// - Assertion over the exact parser diagnostic.
///
/// Transformation:
/// - Parses command-local arguments and expects a duplicate-flag error.
#[test]
fn parse_test_args_rejects_duplicate_test_result_manifest_path() {
    let error = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--emit-test-result-manifest",
        "target/one.json",
        "--emit-test-result-manifest",
        "target/two.json",
    ]))
    .expect_err("error");
    assert_eq!(error, "duplicate --emit-test-result-manifest");
}

#[test]
fn parse_test_args_rejects_unsupported_target() {
    let error =
        parse_test_args(&args(&["tests/sample.terl", "--target", "python"])).expect_err("error");
    assert_eq!(error, "unsupported test target: python");
}

/// Verifies the default JS validation profile.
///
/// Inputs:
/// - Default global CoreV0 target profile.
///
/// Output:
/// - Effective `js.shared` profile.
///
/// Transformation:
/// - Applies command-local JS target semantics without compiling source.
#[test]
fn effective_js_test_profile_defaults_to_shared_js_profile() {
    assert_eq!(
        effective_js_test_profile(TargetProfile::CoreV0).expect("profile"),
        TargetProfile::JsShared
    );
}

/// Verifies explicit JS validation profiles are preserved.
///
/// Inputs:
/// - Explicit browser JS target profile.
///
/// Output:
/// - The same browser JS target profile.
///
/// Transformation:
/// - Applies command-local JS target semantics without compiling source.
#[test]
fn effective_js_test_profile_preserves_explicit_js_profile() {
    assert_eq!(
        effective_js_test_profile(TargetProfile::JsBrowser).expect("profile"),
        TargetProfile::JsBrowser
    );
}

/// Verifies unrelated profiles are rejected for JS validation.
///
/// Inputs:
/// - Removed legacy Vm target profile.
///
/// Output:
/// - Stable JS profile-selection diagnostic.
///
/// Transformation:
/// - Applies command-local JS target semantics without compiling source.
#[test]
fn effective_js_test_profile_rejects_non_js_profile() {
    let error = effective_js_test_profile(TargetProfile::Vm).expect_err("error");
    assert_eq!(
        error,
        "terlc test --target js requires --target-profile js.shared, js.browser, or js.worker; got vm"
    );
}

/// Verifies VM tests reject legacy `beam-thin` project manifests.
///
/// Inputs:
/// - Temporary `terlan.toml` with the removed `beam-thin` artifact selector.
///
/// Output:
/// - Stable manifest parser rejection that mirrors build/deploy manifest semantics.
///
/// Transformation:
/// - Confirms there is no hidden fallback path in test discovery manifest parsing.
#[test]
fn vm_test_project_manifest_rejects_legacy_beam_thin_artifact() {
    let root = std::env::temp_dir().join(format!(
        "terlan_vm_test_legacy_manifest_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).expect("create source root");
    let manifest_path = root.join("terlan.toml");
    fs::write(
        &manifest_path,
        "[package]\nname = \"legacy\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("write manifest");

    let error = read_vm_test_project_manifest(&manifest_path).expect_err("legacy test manifest");
    let _ = fs::remove_dir_all(&root);

    assert!(error.contains("unsupported [build] artifact `beam-thin`"));
}

#[test]
fn supported_test_return_types_include_bool_and_assertions() {
    for text in ["Bool", "Assertion", "std.test.Test.Assertion"] {
        assert!(is_supported_test_return_type(&SyntaxTypeOutput {
            text: text.to_string(),
            span: Default::default(),
        }));
    }
}

#[test]
fn supported_test_return_types_reject_unit() {
    assert!(!is_supported_test_return_type(&SyntaxTypeOutput {
        text: "Unit".to_string(),
        span: Default::default(),
    }));
}

/// Verifies recursive directory discovery finds only test source files.
///
/// Inputs:
/// - A temporary directory containing nested test and non-test `.terl` files.
///
/// Output:
/// - Discovered path list containing only `*Test.terl` and legacy
///   `*_test.terl` files.
///
/// Transformation:
/// - Walks the directory through `collect_test_files`, then removes the
///   temporary fixture tree.
#[test]
fn collect_test_files_finds_only_test_sources() {
    let root = std::env::temp_dir().join(format!(
        "terlan_collect_test_files_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("std/core")).expect("create nested test dir");
    fs::create_dir_all(root.join("helpers")).expect("create helper dir");
    fs::write(
        root.join("std/core/BoolTest.terl"),
        "module std.core.BoolTest.\n",
    )
    .expect("write bool test");
    fs::write(
        root.join("std/core/bool_test.terl"),
        "module std.core.bool_test.\n",
    )
    .expect("write legacy bool test");
    fs::write(
        root.join("std/core/bool_tests.terl"),
        "module std.core.bool_tests.\n",
    )
    .expect("write non-test source");
    fs::write(root.join("helpers/helper.terl"), "module helpers.Helper.\n")
        .expect("write non-test source");
    fs::write(root.join("readme.md"), "# ignored\n").expect("write ignored markdown");

    let mut files = Vec::new();
    collect_test_files(&root, &mut files).expect("collect tests");
    files.sort();
    let paths = files
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(paths.len(), 2);
    assert!(paths
        .iter()
        .any(|path| path.ends_with("std/core/BoolTest.terl")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with("std/core/bool_test.terl")));
}

#[test]
fn test_source_path_requires_test_suffix() {
    assert!(is_test_source_path("std/core/BoolTest.terl"));
    assert!(is_test_source_path("std/core/bool_test.terl"));
    assert!(!is_test_source_path("std/core/bool_tests.terl"));
    assert!(!is_test_source_path("std/core/Bool.terl"));
    assert!(!is_test_source_path("std/core/BoolTest.md"));
}
