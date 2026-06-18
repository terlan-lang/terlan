use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge;
use terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxTypeOutput,
};

use crate::commands::artifacts::{
    collect_syntax_file_import_bytes, collect_syntax_markdown_inputs,
    collect_syntax_template_inputs,
};
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::{CliCommand, CliState};

/// Supported backend runner for `terlc test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestTarget {
    Erlang,
    Js,
}

/// Parsed command-local arguments for `terlc test`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestArgs {
    path: String,
    target: TestTarget,
    emit_test_manifest: Option<PathBuf>,
    emit_test_result_manifest: Option<PathBuf>,
}

/// Embedded source for one release support module used by `terlc test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReleaseSupportModule {
    path: &'static str,
    source: &'static str,
}

/// Validated test function metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredTest {
    name: String,
    span_start: usize,
    span_end: usize,
}

/// Serializable source-level test manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestManifest {
    source_path: String,
    module_name: String,
    target: String,
    target_profile: String,
    tests: Vec<TestManifestEntry>,
}

/// Serializable metadata for one discovered test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestManifestEntry {
    name: String,
    span_start: usize,
    span_end: usize,
}

/// Serializable test execution result manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestResultManifest {
    source_path: String,
    module_name: String,
    target: String,
    target_profile: String,
    passed: usize,
    failed: usize,
    tests: Vec<TestResultManifestEntry>,
}

/// Serializable execution result for one discovered test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestResultManifestEntry {
    name: String,
    status: String,
    message: Option<String>,
    span_start: usize,
    span_end: usize,
}

/// In-memory execution report for a test run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestRunReport {
    passed: usize,
    failed: usize,
    results: Vec<TestRunResult>,
}

/// In-memory execution result for one test.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestRunResult {
    name: String,
    status: TestRunStatus,
    message: Option<String>,
    span_start: usize,
    span_end: usize,
}

/// Stable status labels for test result artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestRunStatus {
    Passed,
    Failed,
}

impl TestRunStatus {
    /// Returns the manifest label for this status.
    ///
    /// Inputs:
    /// - `self`: status value to serialize.
    ///
    /// Output:
    /// - Stable lowercase status label.
    ///
    /// Transformation:
    /// - Maps enum variants to the public result-manifest vocabulary.
    fn as_str(self) -> &'static str {
        match self {
            TestRunStatus::Passed => "passed",
            TestRunStatus::Failed => "failed",
        }
    }
}

impl TestRunReport {
    /// Returns whether all tests passed.
    ///
    /// Inputs:
    /// - `self`: completed execution report.
    ///
    /// Output:
    /// - `true` when no test failed.
    ///
    /// Transformation:
    /// - Checks the aggregate failed count without inspecting stdout.
    fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Temporary BEAM workspace owner.
struct TempBeamWorkspace {
    path: PathBuf,
}

impl TempBeamWorkspace {
    /// Creates a temporary workspace for emitted Erlang and BEAM artifacts.
    ///
    /// Inputs:
    /// - `module_name`: Terlan module name used only for a readable path stem.
    ///
    /// Output:
    /// - `Ok(TempBeamWorkspace)` when the directory exists.
    /// - `Err(message)` when the directory cannot be created.
    ///
    /// Transformation:
    /// - Builds a unique path under the host temp directory using process id
    ///   and current nanoseconds, then creates it.
    fn create(module_name: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("cannot create test workspace timestamp: {err}"))?
            .as_nanos();
        let stem = crate::support::erlang_output_stem(module_name);
        let path = std::env::temp_dir().join(format!(
            "terlan_test_{stem}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&path)
            .map_err(|err| format!("cannot create test workspace {}: {err}", path.display()))?;
        Ok(Self { path })
    }

    /// Returns the temporary workspace path.
    ///
    /// Inputs:
    /// - `self`: live workspace owner.
    ///
    /// Output:
    /// - Borrowed path to the workspace directory.
    ///
    /// Transformation:
    /// - Exposes the path without transferring cleanup ownership.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempBeamWorkspace {
    /// Removes the temporary workspace when the owner goes out of scope.
    ///
    /// Inputs:
    /// - `self`: workspace owner being dropped.
    ///
    /// Output:
    /// - No reported output; cleanup failures are ignored.
    ///
    /// Transformation:
    /// - Attempts recursive removal of the temporary directory.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Executes the `test` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing a source path plus command-local
///   target flags.
/// - `state`: parsed global CLI state including diagnostic format, cache
///   directory, native policy, and target profile.
///
/// Output:
/// - `ExitCode::SUCCESS` when every discovered test passes on the target.
/// - `ExitCode::from(2)` for malformed command arguments.
/// - `ExitCode::from(1)` for compile, discovery, emit, BEAM compile, or test
///   execution failures.
///
/// Transformation:
/// - Routes one source module through the formal compiler path, discovers
///   `@test` declarations, emits Erlang artifacts, and executes the tests
///   against BEAM.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_test_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    match args.target {
        TestTarget::Erlang => run_erlang_tests(&args, state),
        TestTarget::Js => run_js_tests(&args, state),
    }
}

/// Parses command-local arguments for `terlc test`.
///
/// Inputs:
/// - `args`: command arguments after `main.rs` has removed global options and
///   command verb.
///
/// Output:
/// - `Ok(TestArgs)` for zero or one source path, optional
///   `--target erlang|js`, optional `--emit-test-manifest <path>`, and optional
///   `--emit-test-result-manifest <path>`.
/// - `Err(message)` for malformed flags, unsupported targets, or wrong arity.
///
/// Transformation:
/// - Walks raw strings, extracts command-local target selection, and rejects
///   unexpected arguments.
fn parse_test_args(args: &[String]) -> Result<TestArgs, String> {
    let mut path = None;
    let mut target = TestTarget::Erlang;
    let mut emit_test_manifest = None;
    let mut emit_test_result_manifest = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--target requires a value".to_string());
                };
                target = match value.as_str() {
                    "erlang" => TestTarget::Erlang,
                    "js" => TestTarget::Js,
                    other => return Err(format!("unsupported test target: {other}")),
                };
                i += 2;
            }
            "--emit-test-manifest" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--emit-test-manifest requires a path".to_string());
                };
                if emit_test_manifest.replace(PathBuf::from(value)).is_some() {
                    return Err("duplicate --emit-test-manifest".to_string());
                }
                i += 2;
            }
            "--emit-test-result-manifest" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--emit-test-result-manifest requires a path".to_string());
                };
                if emit_test_result_manifest
                    .replace(PathBuf::from(value))
                    .is_some()
                {
                    return Err("duplicate --emit-test-result-manifest".to_string());
                }
                i += 2;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unsupported test option: {arg}"));
            }
            arg => {
                if path.replace(arg.to_string()).is_some() {
                    return Err("missing or extra path argument".to_string());
                }
                i += 1;
            }
        }
    }

    Ok(TestArgs {
        path: path.unwrap_or_else(|| "tests".to_string()),
        target,
        emit_test_manifest,
        emit_test_result_manifest,
    })
}

