use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::arguments::{parse_test_args, TestArgs, TestTarget, TEST_SOURCE_PATTERN_DESCRIPTION};
use super::discovery::{discover_tests, select_tests, TestKind};
use super::manifest::{
    print_validation_pass_report, validation_pass_report, write_test_manifest,
    write_test_result_manifest, TestRunReport, TestRunStatus,
};
use super::project_context::{
    is_test_source_path, prepare_test_project_context, run_js_test_directory,
    run_terlan_vm_test_directory, test_target_profile_options,
};
use super::style::TestOutputStyle;
use super::vm_runner::run_discovered_terlan_vm_tests;
use super::wasm::run_wasm_tests;
use crate::commands::dev_dependencies;
use crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic;
use crate::terlan_typeck::{CoreImportKind, CoreModule};
use crate::validation::target_profile::TargetProfile;
use crate::{CliCommand, CliState};

/// Project context discovered for an editor-launched test file.
///
/// Inputs:
/// - Produced from a test source path below a directory containing
///   `terlan.toml`.
///
/// Output:
/// - Resolved cache directory, source roots, and native artifact bindings.
///
/// Transformation:
/// - Keeps enough project metadata for `terlc test <file>` to behave like a
///   package-local test run, including target runtimes selected by package
///   resolution, instead of compiling the active test file in isolation.
#[derive(Debug, Clone)]
pub(super) struct TestProjectContext {
    pub(super) cache_dir: PathBuf,
    pub(super) source_roots: Vec<PathBuf>,
    pub(super) native_helper_environment: Vec<(String, PathBuf)>,
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
/// - `ExitCode::from(1)` for compile, discovery, emit, backend compile, or
///   test execution failures.
///
/// Transformation:
/// - Routes each explicit source module or directory through the formal
///   compiler path, discovers `@test` declarations, and executes them against
///   the selected test target without starting another compiler process.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_test_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    let paths = std::iter::once(args.path.clone())
        .chain(args.additional_paths.iter().cloned())
        .collect::<Vec<_>>();
    let mut failed = false;
    for path in paths {
        let mut path_args = args.clone();
        path_args.path = path;
        path_args.additional_paths.clear();
        if run_path(&path_args, state.clone()) != ExitCode::SUCCESS {
            failed = true;
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_path(args: &TestArgs, state: CliState) -> ExitCode {
    let dependency_session =
        match dev_dependencies::start_project_dependencies_for_path(Path::new(&args.path)) {
            Ok(session) => Some(session),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };

    let outcome = match args.target {
        TestTarget::TerlanVm => run_terlan_vm_tests(args, state),
        TestTarget::Js => run_js_tests(args, state),
        TestTarget::Wasm => run_wasm_tests(args, state),
    };
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
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
pub(super) fn run_js_tests(args: &TestArgs, state: CliState) -> ExitCode {
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
        eprintln!(
            "terlc test requires a {TEST_SOURCE_PATTERN_DESCRIPTION} source file for JS validation: {path}"
        );
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
    let kind = if args.benchmark {
        TestKind::Benchmark
    } else {
        TestKind::Test
    };
    let tests = match select_tests(tests, &args.test_names, path, kind) {
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
        eprintln!("no {} declarations found in {path}", kind.annotation());
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

/// Executes discovered tests through the compiler-owned Terlan VM.
///
/// Inputs:
/// - `args`: parsed test arguments, including one source file or directory and
///   optional manifest output paths.
/// - `state`: global CLI state used for diagnostics, cache, native policy, and
///   target-profile checks.
///
/// Output:
/// - `ExitCode::SUCCESS` when every selected test returns `true`.
/// - `ExitCode::from(1)` when compilation, discovery, VM loading, VM
///   execution, or manifest writing fails.
///
/// Transformation:
/// - Compiles each test module to a native `.tvm` image. Every selected test
///   must be a native export; runtime CoreIR interpretation is forbidden.
pub(super) fn run_terlan_vm_tests(args: &TestArgs, state: CliState) -> ExitCode {
    let path = args.path.as_str();
    if Path::new(path).is_dir() {
        return run_terlan_vm_test_directory(args, state);
    }
    if !is_test_source_path(path) {
        eprintln!(
            "terlc test requires a {TEST_SOURCE_PATTERN_DESCRIPTION} source file for Terlan VM execution: {path}"
        );
        return ExitCode::from(1);
    }

    let (project_context, state) =
        match prepare_test_project_context(path, state, TargetProfile::Vm) {
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
            TargetProfile::Vm,
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
    let kind = if args.benchmark {
        TestKind::Benchmark
    } else {
        TestKind::Test
    };
    let tests = match select_tests(tests, &args.test_names, path, kind) {
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
            "terlan-vm",
            TargetProfile::Vm.as_str(),
            &tests,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    if tests.is_empty() {
        eprintln!("no {} declarations found in {path}", kind.annotation());
        return ExitCode::from(1);
    }

    let project_core_modules = match project_context.as_ref() {
        Some(context) => match compile_project_source_core_modules(context, &state) {
            Ok(modules) => modules,
            Err(exit_code) => return exit_code,
        },
        None => Vec::new(),
    };
    let std_import_roots = std::iter::once(&compiled.core)
        .chain(project_core_modules.iter())
        .collect::<Vec<_>>();
    let imported_std_core_modules = match compile_imported_std_source_core_modules(
        &std_import_roots,
        Path::new(path),
        &state,
    ) {
        Ok(modules) => modules,
        Err(exit_code) => return exit_code,
    };
    let support_core_modules = project_core_modules
        .iter()
        .chain(imported_std_core_modules.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut core = compiled.core;
    let test_roots = tests
        .iter()
        .map(|test| test.name.as_str())
        .collect::<Vec<_>>();
    crate::compiler::native_ir::prune_module_to_function_roots(&mut core, &test_roots);
    let application_cores = std::iter::once(&core)
        .chain(support_core_modules.iter())
        .collect::<Vec<_>>();

    let module_stem = compiled.syntax_output.module_name.replace('.', "_");
    let test_aot_workspace = state.out_dir.join("test-aot").join(&module_stem);
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let native_image =
        match crate::commands::build::vm_artifact::native_image::compile_test_native_image(
            &test_aot_workspace,
            &native_cache_root,
            &module_stem,
            &application_cores,
            state.incremental,
        ) {
            Ok(image) => image,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };

    let report = match run_discovered_terlan_vm_tests(
        compiled.syntax_output.module_name.as_str(),
        &tests,
        native_image.as_deref(),
        project_context
            .as_ref()
            .map_or(&[], |context| context.native_helper_environment.as_slice()),
        if args.benchmark {
            Some((args.benchmark_warmup, args.benchmark_samples))
        } else {
            None
        },
    ) {
        Ok(report) => report,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if let Some(result_manifest_path) = args.emit_test_result_manifest.as_deref() {
        if let Err(message) = write_test_result_manifest(
            result_manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "terlan-vm",
            TargetProfile::Vm.as_str(),
            &report,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    let output_style = TestOutputStyle::from_diagnostic_format(state.diagnostic_format);
    if args.benchmark {
        print_benchmark_report(&report, output_style);
    } else {
        print_runtime_test_report(&report, output_style);
    }
    if report.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Compiles project source-root modules for VM test execution.
///
/// Inputs:
/// - `context`: project test context discovered from `terlan.toml`.
/// - `state`: VM test command state with project cache and target profile.
///
/// Output:
/// - Checked CoreIR modules for all source-root `.terl` files, or a command
///   exit code when any source fails.
///
/// Transformation:
/// - Reuses the formal compiler path after project source-root validation so
///   tests can execute imported project functions inside the VM without
///   falling back to generated Erlang, BEAM artifacts, or host-side stubs.
pub(super) fn compile_project_source_core_modules(
    context: &TestProjectContext,
    state: &CliState,
) -> Result<Vec<CoreModule>, ExitCode> {
    let mut modules = Vec::new();
    for root in &context.source_roots {
        let files = match crate::formal_pipeline::terlan_sources_in_dir(root) {
            Ok(files) => files,
            Err(message) => {
                eprintln!("{message}");
                return Err(ExitCode::from(1));
            }
        };
        for file in files {
            let path = file.to_string_lossy().into_owned();
            let source = match crate::support::read_file(&path) {
                Ok(source) => source,
                Err(message) => {
                    eprintln!("{message}");
                    return Err(ExitCode::from(1));
                }
            };
            let compiled =
                crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
                    &path,
                    &source,
                    state.diagnostic_format,
                    state.cache_dir.as_deref(),
                    state.native_policy,
                    TargetProfile::Vm,
                    test_target_profile_options(state, true),
                )?;
            modules.push(compiled.core);
        }
    }
    Ok(modules)
}

/// Compiles imported standard-library sources needed by standalone std tests.
///
/// Inputs:
/// - `test_core`: active test module with resolved CoreIR imports.
/// - `test_path`: active test source path.
/// - `state`: VM test command state with cache and native policy.
///
/// Output:
/// - Checked CoreIR modules for imported std sources that exist beside the
///   repository std tree.
///
/// Transformation:
/// - Maps runtime module imports such as `std.range.Range` to
///   `std/range/Range.terl`, skips intrinsic-only std modules that have no
///   source file in the current checkout, and compiles found sources for VM
///   execution before test dispatch.
pub(super) fn compile_imported_std_source_core_modules(
    root_cores: &[&CoreModule],
    test_path: &Path,
    state: &CliState,
) -> Result<Vec<CoreModule>, ExitCode> {
    let mut modules = Vec::new();
    let mut seen = BTreeSet::new();
    let mut pending = root_cores
        .iter()
        .flat_map(|core| &core.imports)
        .filter(|import| import.kind == CoreImportKind::Module)
        .map(|import| import.module.clone())
        .collect::<VecDeque<_>>();
    let active_file = fs::canonicalize(test_path).ok();
    while let Some(module) = pending.pop_front() {
        let Some(path) = imported_std_source_path(&module, test_path) else {
            continue;
        };
        if active_file.as_ref().is_some_and(|active| {
            fs::canonicalize(&path)
                .ok()
                .as_ref()
                .is_some_and(|candidate| candidate == active)
        }) {
            continue;
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        let path_text = path.to_string_lossy().into_owned();
        let source = match crate::support::read_file(&path_text) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{message}");
                return Err(ExitCode::from(1));
            }
        };
        let compiled =
            crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
                &path_text,
                &source,
                state.diagnostic_format,
                state.cache_dir.as_deref(),
                state.native_policy,
                TargetProfile::Vm,
                test_target_profile_options(state, true),
            )?;
        let mut core = compiled.core;
        pending.extend(
            core.imports
                .iter()
                .filter(|import| import.kind == CoreImportKind::Module)
                .map(|import| import.module.clone()),
        );
        remove_compiler_intrinsic_functions(&mut core);
        if !core.functions.is_empty() {
            modules.push(core);
        }
    }
    Ok(modules)
}

/// Removes compiler-owned intrinsic declarations from an imported std module.
///
/// Inputs:
/// - `module`: mutable checked standard-library CoreIR module.
///
/// Output:
/// - No return value; functions registered as compiler-owned primitive
///   intrinsics are removed in place.
///
/// Transformation:
/// - Uses the canonical module/name/arity intrinsic registry so placeholder
///   declaration bodies cannot shadow VM intrinsic evaluation when `terlc
///   test` merges executable std helpers into a standalone test module.
pub(super) fn remove_compiler_intrinsic_functions(module: &mut CoreModule) {
    let module_name = module.module.clone();
    module.functions.retain(|function| {
        core_primitive_intrinsic(&module_name, &function.name, function.arity).is_none()
    });
}

/// Resolves one imported std module to a source file in the current checkout.
///
/// Inputs:
/// - `module`: fully qualified std module name from CoreIR imports.
/// - `test_path`: active test path used to locate the repository std root.
///
/// Output:
/// - Source path when the module has a checked-in `.terl` implementation.
///
/// Transformation:
/// - Converts dots to path separators and tries the std root containing the
///   active test first, then the process working directory for installed
///   compiler development workflows.
pub(super) fn imported_std_source_path(module: &str, test_path: &Path) -> Option<PathBuf> {
    if !module.starts_with("std.") {
        return None;
    }
    let relative = PathBuf::from(format!("{}.terl", module.replace('.', "/")));
    let mut candidates = Vec::new();
    if let Some(root) = repository_root_from_std_path(test_path) {
        candidates.push(root.join(&relative));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(&relative));
    }
    if let Some(share_root) = crate::commands::release_layout::installed_share_root() {
        candidates.push(share_root.join(&relative));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(&relative),
    );
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Finds the repository root when a test lives below a `std` directory.
///
/// Inputs:
/// - `path`: test file path.
///
/// Output:
/// - Parent of the nearest `std` directory, if present.
///
/// Transformation:
/// - Walks path ancestors without touching the filesystem so relative test
///   paths and canonical paths both map to the same std tree shape.
fn repository_root_from_std_path(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|name| name.to_str()) == Some("std") {
            return dir.parent().map(Path::to_path_buf);
        }
        current = dir.parent();
    }
    None
}

/// Prints a runtime execution test report.
///
/// Inputs:
/// - `report`: completed runtime test report.
/// - `style`: terminal color policy for pass/fail labels.
///
/// Output:
/// - Human-readable test status lines written to stdout.
///
/// Transformation:
/// - Renders the same compact pass/fail shape as the VM runner without
///   exposing backend-specific crash details.
pub(super) fn print_runtime_test_report(report: &TestRunReport, style: TestOutputStyle) {
    println!("running {} tests", report.results.len());
    for result in &report.results {
        match result.status {
            TestRunStatus::Passed => {
                println!("test {} ... {}", result.name, style.success("ok"));
            }
            TestRunStatus::Failed => {
                println!("test {} ... {}", result.name, style.failure("FAILED"));
                if let Some(message) = result.message.as_deref() {
                    println!("  {message}");
                }
            }
        }
    }
    if report.failed == 0 {
        println!(
            "test result: {}. {} passed; 0 failed",
            style.success("ok"),
            report.passed
        );
    } else {
        println!(
            "test result: {}. {} passed; {} failed",
            style.failure("FAILED"),
            report.passed,
            report.failed
        );
    }
}

/// Prints native benchmark samples with stable min/median/p95 timing fields.
pub(super) fn print_benchmark_report(report: &TestRunReport, style: TestOutputStyle) {
    println!("running {} benchmarks", report.results.len());
    for result in &report.results {
        match result.status {
            TestRunStatus::Passed => println!(
                "benchmark {} ... {} ({} samples; min {} ns; median {} ns; p95 {} ns)",
                result.name,
                style.success("ok"),
                result.benchmark_samples.unwrap_or(0),
                result.benchmark_min_nanoseconds.unwrap_or(0),
                result.execution_nanoseconds,
                result.benchmark_p95_nanoseconds.unwrap_or(0),
            ),
            TestRunStatus::Failed => {
                println!("benchmark {} ... {}", result.name, style.failure("FAILED"));
                if let Some(message) = result.message.as_deref() {
                    println!("  {message}");
                }
            }
        }
    }
    if report.failed == 0 {
        println!(
            "benchmark result: {}. {} passed; 0 failed",
            style.success("ok"),
            report.passed
        );
    } else {
        println!(
            "benchmark result: {}. {} passed; {} failed",
            style.failure("FAILED"),
            report.passed,
            report.failed
        );
    }
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
/// - Treats the default global CoreV0 profile as an ergonomic request for
///   `js.shared`, while preserving explicit JS profile choices and rejecting
///   unrelated backend profiles.
pub(super) fn effective_js_test_profile(profile: TargetProfile) -> Result<TargetProfile, String> {
    if profile == TargetProfile::CoreV0 {
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
