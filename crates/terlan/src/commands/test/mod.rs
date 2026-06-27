use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::{CliCommand, CliState};

mod beam_runner;
mod command_runner;
mod discovery;
mod manifest;
mod release_support;
mod style;

#[cfg(test)]
use crate::terlan_syntax::SyntaxTypeOutput;
use beam_runner::{
    emit_and_compile_eunit_wrapper, emit_compiled_module_to_workspace, run_discovered_erlang_tests,
    run_eunit_wrapper, TempBeamWorkspace,
};
#[cfg(test)]
use discovery::is_supported_test_return_type;
use discovery::tests_require_release_support;
pub(super) use discovery::DiscoveredTest;
use discovery::{discover_tests, select_tests};
use manifest::{
    literal_bool_report, print_literal_bool_report, print_validation_pass_report,
    validation_pass_report, write_test_manifest, write_test_result_manifest,
};
use release_support::emit_and_compile_release_support_modules;
use style::TestOutputStyle;

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
    test_name: Option<String>,
    emit_test_manifest: Option<PathBuf>,
    emit_test_result_manifest: Option<PathBuf>,
}

/// Project context discovered for an editor-launched test file.
///
/// Inputs:
/// - Produced from a test source path below a directory containing
///   `terlan.toml`.
///
/// Output:
/// - Resolved cache directory, source roots, and manifest source files.
///
/// Transformation:
/// - Keeps enough project metadata for `terlc test <file>` to behave like a
///   package-local test run instead of compiling the active test file in
///   isolation.
#[derive(Debug, Clone)]
struct TestProjectContext {
    cache_dir: PathBuf,
    source_roots: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
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
///   `--target erlang|js`, optional `--name <test>`, optional
///   `--emit-test-manifest <path>`, and optional `--emit-test-result-manifest
///   <path>`.
/// - `Err(message)` for malformed flags, unsupported targets, or wrong arity.
///
/// Transformation:
/// - Walks raw strings, extracts command-local target selection, and rejects
///   unexpected arguments.
fn parse_test_args(args: &[String]) -> Result<TestArgs, String> {
    let mut path = None;
    let mut target = TestTarget::Erlang;
    let mut test_name = None;
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
            "--name" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--name requires a test function name".to_string());
                };
                if test_name.replace(value.clone()).is_some() {
                    return Err("duplicate --name".to_string());
                }
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
        test_name,
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
        eprintln!("terlc test requires a *Test.terl source file: {path}");
        return ExitCode::from(1);
    }

    let (project_context, state) =
        match prepare_test_project_context(path, state, TargetProfile::Erlang) {
            Ok(result) => result,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };

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
            test_target_profile_options(&state, true),
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
    let tests = match select_tests(tests, args.test_name.as_deref(), path) {
        Ok(tests) => tests,
        Err(message) => {
            eprintln!("{message}");
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
    if !tests_require_release_support(&tests) {
        let report = literal_bool_report(&tests);
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
        let output_style = TestOutputStyle::from_diagnostic_format(state.diagnostic_format);
        print_literal_bool_report(&report, output_style);
        return if report.is_success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    let workspace = match TempBeamWorkspace::create(&compiled.syntax_output.module_name) {
        Ok(workspace) => workspace,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    if let Err(message) =
        emit_compiled_module_to_workspace(path, workspace.path(), &compiled, &tests)
    {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let module_atom = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let requires_release_support = tests_require_release_support(&tests);
    let eunit_module_atom = if requires_release_support {
        if let Err(message) = emit_and_compile_release_support_modules(
            workspace.path(),
            &state,
            &compiled.syntax_output.module_name,
            &compiled.core.imports,
            &source,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
        if let Some(context) = project_context.as_ref() {
            if let Err(message) = emit_project_test_support_modules(
                workspace.path(),
                &state,
                &compiled.syntax_output.module_name,
                &compiled
                    .core
                    .imports
                    .iter()
                    .map(|import| import.module.clone())
                    .collect(),
                context,
            ) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
        match emit_and_compile_eunit_wrapper(workspace.path(), &module_atom, &tests) {
            Ok(module_atom) => module_atom,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
    } else {
        String::new()
    };

    let output_style = TestOutputStyle::from_diagnostic_format(state.diagnostic_format);
    let report = run_discovered_erlang_tests(workspace.path(), &module_atom, &tests, output_style);
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
    if requires_release_support {
        if let Err(message) = run_eunit_wrapper(workspace.path(), &eunit_module_atom) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
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
        eprintln!("terlc test requires a *Test.terl source file for JS validation: {path}");
        return ExitCode::from(1);
    }

    let (_, state) = match prepare_test_project_context(path, state, profile) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

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
            test_target_profile_options(&state, true),
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
    let tests = match select_tests(tests, args.test_name.as_deref(), path) {
        Ok(tests) => tests,
        Err(message) => {
            eprintln!("{message}");
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
    let output_style = TestOutputStyle::from_diagnostic_format(state.diagnostic_format);
    print_validation_pass_report(&report, output_style);
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
/// - Recursively discovers `*Test.terl` files in deterministic order, then
///   delegates each file to the JS validation runner and aggregates status
///   without inventing a directory-level manifest format.
fn run_js_test_directory(args: &TestArgs, state: CliState, profile: TargetProfile) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!("test manifest output is only supported for a single *Test.terl file");
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!("no *Test.terl files found in {}", args.path);
        return ExitCode::from(1);
    }

    let mut failed = false;
    for file in files {
        let file_args = TestArgs {
            path: file.to_string_lossy().into_owned(),
            target: args.target,
            test_name: args.test_name.clone(),
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

/// Prepares manifest source roots for one test file.
///
/// Inputs:
/// - `path`: active test file passed to `terlc test`.
/// - `state`: command state before project-specific cache selection.
/// - `profile`: profile used to validate project source roots.
///
/// Output:
/// - Optional project context and state with an effective cache directory.
/// - Error text when a discovered project manifest or source root is invalid.
///
/// Transformation:
/// - Walks upward from the test file to find `terlan.toml`; when found, reads
///   `[build] source_roots`, validates those roots through the normal check
///   pipeline, and uses the same cache for subsequent test compilation.
fn prepare_test_project_context(
    path: &str,
    state: CliState,
    profile: TargetProfile,
) -> Result<(Option<TestProjectContext>, CliState), String> {
    let Some(mut context) =
        discover_test_project_context(Path::new(path), state.cache_dir.as_ref())?
    else {
        return Ok((None, state));
    };

    let mut project_state = state.clone();
    project_state.target_profile = profile;
    project_state.cache_dir = Some(context.cache_dir.clone());

    for root in &context.source_roots {
        let status = crate::commands::check::run_check_dir(
            &root.to_string_lossy(),
            project_state.clone(),
            None,
        );
        if status != ExitCode::SUCCESS {
            return Err(format!(
                "project source root `{}` failed while preparing tests",
                root.display()
            ));
        }
    }

    context.source_files.sort();
    Ok((Some(context), project_state))
}

/// Discovers the nearest Terlan project containing a test file.
///
/// Inputs:
/// - `test_path`: source path passed to `terlc test`.
/// - `explicit_cache_dir`: optional global cache directory.
///
/// Output:
/// - Project context when an ancestor `terlan.toml` exists.
/// - `None` for standalone test files.
///
/// Transformation:
/// - Canonicalizes the test path, searches ancestors for the project manifest,
///   resolves manifest source roots against the project root, and recursively
///   collects implementation source files from those roots.
fn discover_test_project_context(
    test_path: &Path,
    explicit_cache_dir: Option<&PathBuf>,
) -> Result<Option<TestProjectContext>, String> {
    let canonical_test = match fs::canonicalize(test_path) {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let mut current = canonical_test.parent();
    let mut project_root = None;
    while let Some(dir) = current {
        if dir.join("terlan.toml").is_file() {
            project_root = Some(dir.to_path_buf());
            break;
        }
        current = dir.parent();
    }
    let Some(root) = project_root else {
        return Ok(None);
    };

    let manifest_path = root.join("terlan.toml");
    let manifest = crate::commands::build::project_manifest::read_project_manifest(&manifest_path)
        .map_err(|err| format!("failed to read test project manifest: {err}"))?;
    let cache_dir = explicit_cache_dir
        .cloned()
        .unwrap_or_else(|| root.join(".terlan"));
    let mut source_roots = Vec::new();
    let mut source_files = Vec::new();

    for source_root in &manifest.source_roots {
        let root_path = root.join(source_root);
        if !root_path.is_dir() {
            return Err(format!(
                "test project source root `{}` does not exist or is not a directory",
                source_root
            ));
        }
        let mut files = crate::formal_pipeline::terlan_sources_in_dir(&root_path)?;
        source_files.append(&mut files);
        source_roots.push(root_path);
    }

    Ok(Some(TestProjectContext {
        cache_dir,
        source_roots,
        source_files,
    }))
}

/// Emits manifest source modules into a BEAM test workspace.
///
/// Inputs:
/// - `workspace`: temporary BEAM workspace for the active test run.
/// - `state`: project-prepared test state with interface cache populated.
/// - `primary_module_name`: module under test, skipped to avoid duplicate emit.
/// - `required_modules`: direct imports from the active test module.
/// - `context`: manifest source files discovered for the project.
///
/// Output:
/// - `Ok(())` when all project support modules compile into the workspace.
/// - Error text when any project source fails compilation or emission.
///
/// Transformation:
/// - Walks only the package-local import closure needed by the active test,
///   compiles each implementation module with the same formal pipeline used for
///   the test, then emits them beside the generated test module so runtime
///   remote calls from editor-launched tests can resolve on BEAM.
fn emit_project_test_support_modules(
    workspace: &Path,
    state: &CliState,
    primary_module_name: &str,
    required_modules: &BTreeSet<String>,
    context: &TestProjectContext,
) -> Result<(), String> {
    let mut module_paths = BTreeMap::new();
    for file in &context.source_files {
        if let Some(module_name) = project_source_module_name(context, file) {
            module_paths.insert(module_name, file.clone());
        }
    }

    let mut pending = required_modules.clone();
    let mut emitted = BTreeSet::new();

    loop {
        let Some(module_name) = pending
            .iter()
            .find(|module_name| !emitted.contains(*module_name))
            .cloned()
        else {
            break;
        };
        emitted.insert(module_name.clone());
        if module_name == primary_module_name {
            continue;
        }

        let Some(file) = module_paths.get(&module_name).cloned() else {
            continue;
        };
        let path = file.to_string_lossy();
        let source = crate::support::read_file(&path)?;
        let compiled =
            crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
                &path,
                &source,
                state.diagnostic_format,
                state.cache_dir.as_deref(),
                state.native_policy,
                TargetProfile::Erlang,
                test_target_profile_options(state, true),
            )
            .map_err(|exit_code| {
                format!(
                    "project support module {} failed with exit code {exit_code:?}",
                    file.display()
                )
            })?;

        for import in &compiled.core.imports {
            if module_paths.contains_key(&import.module) && !emitted.contains(&import.module) {
                pending.insert(import.module.clone());
            }
        }

        emit_compiled_module_to_workspace(&path, workspace, &compiled, &[])?;
    }
    Ok(())
}

/// Derives a Terlan module name from a project source path.
///
/// Inputs:
/// - `context`: discovered project source roots.
/// - `file`: one `.terl` file below one of those roots.
///
/// Output:
/// - Fully qualified module name implied by the generated-profile source tree,
///   or `None` when the path is outside every source root or is not UTF-8.
///
/// Transformation:
/// - Mirrors the check command's module-layout contract closely enough for test
///   dependency discovery without reparsing every source file up front.
fn project_source_module_name(context: &TestProjectContext, file: &Path) -> Option<String> {
    for root in &context.source_roots {
        let Ok(relative) = file.strip_prefix(root) else {
            continue;
        };
        let mut segments = Vec::new();
        for component in relative.components() {
            let text = component.as_os_str().to_str()?;
            segments.push(text.to_string());
        }
        let Some(last) = segments.last_mut() else {
            return None;
        };
        let stem = last.strip_suffix(".terl")?;
        *last = stem.to_string();
        return Some(segments.join("."));
    }
    None
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
/// - Recursively discovers `*Test.terl` files in deterministic order, then
///   delegates each file to the existing single-file runner and aggregates the
///   command exit status without inventing a directory-level manifest format.
fn run_erlang_test_directory(args: &TestArgs, state: CliState) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!("test manifest output is only supported for a single *Test.terl file");
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!("no *Test.terl files found in {}", args.path);
        return ExitCode::from(1);
    }

    let mut failed = false;
    for file in files {
        let file_args = TestArgs {
            path: file.to_string_lossy().into_owned(),
            target: args.target,
            test_name: args.test_name.clone(),
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
///   the `*Test.terl` source layout predicate.
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

/// Returns whether a source path is accepted by the 0.0.1 test-file layout.
///
/// Inputs:
/// - `path`: user-provided source path passed to `terlc test`.
///
/// Output:
/// - `true` when the file stem ends in `Test` and the extension is `.terl`.
///
/// Transformation:
/// - Reads only the final path component and compares the canonical Terlan
///   test-module suffix without accepting the old underscore convention.
fn is_test_source_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.ends_with("Test"))
                && name.ends_with(".terl")
        })
}

/// Builds target-profile options for test compilation paths.
///
/// Inputs:
/// - `state`: CLI state carrying native policy.
/// - `allow_asset_imports`: whether the test path owns asset import resolution.
///
/// Output:
/// - Target-profile validation options for primary and support test modules.
///
/// Transformation:
/// - Keeps test validation aligned with package build: SafeNative-backed std
///   APIs are admitted only when native policy is not pure.
fn test_target_profile_options(
    state: &CliState,
    allow_asset_imports: bool,
) -> TargetProfileCheckOptions {
    TargetProfileCheckOptions {
        allow_asset_imports,
        allow_rust_backed_std_modules: state.native_policy != NativePolicy::Pure,
    }
}

#[cfg(test)]
#[path = "test_command_test.rs"]
mod test_command_test;