/// Executes discovered tests through the Erlang target runner.
///
/// Inputs:
/// - `args`: parsed test arguments, including source path and optional
///   manifest output path.
/// - `state`: global CLI state used for formal compilation.
///
/// Output:
/// - `ExitCode::SUCCESS` when all tests pass.
/// - `ExitCode::from(1)` for compile, emit, toolchain, or execution failure.
///
/// Transformation:
/// - Compiles the module through the formal pipeline, emits BEAM-ready Erlang,
///   compiles support modules, and invokes each exported test function.
fn run_erlang_tests(args: &TestArgs, state: CliState) -> ExitCode {
    if state.target_profile != TargetProfile::Erlang {
        eprintln!(
            "terlc test --target erlang requires --target-profile erlang, got {}",
            state.target_profile.as_str()
        );
        return ExitCode::from(1);
    }

    let path = args.path.as_str();
    if Path::new(path).is_dir() {
        return run_erlang_test_directory(args, state);
    }
    if !is_test_source_path(path) {
        eprintln!("terlc test requires a *_test.terl source file for 0.0.1: {path}");
        return ExitCode::from(1);
    }

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::Erlang,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };

    let tests = match discover_tests(&compiled.syntax_output) {
        Ok(tests) => tests,
        Err(messages) => {
            for message in messages {
                eprintln!("{message}");
            }
            return ExitCode::from(1);
        }
    };
    if let Some(manifest_path) = args.emit_test_manifest.as_deref() {
        if let Err(message) = write_test_manifest(
            manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "erlang",
            state.target_profile.as_str(),
            &tests,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    if tests.is_empty() {
        eprintln!("no @test declarations found in {path}");
        return ExitCode::from(1);
    }

    let workspace = match TempBeamWorkspace::create(&compiled.syntax_output.module_name) {
        Ok(workspace) => workspace,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    if let Err(message) =
        emit_and_compile_erlang_module(path, &source, workspace.path(), &state, &tests)
    {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if let Err(message) = emit_and_compile_release_support_modules(
        workspace.path(),
        &state,
        &compiled.syntax_output.module_name,
    ) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }

    let module_atom = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let eunit_module_atom =
        match emit_and_compile_eunit_wrapper(workspace.path(), &module_atom, &tests) {
            Ok(module_atom) => module_atom,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };

    let report = run_discovered_erlang_tests(workspace.path(), &module_atom, &tests);
    if let Some(result_manifest_path) = args.emit_test_result_manifest.as_deref() {
        if let Err(message) = write_test_result_manifest(
            result_manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "erlang",
            state.target_profile.as_str(),
            &report,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    if !report.is_success() {
        return ExitCode::from(1);
    }
    if let Err(message) = run_eunit_wrapper(workspace.path(), &eunit_module_atom) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Validates discovered tests through the JavaScript target compile path.
///
/// Inputs:
/// - `args`: parsed test arguments, including one source file or directory and
///   optional manifest output paths.
/// - `state`: global CLI state used for diagnostics, cache, native policy, and
///   target-profile selection.
///
/// Output:
/// - `ExitCode::SUCCESS` when every selected test module compiles for a JS
///   profile and contains valid `@test` functions.
/// - `ExitCode::from(1)` when profile selection, file discovery, formal
///   compilation, test discovery, or manifest writing fails.
///
/// Transformation:
/// - Compiles each test module through the formal pipeline with a JavaScript
///   target profile, validates source-level test declarations, and records a
///   validation-only pass report without executing JavaScript runtime code.
fn run_js_tests(args: &TestArgs, state: CliState) -> ExitCode {
    let profile = match effective_js_test_profile(state.target_profile) {
        Ok(profile) => profile,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    let path = args.path.as_str();
    if Path::new(path).is_dir() {
        return run_js_test_directory(args, state, profile);
    }
    if !is_test_source_path(path) {
        eprintln!("terlc test requires a *_test.terl source file for 0.0.4 JS validation: {path}");
        return ExitCode::from(1);
    }

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            profile,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };

    let tests = match discover_tests(&compiled.syntax_output) {
        Ok(tests) => tests,
        Err(messages) => {
            for message in messages {
                eprintln!("{message}");
            }
            return ExitCode::from(1);
        }
    };
    if let Some(manifest_path) = args.emit_test_manifest.as_deref() {
        if let Err(message) = write_test_manifest(
            manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "js",
            profile.as_str(),
            &tests,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    if tests.is_empty() {
        eprintln!("no @test declarations found in {path}");
        return ExitCode::from(1);
    }

    let report = validation_pass_report(&tests);
    if let Some(result_manifest_path) = args.emit_test_result_manifest.as_deref() {
        if let Err(message) = write_test_result_manifest(
            result_manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "js",
            profile.as_str(),
            &report,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    print_validation_pass_report(&report);
    ExitCode::SUCCESS
}

/// Returns the JavaScript profile used by `terlc test --target js`.
///
/// Inputs:
/// - `profile`: global target profile selected before command dispatch.
///
/// Output:
/// - `Ok(TargetProfile)` for accepted JS profiles.
/// - `Err(message)` when the selected profile is not compatible with JS tests.
///
/// Transformation:
/// - Treats the default global Erlang profile as an ergonomic request for
///   `js.shared`, while preserving explicit JS profile choices and rejecting
///   unrelated backend profiles.
fn effective_js_test_profile(profile: TargetProfile) -> Result<TargetProfile, String> {
    if profile == TargetProfile::Erlang {
        return Ok(TargetProfile::JsShared);
    }
    if profile.is_js() {
        return Ok(profile);
    }
    Err(format!(
        "terlc test --target js requires --target-profile js.shared, js.browser, or js.worker; got {}",
        profile.as_str()
    ))
}

/// Validates all JavaScript test modules below one directory.
///
/// Inputs:
/// - `args`: parsed test arguments whose path is a directory.
/// - `state`: global CLI state used for formal compilation.
/// - `profile`: effective JavaScript target profile for every file.
///
/// Output:
/// - `ExitCode::SUCCESS` when every discovered JS test file validates.
/// - `ExitCode::from(1)` when discovery fails, no test files exist, manifest
///   flags are used with a directory, or any test file fails.
///
/// Transformation:
/// - Recursively discovers `*_test.terl` files in deterministic order, then
///   delegates each file to the JS validation runner and aggregates status
///   without inventing a directory-level manifest format.
fn run_js_test_directory(args: &TestArgs, state: CliState, profile: TargetProfile) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!("test manifest output is only supported for a single *_test.terl file");
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!("no *_test.terl files found in {}", args.path);
        return ExitCode::from(1);
    }

    let mut failed = false;
    for file in files {
        let file_args = TestArgs {
            path: file.to_string_lossy().into_owned(),
            target: args.target,
            emit_test_manifest: None,
            emit_test_result_manifest: None,
        };
        let mut file_state = state.clone();
        file_state.target_profile = profile;
        if run_js_tests(&file_args, file_state) != ExitCode::SUCCESS {
            failed = true;
        }
    }

    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Builds a validation-only pass report for JS tests.
///
/// Inputs:
/// - `tests`: discovered test metadata already accepted by test discovery.
///
/// Output:
/// - A `TestRunReport` with every test marked passed.
///
/// Transformation:
/// - Converts source-level test metadata into result entries with a stable
///   message that distinguishes compile/discovery validation from runtime
///   JavaScript execution.
fn validation_pass_report(tests: &[DiscoveredTest]) -> TestRunReport {
    TestRunReport {
        passed: tests.len(),
        failed: 0,
        results: tests
            .iter()
            .map(|test| TestRunResult {
                name: test.name.clone(),
                status: TestRunStatus::Passed,
                message: Some("validated without runtime execution".to_string()),
                span_start: test.span_start,
                span_end: test.span_end,
            })
            .collect(),
    }
}

/// Prints a validation-only test report.
///
/// Inputs:
/// - `report`: validation-only result report for one JS test module.
///
/// Output:
/// - Human-readable test status lines written to stdout.
///
/// Transformation:
/// - Renders the same compact shape as the Erlang runner while adding
///   `(validated)` to make the non-runtime status explicit.
fn print_validation_pass_report(report: &TestRunReport) {
    println!("running {} tests", report.results.len());
    for result in &report.results {
        println!("test {} ... ok (validated)", result.name);
    }
    println!("test result: ok. {} passed; 0 failed", report.passed);
}

/// Executes all test modules below one directory through the Erlang runner.
///
/// Inputs:
/// - `args`: parsed test arguments whose path is a directory.
/// - `state`: global CLI state used for formal compilation and execution.
///
/// Output:
/// - `ExitCode::SUCCESS` when every discovered test file passes.
/// - `ExitCode::from(1)` when discovery fails, no test files exist, manifest
///   flags are used with a directory, or any test file fails.
///
/// Transformation:
/// - Recursively discovers `*_test.terl` files in deterministic order, then
///   delegates each file to the existing single-file runner and aggregates the
///   command exit status without inventing a directory-level manifest format.
fn run_erlang_test_directory(args: &TestArgs, state: CliState) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!("test manifest output is only supported for a single *_test.terl file");
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!("no *_test.terl files found in {}", args.path);
        return ExitCode::from(1);
    }

    let mut failed = false;
    for file in files {
        let file_args = TestArgs {
            path: file.to_string_lossy().into_owned(),
            target: args.target,
            emit_test_manifest: None,
            emit_test_result_manifest: None,
        };
        if run_erlang_tests(&file_args, state.clone()) != ExitCode::SUCCESS {
            failed = true;
        }
    }

    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Collects test source files below a directory.
///
/// Inputs:
/// - `dir`: directory to traverse.
/// - `files`: accumulator for discovered test files.
///
/// Output:
/// - `Ok(())` when traversal succeeds.
/// - `Err(message)` when the directory cannot be read.
///
/// Transformation:
/// - Recursively walks the directory tree and records only files accepted by
///   the `*_test.terl` source layout predicate.
fn collect_test_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read test directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read test directory entry in {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, files)?;
        } else if path
            .to_str()
            .is_some_and(|path_text| is_test_source_path(path_text))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// Writes a source-level test manifest.
///
/// Inputs:
/// - `manifest_path`: filesystem path for the JSON manifest.
/// - `source_path`: user-provided source file path.
/// - `module_name`: parsed Terlan module name.
/// - `target`: selected test runner target.
/// - `target_profile`: selected compiler target profile.
/// - `tests`: discovered source-level tests with spans.
///
/// Output:
/// - `Ok(())` when deterministic JSON is written.
/// - `Err(message)` when serialization, parent-directory creation, or file
///   writing fails.
///
/// Transformation:
/// - Converts discovered test metadata into a stable JSON artifact for release
///   gates and downstream runner integrations.
fn write_test_manifest(
    manifest_path: &Path,
    source_path: &str,
    module_name: &str,
    target: &str,
    target_profile: &str,
    tests: &[DiscoveredTest],
) -> Result<(), String> {
    if let Some(parent) = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create test manifest directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let manifest = TestManifest {
        source_path: source_path.to_string(),
        module_name: module_name.to_string(),
        target: target.to_string(),
        target_profile: target_profile.to_string(),
        tests: tests
            .iter()
            .map(|test| TestManifestEntry {
                name: test.name.clone(),
                span_start: test.span_start,
                span_end: test.span_end,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to serialize test manifest: {err}"))?;
    fs::write(manifest_path, format!("{json}\n")).map_err(|err| {
        format!(
            "failed to write test manifest {}: {err}",
            manifest_path.display()
        )
    })
}

/// Writes a source-level test execution result manifest.
///
/// Inputs:
/// - `manifest_path`: filesystem path for the JSON manifest.
/// - `source_path`: user-provided source file path.
/// - `module_name`: parsed Terlan module name.
/// - `target`: selected test runner target.
/// - `target_profile`: selected compiler target profile.
/// - `report`: direct BEAM execution report.
///
/// Output:
/// - `Ok(())` when deterministic JSON is written.
/// - `Err(message)` when serialization, parent-directory creation, or file
///   writing fails.
///
/// Transformation:
/// - Converts runtime pass/fail data into a stable JSON artifact for release
///   tooling and runner integrations.
fn write_test_result_manifest(
    manifest_path: &Path,
    source_path: &str,
    module_name: &str,
    target: &str,
    target_profile: &str,
    report: &TestRunReport,
) -> Result<(), String> {
    if let Some(parent) = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create test result manifest directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let manifest = TestResultManifest {
        source_path: source_path.to_string(),
        module_name: module_name.to_string(),
        target: target.to_string(),
        target_profile: target_profile.to_string(),
        passed: report.passed,
        failed: report.failed,
        tests: report
            .results
            .iter()
            .map(|result| TestResultManifestEntry {
                name: result.name.clone(),
                status: result.status.as_str().to_string(),
                message: result.message.clone(),
                span_start: result.span_start,
                span_end: result.span_end,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to serialize test result manifest: {err}"))?;
    fs::write(manifest_path, format!("{json}\n")).map_err(|err| {
        format!(
            "failed to write test result manifest {}: {err}",
            manifest_path.display()
        )
    })
}

/// Returns whether a source path is accepted by the 0.0.1 test-file layout.
///
/// Inputs:
/// - `path`: user-provided source path passed to `terlc test`.
///
/// Output:
/// - `true` when the file name ends in `_test.terl`.
///
/// Transformation:
/// - Reads only the final path component and compares its suffix, preserving
///   the documented 0.0.1 rule that tests live in separate `*_test.terl` files.
fn is_test_source_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.terl"))
}

/// Discovers valid `@test` function declarations.
///
/// Inputs:
/// - `module`: syntax output produced by formal parsing.
///
/// Output:
/// - `Ok(Vec<DiscoveredTest>)` when all annotated declarations are valid.
/// - `Err(Vec<String>)` when any annotated declaration violates the test
///   contract.
///
/// Transformation:
/// - Filters declarations with `@test` annotations and validates that they are
///   zero-argument functions returning `Bool` or assertion-compatible types.
fn discover_tests(module: &SyntaxModuleOutput) -> Result<Vec<DiscoveredTest>, Vec<String>> {
    let mut tests = Vec::new();
    let mut errors = Vec::new();

    for declaration in &module.declarations {
        if !has_test_annotation(declaration) {
            continue;
        }
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                ..
            } => {
                if !params.is_empty() {
                    errors.push(format!("@test function {name} must have zero parameters"));
                }
                if !is_supported_test_return_type(return_type) {
                    errors.push(format!(
                        "@test function {name} must return Bool or std.test.Test.Assertion, got {}",
                        return_type.text
                    ));
                }
                if params.is_empty() && is_supported_test_return_type(return_type) {
                    tests.push(DiscoveredTest {
                        name: name.clone(),
                        span_start: declaration.span.start,
                        span_end: declaration.span.end,
                    });
                }
            }
            _ => errors.push("@test can only annotate function declarations".to_string()),
        }
    }

    if errors.is_empty() {
        Ok(tests)
    } else {
        Err(errors)
    }
}

/// Returns whether a declaration carries the source-level `@test` annotation.
///
/// Inputs:
/// - `declaration`: syntax declaration to inspect.
///
/// Output:
/// - `true` when any annotation path is exactly `test`.
///
/// Transformation:
/// - Compares serialized annotation path segments without reading source text.
fn has_test_annotation(declaration: &SyntaxDeclarationOutput) -> bool {
    declaration
        .annotations
        .iter()
        .any(|annotation| annotation.path.as_slice() == ["test"])
}

/// Returns whether a test return type is supported by the first runner.
///
/// Inputs:
/// - `return_type`: syntax-level return type text and span.
///
/// Output:
/// - `true` for `Bool`, imported `Assertion`, and canonical
///   `std.test.Test.Assertion`.
///
/// Transformation:
/// - Trims syntax-output type text and checks the stable 0.0.1 assertion
///   spellings accepted by test discovery, without accepting backend-shaped or
///   AST module spellings.
fn is_supported_test_return_type(return_type: &SyntaxTypeOutput) -> bool {
    matches!(
        return_type.text.trim(),
        "Bool" | "Assertion" | "std.test.Test.Assertion"
    )
}

/// Emits and compiles the primary test module.
///
/// Inputs:
/// - `path`: source path used for compiler diagnostics and dependency roots.
/// - `source`: source text for the module.
/// - `workspace`: temporary directory for emitted and compiled artifacts.
/// - `state`: borrowed global CLI state used by formal compilation.
/// - `tests`: discovered test functions that should be exported only in the
///   temporary test artifact.
///
/// Output:
/// - `Ok(())` when `.erl` and `.beam` artifacts are ready.
/// - `Err(message)` when compilation, emission, write, or `erlc` fails.
///
/// Transformation:
/// - Recompiles the module through the formal pipeline, emits Erlang into the
///   workspace with test-only exports, and invokes `erlc`.
fn emit_and_compile_erlang_module(
    path: &str,
    source: &str,
    workspace: &Path,
    state: &CliState,
    tests: &[DiscoveredTest],
) -> Result<(), String> {
    let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        path,
        source,
        state.diagnostic_format,
        state.cache_dir.as_deref(),
        state.native_policy,
        TargetProfile::Erlang,
    )
    .map_err(|exit_code| format!("formal pipeline failed with exit code {exit_code:?}"))?;
    emit_compiled_module_to_workspace(path, workspace, &compiled, tests)
}

/// Emits and compiles release support modules for runtime tests.
///
/// Inputs:
/// - `workspace`: temporary directory for emitted and compiled artifacts.
/// - `state`: borrowed global CLI state used by formal compilation.
/// - `primary_module_name`: module under test, used to avoid recompiling it as
///   support.
///
/// Output:
/// - `Ok(())` when all selected support `.beam` files are available.
/// - `Err(message)` when a support source is missing or compilation fails.
///
/// Transformation:
/// - Compiles the minimal 0.0.1 release support module list through the formal
///   pipeline, emits Erlang, and invokes `erlc`.
fn emit_and_compile_release_support_modules(
    workspace: &Path,
    state: &CliState,
    primary_module_name: &str,
) -> Result<(), String> {
    for module in release_support_modules() {
        let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            module.path,
            module.source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::Erlang,
        )
        .map_err(|exit_code| {
            format!(
                "release support module {} failed with exit code {exit_code:?}",
                module.path
            )
        })?;
        if compiled.syntax_output.module_name == primary_module_name {
            continue;
        }
        emit_compiled_module_to_workspace(module.path, workspace, &compiled, &[])?;
    }
    Ok(())
}

/// Returns the embedded release support module list for `terlc test`.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static source path/text pairs compiled into the BEAM test workspace.
///
/// Transformation:
/// - Centralizes the current release-matrix support set so future additions are
///   explicit and reviewable, while keeping installed `terlc test` independent
///   of the caller's current working directory.
fn release_support_modules() -> &'static [ReleaseSupportModule] {
    &[
        ReleaseSupportModule {
            path: "std/test/test.terl",
            source: include_str!("../../../../../std/test/test.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/atom.terl",
            source: include_str!("../../../../../std/core/atom.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/bool.terl",
            source: include_str!("../../../../../std/core/bool.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/unit.terl",
            source: include_str!("../../../../../std/core/unit.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/ordering.terl",
            source: include_str!("../../../../../std/core/ordering.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/int.terl",
            source: include_str!("../../../../../std/core/int.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/float.terl",
            source: include_str!("../../../../../std/core/float.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/option.terl",
            source: include_str!("../../../../../std/core/option.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/result.terl",
            source: include_str!("../../../../../std/core/result.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/error.terl",
            source: include_str!("../../../../../std/core/error.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/equal.terl",
            source: include_str!("../../../../../std/core/equal.terl"),
        },
        ReleaseSupportModule {
            path: "std/http/error.terl",
            source: include_str!("../../../../../std/http/error.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/string.terl",
            source: include_str!("../../../../../std/core/string.terl"),
        },
        ReleaseSupportModule {
            path: "std/io/console.terl",
            source: include_str!("../../../../../std/io/console.terl"),
        },
        ReleaseSupportModule {
            path: "std/io/file.terl",
            source: include_str!("../../../../../std/io/file.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/iterator.terl",
            source: include_str!("../../../../../std/collections/iterator.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/index.terl",
            source: include_str!("../../../../../std/collections/index.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/list.terl",
            source: include_str!("../../../../../std/collections/list.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/map.terl",
            source: include_str!("../../../../../std/collections/map.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/set.terl",
            source: include_str!("../../../../../std/collections/set.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/iterable.terl",
            source: include_str!("../../../../../std/collections/iterable.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/enumerable.terl",
            source: include_str!("../../../../../std/collections/enumerable.terl"),
        },
    ]
}

/// Emits a compiled module into the temporary workspace and compiles it.
///
/// Inputs:
/// - `path`: source path used to resolve imports.
/// - `workspace`: output directory for `.erl` and `.beam` files.
/// - `compiled`: formal compiler artifacts for the module.
/// - `test_exports`: test-only functions to export in the temporary artifact.
///
/// Output:
/// - `Ok(())` when Erlang source is written and `erlc` succeeds.
/// - `Err(message)` for dependency collection, emission, write, or compile
///   failures.
///
/// Transformation:
/// - Converts CoreIR plus syntax bridge data into Erlang source, writes
///   it to the workspace with optional test-only exports, then runs
///   `erlc -o <workspace>`.
fn emit_compiled_module_to_workspace(
    path: &str,
    workspace: &Path,
    compiled: &crate::formal_pipeline::CheckedSyntaxModuleArtifacts,
    test_exports: &[DiscoveredTest],
) -> Result<(), String> {
    let file_imports = collect_syntax_file_import_bytes(&compiled.syntax_output, Path::new(path))?;
    let templates = collect_syntax_template_inputs(&compiled.syntax_output, Path::new(path))?;
    let markdown_imports =
        collect_syntax_markdown_inputs(&compiled.syntax_output, Path::new(path))?;
    let code = try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &compiled
            .interfaces
            .iter()
            .map(|(name, interface)| (name.clone(), interface.clone()))
            .collect::<BTreeMap<_, _>>(),
        &file_imports,
        &templates,
        &markdown_imports,
    )?;
    let code = add_test_exports_to_erlang_source(code, test_exports);
    let output_stem = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let erl_path = workspace.join(format!("{output_stem}.erl"));
    fs::write(&erl_path, code)
        .map_err(|err| format!("failed to write {}: {err}", erl_path.display()))?;
    compile_erlang_source(workspace, &erl_path)
}

/// Adds test-only exports to generated Erlang source.
///
/// Inputs:
/// - `code`: generated Erlang module source.
/// - `tests`: discovered source-level tests to export in the temporary test
///   artifact.
///
/// Output:
/// - Erlang source with an extra `-export([...]).` attribute when tests are
///   present.
///
/// Transformation:
/// - Inserts one deterministic export attribute after the first line of the
///   module source. This is used only by `terlc test` temporary artifacts so
///   private Terlan tests can execute without widening production emits.
fn add_test_exports_to_erlang_source(code: String, tests: &[DiscoveredTest]) -> String {
    if tests.is_empty() {
        return code;
    }
    let exports = tests
        .iter()
        .map(|test| format!("{}/0", quote_erlang_atom(&test.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let export_attr = format!("-export([{}]).\n", exports);
    if let Some(insert_at) = code.find('\n') {
        let mut with_exports = String::with_capacity(code.len() + export_attr.len());
        with_exports.push_str(&code[..=insert_at]);
        with_exports.push_str(&export_attr);
        with_exports.push_str(&code[insert_at + 1..]);
        with_exports
    } else {
        format!("{code}\n{export_attr}")
    }
}

/// Compiles one Erlang source file into BEAM bytecode.
///
/// Inputs:
/// - `workspace`: target directory for compiled `.beam` output.
/// - `erl_path`: path to the generated Erlang source file.
///
/// Output:
/// - `Ok(())` when `erlc` exits successfully.
/// - `Err(message)` when spawning or compilation fails.
///
/// Transformation:
/// - Executes `erlc -o <workspace> <erl_path>` with crash-dump suppression and
///   formats stderr/stdout into command diagnostics on failure.
fn compile_erlang_source(workspace: &Path, erl_path: &Path) -> Result<(), String> {
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let mut command = Command::new("erlc");
    command.arg("-o").arg(workspace).arg(erl_path);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erlc", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "erlc failed for {}\n{}{}",
        erl_path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Emits and compiles a backend-owned EUnit wrapper module.
///
/// Inputs:
/// - `workspace`: temporary directory for generated Erlang artifacts.
/// - `module_atom`: compiled Erlang module containing exported Terlan tests.
/// - `tests`: discovered source-level tests to expose as EUnit functions.
///
/// Output:
/// - `Ok(wrapper_module_atom)` when the wrapper source and `.beam` are ready.
/// - `Err(message)` when writing or compiling the wrapper fails.
///
/// Transformation:
/// - Generates an Erlang module with one `<test>_test/0` function per Terlan
///   test. Each function delegates to the emitted Terlan module and maps
///   `true` to `ok`, `false` to an EUnit failure, and other values to an
///   unexpected-result failure.
fn emit_and_compile_eunit_wrapper(
    workspace: &Path,
    module_atom: &str,
    tests: &[DiscoveredTest],
) -> Result<String, String> {
    let wrapper_module_atom = format!("{module_atom}_eunit_tests");
    let source = render_eunit_wrapper_source(&wrapper_module_atom, module_atom, tests);
    let erl_path = workspace.join(format!("{wrapper_module_atom}.erl"));
    fs::write(&erl_path, source)
        .map_err(|err| format!("failed to write {}: {err}", erl_path.display()))?;
    compile_erlang_source(workspace, &erl_path)?;
    Ok(wrapper_module_atom)
}

/// Renders the backend-owned EUnit wrapper source.
///
/// Inputs:
/// - `wrapper_module_atom`: Erlang module atom for the generated wrapper.
/// - `target_module_atom`: Erlang module atom for the emitted Terlan module.
/// - `tests`: discovered source-level tests to expose as EUnit functions.
///
/// Output:
/// - Complete Erlang source for the wrapper module.
///
/// Transformation:
/// - Builds deterministic Erlang text without changing Terlan source syntax or
///   standard-library APIs.
fn render_eunit_wrapper_source(
    wrapper_module_atom: &str,
    target_module_atom: &str,
    tests: &[DiscoveredTest],
) -> String {
    let exports = tests
        .iter()
        .map(|test| format!("{}/0", quote_erlang_atom(&format!("{}_test", test.name))))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "-module({}).\n-export([{}]).\n\n",
        quote_erlang_atom(wrapper_module_atom),
        exports
    );
    for test in tests {
        let wrapper_function = quote_erlang_atom(&format!("{}_test", test.name));
        source.push_str(&format!(
            "{}() ->\n    case {}:{}() of\n        true -> ok;\n        false -> erlang:error(assertion_returned_false);\n        Other -> erlang:error({{unexpected_test_result, Other}})\n    end.\n\n",
            wrapper_function,
            quote_erlang_atom(target_module_atom),
            quote_erlang_atom(&test.name)
        ));
    }
    source
}

/// Runs the generated EUnit wrapper without changing normal test output.
///
/// Inputs:
/// - `workspace`: BEAM code path containing the wrapper module.
/// - `wrapper_module_atom`: Erlang module atom for the generated wrapper.
///
/// Output:
/// - `Ok(())` when EUnit reports `ok`.
/// - `Err(message)` when EUnit reports failure or the Erlang runtime cannot be
///   spawned.
///
/// Transformation:
/// - Invokes `eunit:test/2` with `no_tty`, captures output, and converts the
///   EUnit result into a Terlan CLI backend-validation diagnostic.
fn run_eunit_wrapper(workspace: &Path, wrapper_module_atom: &str) -> Result<(), String> {
    let eval = format!(
        "Result = eunit:test({}, [no_tty]), case Result of ok -> halt(0); Other -> io:format(\"eunit wrapper result: ~p~n\", [Other]), halt(1) end.",
        quote_erlang_atom(wrapper_module_atom)
    );
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let mut command = Command::new("erl");
    command
        .arg("-noshell")
        .arg("-pa")
        .arg(workspace)
        .arg("-eval")
        .arg(eval);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erl", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "EUnit wrapper validation failed for {}\n{}{}",
        wrapper_module_atom,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Executes all discovered tests against the compiled BEAM module.
///
/// Inputs:
/// - `workspace`: directory containing compiled `.beam` artifacts.
/// - `module_atom`: Erlang module atom to invoke.
/// - `tests`: discovered source-level test functions.
///
/// Output:
/// - Structured execution report with pass/fail counts and per-test results.
///
/// Transformation:
/// - Calls each exported zero-argument test function via `erl -noshell`,
///   renders a compact status report, and aggregates pass/fail counts.
fn run_discovered_erlang_tests(
    workspace: &Path,
    module_atom: &str,
    tests: &[DiscoveredTest],
) -> TestRunReport {
    println!("running {} tests", tests.len());
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();
    for test in tests {
        match run_single_erlang_test(workspace, module_atom, &test.name) {
            Ok(()) => {
                passed += 1;
                println!("test {} ... ok", test.name);
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Passed,
                    message: None,
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Err(message) => {
                failed += 1;
                println!("test {} ... FAILED", test.name);
                println!("  {message}");
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Failed,
                    message: Some(message),
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
        }
    }

    if failed == 0 {
        println!("test result: ok. {passed} passed; 0 failed");
    } else {
        println!("test result: FAILED. {passed} passed; {failed} failed");
    }
    TestRunReport {
        passed,
        failed,
        results,
    }
}

/// Executes one exported BEAM test function.
///
/// Inputs:
/// - `workspace`: BEAM code path.
/// - `module_atom`: Erlang module atom to call.
/// - `function_atom`: Erlang function atom to call with arity 0.
///
/// Output:
/// - `Ok(())` when the function returns `true`.
/// - `Err(message)` when the function returns `false`, returns another value,
///   crashes, or the Erlang runtime cannot be spawned.
///
/// Transformation:
/// - Builds an Erlang `-eval` expression that maps `true` to exit code 0 and
///   all other outcomes to non-zero exit codes with diagnostic text.
fn run_single_erlang_test(
    workspace: &Path,
    module_atom: &str,
    function_atom: &str,
) -> Result<(), String> {
    let eval = format!(
        "Result = {}:{}(), case Result of true -> halt(0); false -> io:format(\"assertion returned false~n\", []), halt(1); Other -> io:format(\"unexpected test result: ~p~n\", [Other]), halt(2) end.",
        quote_erlang_atom(module_atom),
        quote_erlang_atom(function_atom)
    );
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let mut command = Command::new("erl");
    command
        .arg("-noshell")
        .arg("-pa")
        .arg(workspace)
        .arg("-eval")
        .arg(eval);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erl", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!("{}{}", stdout.trim_end(), stderr.trim_end());
    if message.is_empty() {
        Err(format!("erl exited with status {}", output.status))
    } else {
        Err(message)
    }
}

/// Runs a process while preventing local `erl_crash.dump` files in the workspace.
///
/// Inputs:
/// - `command`: process builder to execute.
/// - `label`: human-readable tool name used in spawn failures.
/// - `erl_crash_dump`: optional path assigned to `ERL_CRASH_DUMP`.
///
/// Output:
/// - `Ok(Output)` when the process starts and exits.
/// - `Err(message)` when the process cannot be spawned.
///
/// Transformation:
/// - Adds the Erlang crash-dump environment override and delegates to
///   `Command::output`.
fn run_command_with_no_erl_crash_dump(
    command: &mut Command,
    label: &str,
    erl_crash_dump: Option<&Path>,
) -> Result<Output, String> {
    if let Some(path) = erl_crash_dump {
        command.env("ERL_CRASH_DUMP", path);
    }
    command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))
}

/// Quotes text as an Erlang atom literal.
///
/// Inputs:
/// - `atom`: untrusted atom text.
///
/// Output:
/// - Single-quoted Erlang atom literal.
///
/// Transformation:
/// - Escapes backslashes and single quotes so generated `erl -eval` code
///   remains syntactically valid.
fn quote_erlang_atom(atom: &str) -> String {
    let mut quoted = String::from("'");
    for ch in atom.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '\'' => quoted.push_str("\\'"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
#[path = "test_command_test.rs"]
mod test_command_test;
