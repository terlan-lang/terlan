use super::beam_runner::{add_test_exports_to_erlang_source, render_eunit_wrapper_source};
use super::command_runner::quote_erlang_atom;
use super::command_runner::run_command_with_timeout;
use super::manifest::{TestRunReport, TestRunResult, TestRunStatus};
use super::release_support::{
    direct_release_support_module_names, release_support_modules, release_support_modules_by_name,
    source_release_support_module_names,
};
use super::*;
use crate::terlan_typeck::{CoreImport, CoreImportKind};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Parsed release-manifest module ownership used by support-module tests.
///
/// Inputs:
/// - One `module` row from `std/RELEASE_MANIFEST.tsv`.
///
/// Output:
/// - Module identifier and source path owned by that release row.
///
/// Transformation:
/// - Keeps only the fields needed to prove release API tests have embedded
///   source support when `terlc test` is installed outside a repository tree.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseModuleSource {
    module: String,
    source: String,
}

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
fn parse_test_args_accepts_default_erlang_target() {
    let parsed = parse_test_args(&args(&["tests/sample.terl"])).expect("test args");
    assert_eq!(parsed.path, "tests/sample.terl");
    assert_eq!(parsed.target, TestTarget::Erlang);
    assert_eq!(parsed.test_name, None);
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

/// Verifies no-argument `terlc test` targets the project test tree.
///
/// Inputs:
/// - Empty command-local argument vector.
///
/// Output:
/// - Parsed args with path `tests` and default Erlang target.
///
/// Transformation:
/// - Exercises the project-default CLI contract without touching the
///   filesystem.
#[test]
fn parse_test_args_defaults_to_tests_directory() {
    let parsed = parse_test_args(&[]).expect("test args");

    assert_eq!(parsed.path, "tests");
    assert_eq!(parsed.target, TestTarget::Erlang);
    assert_eq!(parsed.test_name, None);
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
}

#[test]
fn parse_test_args_accepts_explicit_erlang_target() {
    let parsed =
        parse_test_args(&args(&["tests/sample.terl", "--target", "erlang"])).expect("test args");
    assert_eq!(parsed.path, "tests/sample.terl");
    assert_eq!(parsed.target, TestTarget::Erlang);
    assert_eq!(parsed.test_name, None);
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
    assert_eq!(parsed.test_name, None);
    assert_eq!(parsed.emit_test_manifest, None);
    assert_eq!(parsed.emit_test_result_manifest, None);
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
    assert_eq!(parsed.test_name.as_deref(), Some("smoke_test"));
}

/// Verifies duplicate test-name selectors are rejected.
///
/// Inputs:
/// - Synthetic CLI arguments with two `--name` flags.
///
/// Output:
/// - Assertion over the exact parser diagnostic.
///
/// Transformation:
/// - Parses command-local arguments and expects a duplicate-flag error.
#[test]
fn parse_test_args_rejects_duplicate_test_name_selector() {
    let error = parse_test_args(&args(&[
        "tests/SampleTest.terl",
        "--name",
        "one",
        "--name",
        "two",
    ]))
    .expect_err("error");

    assert_eq!(error, "duplicate --name");
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
/// - Default global Erlang target profile.
///
/// Output:
/// - Effective `js.shared` profile.
///
/// Transformation:
/// - Applies command-local JS target semantics without compiling source.
#[test]
fn effective_js_test_profile_defaults_to_shared_js_profile() {
    assert_eq!(
        effective_js_test_profile(TargetProfile::Erlang).expect("profile"),
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
/// - Portable CoreIR target profile.
///
/// Output:
/// - Stable JS profile-selection diagnostic.
///
/// Transformation:
/// - Applies command-local JS target semantics without compiling source.
#[test]
fn effective_js_test_profile_rejects_non_js_profile() {
    let error = effective_js_test_profile(TargetProfile::CoreV0).expect_err("error");
    assert_eq!(
        error,
        "terlc test --target js requires --target-profile js.shared, js.browser, or js.worker; got core-v0"
    );
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
/// - Discovered path list containing only `*Test.terl` files.
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

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("std/core/BoolTest.terl"));
}

#[test]
fn test_source_path_requires_test_suffix() {
    assert!(is_test_source_path("std/core/BoolTest.terl"));
    assert!(!is_test_source_path("std/core/bool_test.terl"));
    assert!(!is_test_source_path("std/core/Bool.terl"));
    assert!(!is_test_source_path("std/core/BoolTest.md"));
}

/// Verifies release support modules are embedded for installed test runs.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Assertions over support module paths and embedded source text.
///
/// Transformation:
/// - Reads the static release support inventory without touching the
///   current working directory, proving `terlc test` does not depend on a
///   repo-relative `std/` folder at runtime.
#[test]
fn release_support_modules_are_embedded_for_installed_runner() {
    let modules = release_support_modules();

    assert!(modules.iter().any(|module| {
        module.path == "std/test/test.terl" && module.source.contains("module std.test.Test.")
    }));
    assert!(modules.iter().any(|module| {
        module.path == "std/core/string.terl" && module.source.contains("module std.core.String.")
    }));
    assert!(modules.iter().any(|module| {
        module.path == "std/http/error.terl" && module.source.contains("module std.http.Error.")
    }));
}

/// Verifies every exact release API test has embedded source support.
///
/// Inputs:
/// - `std/RELEASE_MANIFEST.tsv`, embedded at compile time.
/// - `tests/std/RELEASE_API_TESTS.tsv`, embedded at compile time.
/// - The static `release_support_modules` inventory.
///
/// Output:
/// - Test passes when each API id's release module source is present in the
///   installed test-runner support list.
///
/// Transformation:
/// - Maps every API id to the longest matching release module prefix, then
///   checks that module's source path is embedded for BEAM test execution.
#[test]
fn release_api_test_modules_have_embedded_runner_support() {
    let release_modules =
        parse_release_module_sources(include_str!("../../../../../std/RELEASE_MANIFEST.tsv"));
    let embedded_sources = release_support_modules()
        .iter()
        .map(|module| module.path)
        .collect::<std::collections::BTreeSet<_>>();

    for api_id in parse_release_api_ids(include_str!(
        "../../../../../tests/std/RELEASE_API_TESTS.tsv"
    )) {
        let owner = owning_release_module(&api_id, &release_modules)
            .unwrap_or_else(|| panic!("release API `{api_id}` has no release module owner"));
        if !release_module_requires_embedded_support(&owner.source) {
            continue;
        }
        assert!(
            embedded_sources.contains(owner.source.as_str()),
            "release API `{api_id}` is tested but owning source `{}` is not embedded for installed `terlc test`",
            owner.source
        );
    }
}

/// Verifies embedded support lookup is keyed by declared module name.
///
/// Inputs:
/// - Static embedded release support inventory.
///
/// Output:
/// - Test passes when representative std modules can be found by canonical
///   module name.
///
/// Transformation:
/// - Builds the dependency-planning index without compiling support modules.
#[test]
fn release_support_modules_by_name_indexes_declared_modules() {
    let support_by_name = release_support_modules_by_name();

    assert!(support_by_name.contains_key("std.test.Test"));
    assert!(support_by_name.contains_key("std.core.String"));
    assert!(support_by_name.contains_key("std.core.Object"));
    assert!(support_by_name.contains_key("std.collections.Map"));
}

/// Verifies runtime support planning does not compile unused std modules.
///
/// Inputs:
/// - Empty import list and representative CoreIR imports.
///
/// Output:
/// - Empty plan for no imports; exact std support module for a matching import.
///
/// Transformation:
/// - Exercises import filtering without invoking the formal compiler or Erlang.
#[test]
fn direct_release_support_module_names_follows_core_module_imports() {
    let support_by_name = release_support_modules_by_name();
    assert!(direct_release_support_module_names(&[], &support_by_name).is_empty());

    let imports = vec![
        CoreImport {
            module: "std.core.Bool".to_string(),
            kind: CoreImportKind::Module,
        },
        CoreImport {
            module: "std.core.Bool".to_string(),
            kind: CoreImportKind::Css,
        },
        CoreImport {
            module: "project.Local".to_string(),
            kind: CoreImportKind::Module,
        },
    ];

    let selected = direct_release_support_module_names(&imports, &support_by_name);

    assert_eq!(
        selected,
        ["std.core.Bool".to_string()].into_iter().collect()
    );
}

/// Verifies fully-qualified std references select embedded runtime support.
///
/// Inputs:
/// - Synthetic source containing fully-qualified calls without imports.
///
/// Output:
/// - Support module names for the referenced std modules.
///
/// Transformation:
/// - Exercises the source-reference fallback used by installed `terlc test`
///   for explicit calls such as `std.test.Test.assert(...)`.
#[test]
fn source_release_support_module_names_follows_qualified_references() {
    let support_by_name = release_support_modules_by_name();
    let source = "\
module sample.Test.

@test
pub passes(): Bool ->
    std.test.Test.assert(std.core.Bool.is_true(true)).
";

    let selected = source_release_support_module_names(source, &support_by_name);

    assert!(selected.contains("std.test.Test"));
    assert!(selected.contains("std.core.Bool"));
    assert!(!selected.contains("std.core.Int"));
}

/// Parses release module ownership rows from the std release manifest.
///
/// Inputs:
/// - `manifest`: text of `std/RELEASE_MANIFEST.tsv`.
///
/// Output:
/// - Release module/source pairs.
///
/// Transformation:
/// - Skips comments, blank lines, and the header row, then preserves only
///   module identifier and source-path cells from `module` rows.
fn parse_release_module_sources(manifest: &str) -> Vec<ReleaseModuleSource> {
    manifest
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() == 6 && fields[0] == "module" {
                Some(ReleaseModuleSource {
                    module: fields[1].to_string(),
                    source: fields[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Parses exact release API identifiers from the API test manifest.
///
/// Inputs:
/// - `manifest`: text of `tests/std/RELEASE_API_TESTS.tsv`.
///
/// Output:
/// - API identifiers from non-comment manifest rows.
///
/// Transformation:
/// - Skips comments and blank lines, then returns the first tab-separated cell
///   from each release API test row.
fn parse_release_api_ids(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split('\t').next())
        .map(str::to_string)
        .collect()
}

/// Finds the release module that owns one API identifier.
///
/// Inputs:
/// - `api_id`: exact release API identifier.
/// - `modules`: release modules parsed from the release manifest.
///
/// Output:
/// - Longest matching release module prefix, or `None` when no module owns the
///   API id.
///
/// Transformation:
/// - Mirrors the release-manifest checker's longest-prefix ownership rule so
///   nested modules such as `std.core.Ordering` win over broader packages.
fn owning_release_module<'a>(
    api_id: &str,
    modules: &'a [ReleaseModuleSource],
) -> Option<&'a ReleaseModuleSource> {
    modules
        .iter()
        .filter(|module| {
            api_id == module.module || api_id.starts_with(&format!("{}.", module.module))
        })
        .max_by_key(|module| module.module.len())
}

/// Returns whether a release module can be embedded into BEAM test workspaces.
///
/// Inputs:
/// - `source_path`: repository-relative std module source path from the release
///   manifest.
///
/// Output:
/// - `true` when the source has executable functions with no compiler-owned
///   body and should therefore be embedded for installed `terlc test` runs.
/// - `false` when native, intrinsic, or runtime lowering owns execution
///   coverage.
///
/// Transformation:
/// - Reads the source file from the repository checkout, ignores type-only
///   modules, and treats native or intrinsic annotations/bodies as markers
///   that the ordinary Erlang test workspace should not compile the full
///   support module.
fn release_module_requires_embedded_support(source_path: &str) -> bool {
    let source = std::fs::read_to_string(repo_root_for_test().join(source_path))
        .unwrap_or_else(|err| panic!("read release module source `{source_path}`: {err}"));
    has_executable_public_function(&source)
        && !source.contains("->\n    native.")
        && !source.contains("@compiler.intrinsic")
}

/// Returns whether a Terlan source contains public executable functions.
///
/// Inputs:
/// - `source`: Terlan module source text from the release manifest.
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
        trimmed.starts_with("pub (") || trimmed.starts_with("pub mut (") || {
            trimmed.starts_with("pub ")
                && !trimmed.starts_with("pub type ")
                && !trimmed.starts_with("pub opaque type ")
                && !trimmed.starts_with("pub struct ")
                && !trimmed.starts_with("pub trait ")
                && trimmed.contains('(')
        }
    })
}

/// Returns the repository root used by filesystem-backed unit tests.
///
/// Inputs:
/// - Cargo's compile-time `CARGO_MANIFEST_DIR` for `crates/terlan`.
///
/// Output:
/// - Path to the repository root.
///
/// Transformation:
/// - Walks from `crates/terlan` to `crates`, then to the repository root.
fn repo_root_for_test() -> std::path::PathBuf {
    crate::support::test_fs::repo_root()
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
                status: TestRunStatus::Passed,
                message: None,
                span_start: 10,
                span_end: 20,
            },
            TestRunResult {
                name: "fails".to_string(),
                status: TestRunStatus::Failed,
                message: Some("assertion returned false".to_string()),
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
    assert_eq!(json["tests"][0]["status"], "passed");
    assert!(json["tests"][0]["message"].is_null());
    assert_eq!(json["tests"][1]["name"], "fails");
    assert_eq!(json["tests"][1]["status"], "failed");
    assert_eq!(json["tests"][1]["message"], "assertion returned false");
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

/// Verifies literal bool tests do not require release support modules.
///
/// Inputs:
/// - Synthetic discovered test metadata for one surface marker and one runtime
///   test.
///
/// Output:
/// - Test passes when only the runtime test requires support modules.
///
/// Transformation:
/// - Exercises the runner decision without spawning Erlang or compiling std
///   support modules.
#[test]
fn tests_require_release_support_only_for_non_literal_tests() {
    let surface_test = DiscoveredTest {
        name: "surface".to_string(),
        span_start: 1,
        span_end: 2,
        literal_bool_result: Some(true),
    };
    let runtime_test = DiscoveredTest {
        name: "runtime".to_string(),
        span_start: 3,
        span_end: 4,
        literal_bool_result: None,
    };

    assert!(!tests_require_release_support(&[surface_test]));
    assert!(tests_require_release_support(&[runtime_test]));
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
                span_start: 1,
                span_end: 2,
                literal_bool_result: Some(true),
            },
            DiscoveredTest {
                name: "second".to_string(),
                span_start: 3,
                span_end: 4,
                literal_bool_result: Some(true),
            },
        ],
        Some("second"),
        "tests/SampleTest.terl",
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
            span_start: 1,
            span_end: 2,
            literal_bool_result: Some(true),
        }],
        Some("missing"),
        "tests/SampleTest.terl",
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

