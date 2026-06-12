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
    }
}

/// Parses command-local arguments for `terlc test`.
///
/// Inputs:
/// - `args`: command arguments after `main.rs` has removed global options and
///   command verb.
///
/// Output:
/// - `Ok(TestArgs)` for exactly one source path, optional `--target erlang`,
///   optional `--emit-test-manifest <path>`, and optional
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

    let Some(path) = path else {
        return Err("missing or extra path argument".to_string());
    };
    Ok(TestArgs {
        path,
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
    if !is_test_source_path(path) {
        eprintln!("terlc test requires a *_test.tl source file for 0.0.1: {path}");
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
/// - `true` when the file name ends in `_test.tl`.
///
/// Transformation:
/// - Reads only the final path component and compares its suffix, preserving
///   the documented 0.0.1 rule that tests live in separate `*_test.tl` files.
fn is_test_source_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.tl"))
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

/// Returns the embedded 0.0.1 support module list for `terlc test`.
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
            path: "std/test/test.tl",
            source: include_str!("../../../../../std/test/test.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/bool.tl",
            source: include_str!("../../../../../std/core/bool.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/unit.tl",
            source: include_str!("../../../../../std/core/unit.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/ordering.tl",
            source: include_str!("../../../../../std/core/ordering.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/int.tl",
            source: include_str!("../../../../../std/core/int.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/float.tl",
            source: include_str!("../../../../../std/core/float.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/option.tl",
            source: include_str!("../../../../../std/core/option.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/result.tl",
            source: include_str!("../../../../../std/core/result.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/identity.tl",
            source: include_str!("../../../../../std/core/identity.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/function.tl",
            source: include_str!("../../../../../std/core/function.tl"),
        },
        ReleaseSupportModule {
            path: "std/core/string.tl",
            source: include_str!("../../../../../std/core/string.tl"),
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
mod tests {
    use super::*;

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
        let parsed = parse_test_args(&args(&["tests/sample.tl"])).expect("test args");
        assert_eq!(parsed.path, "tests/sample.tl");
        assert_eq!(parsed.target, TestTarget::Erlang);
        assert_eq!(parsed.emit_test_manifest, None);
        assert_eq!(parsed.emit_test_result_manifest, None);
    }

    #[test]
    fn parse_test_args_accepts_explicit_erlang_target() {
        let parsed =
            parse_test_args(&args(&["tests/sample.tl", "--target", "erlang"])).expect("test args");
        assert_eq!(parsed.path, "tests/sample.tl");
        assert_eq!(parsed.target, TestTarget::Erlang);
        assert_eq!(parsed.emit_test_manifest, None);
        assert_eq!(parsed.emit_test_result_manifest, None);
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
            "tests/sample_test.tl",
            "--emit-test-manifest",
            "target/sample.test-manifest.json",
        ]))
        .expect("test args");
        assert_eq!(parsed.path, "tests/sample_test.tl");
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
            "tests/sample_test.tl",
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
            "tests/sample_test.tl",
            "--emit-test-result-manifest",
            "target/sample.test-results.json",
        ]))
        .expect("test args");
        assert_eq!(parsed.path, "tests/sample_test.tl");
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
            "tests/sample_test.tl",
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
            parse_test_args(&args(&["tests/sample.tl", "--target", "js"])).expect_err("error");
        assert_eq!(error, "unsupported test target: js");
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

    #[test]
    fn test_source_path_requires_test_suffix() {
        assert!(is_test_source_path("tests/std/core/bool_test.tl"));
        assert!(!is_test_source_path("std/core/bool.tl"));
        assert!(!is_test_source_path("tests/std/core/bool_test.md"));
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
            module.path == "std/test/test.tl" && module.source.contains("module std.test.Test.")
        }));
        assert!(modules.iter().any(|module| {
            module.path == "std/core/string.tl" && module.source.contains("module std.core.String.")
        }));
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
            "tests/sample_test.tl",
            "tests.SampleTest",
            "erlang",
            "erlang",
            &[DiscoveredTest {
                name: "sample".to_string(),
                span_start: 12,
                span_end: 34,
            }],
        )
        .expect("write manifest");

        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("manifest text"))
                .expect("manifest json");
        let _ = fs::remove_file(&path);

        assert_eq!(json["source_path"], "tests/sample_test.tl");
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
            "tests/sample_test.tl",
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

        assert_eq!(json["source_path"], "tests/sample_test.tl");
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
}