/// Verifies backend-owned EUnit wrapper rendering.
///
/// Inputs:
/// - Synthetic target module atom and discovered test metadata.
///
/// Output:
/// - Assertions over Erlang module, export, delegate call, and failure
///   mapping text.
///
/// Transformation:
/// - Renders wrapper source without compiling it.
#[test]
fn render_eunit_wrapper_source_delegates_to_target_tests() {
    let source = render_eunit_wrapper_source(
        "sample_eunit_tests",
        "sample",
        &[DiscoveredTest {
            name: "passes".to_string(),
            span_start: 1,
            span_end: 2,
            literal_bool_result: Some(true),
        }],
    );
    assert!(
        source.contains("-module('sample_eunit_tests')."),
        "{source}"
    );
    assert!(source.contains("-export(['passes_test'/0])."), "{source}");
    assert!(source.contains("'passes_test'() ->"), "{source}");
    assert!(source.contains("case 'sample':'passes'() of"), "{source}");
    assert!(
        source.contains("false -> erlang:error(assertion_returned_false);"),
        "{source}"
    );
    assert!(
        source.contains("Other -> erlang:error({unexpected_test_result, Other})"),
        "{source}"
    );
}

/// Verifies test-only Erlang export injection.
///
/// Inputs:
/// - Minimal generated Erlang source and synthetic discovered tests.
///
/// Output:
/// - Assertions over the inserted export attribute and original source.
///
/// Transformation:
/// - Inserts exports after the module line without altering production
///   emitter behavior.
#[test]
fn add_test_exports_to_erlang_source_inserts_test_only_export() {
    let source = add_test_exports_to_erlang_source(
        "-module(sample).\n\nhidden() -> true.\n".to_string(),
        &[DiscoveredTest {
            name: "hidden".to_string(),
            span_start: 1,
            span_end: 2,
            literal_bool_result: Some(true),
        }],
    );
    assert!(
        source.starts_with("-module(sample).\n-export(['hidden'/0]).\n\n"),
        "{source}"
    );
    assert!(source.contains("hidden() -> true."), "{source}");
}

#[test]
fn quote_erlang_atom_escapes_quotes_and_backslashes() {
    assert_eq!(quote_erlang_atom("std_test"), "'std_test'");
    assert_eq!(quote_erlang_atom("a'b\\c"), "'a\\'b\\\\c'");
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
        message.contains("test-shell timed out after 0 seconds"),
        "{message}"
    );
}
