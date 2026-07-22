use std::path::Path;
use std::process::ExitCode;

use super::{
    collect_test_files, discover_tests, is_test_source_path, prepare_test_project_context,
    print_runtime_test_report, select_tests, test_target_profile_options, write_test_manifest,
    write_test_result_manifest, CliState, DiscoveredTest, TargetProfile, TestArgs, TestOutputStyle,
    TestRunReport, TestRunResult, TestRunStatus, TEST_SOURCE_PATTERN_DESCRIPTION,
};

/// Compiles and executes source-level tests through the Wasm CoreIR lane.
pub(super) fn run_wasm_tests(args: &TestArgs, state: CliState) -> ExitCode {
    let path = args.path.as_str();
    if Path::new(path).is_dir() {
        return run_wasm_test_directory(args, state);
    }
    if !is_test_source_path(path) {
        eprintln!(
            "terlc test requires a {TEST_SOURCE_PATTERN_DESCRIPTION} source file for Wasm execution: {path}"
        );
        return ExitCode::from(1);
    }
    let (_, state) = match prepare_test_project_context(path, state, TargetProfile::WasmCore) {
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
            TargetProfile::WasmCore,
            test_target_profile_options(&state, false),
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };
    let tests = match discover_tests(&compiled.syntax_output).and_then(|tests| {
        select_tests(tests, args.test_name.as_deref(), path).map_err(|error| vec![error])
    }) {
        Ok(tests) => tests,
        Err(messages) => {
            for message in messages {
                eprintln!("{message}");
            }
            return ExitCode::from(1);
        }
    };
    if tests.is_empty() {
        eprintln!("no @test declarations found in {path}");
        return ExitCode::from(1);
    }
    if let Some(manifest_path) = args.emit_test_manifest.as_deref() {
        if let Err(message) = write_test_manifest(
            manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "wasm",
            TargetProfile::WasmCore.as_str(),
            &tests,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    let artifact = match crate::commands::build::wasm_artifact::write_checked_wasm_core_artifact(
        &compiled.core,
        &state,
    ) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let report = run_discovered_wasm_tests(&artifact, &tests);
    if let Some(result_manifest_path) = args.emit_test_result_manifest.as_deref() {
        if let Err(message) = write_test_result_manifest(
            result_manifest_path,
            path,
            &compiled.syntax_output.module_name,
            "wasm",
            TargetProfile::WasmCore.as_str(),
            &report,
        ) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    print_runtime_test_report(
        &report,
        TestOutputStyle::from_diagnostic_format(state.diagnostic_format),
    );
    if report.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Executes each discovered Boolean Wasm test export and records its result.
fn run_discovered_wasm_tests(artifact: &Path, tests: &[DiscoveredTest]) -> TestRunReport {
    let mut passed = 0;
    let mut failed = 0;
    let results = tests
        .iter()
        .map(|test| {
            let outcome = crate::commands::wasm_runtime::execute_test_export(artifact, &test.name);
            let (status, message) = match outcome {
                Ok(()) => {
                    passed += 1;
                    (TestRunStatus::Passed, None)
                }
                Err(message) => {
                    failed += 1;
                    (TestRunStatus::Failed, Some(message))
                }
            };
            TestRunResult {
                name: test.name.clone(),
                status,
                message,
                span_start: test.span_start,
                span_end: test.span_end,
            }
        })
        .collect();
    TestRunReport {
        passed,
        failed,
        results,
    }
}

/// Discovers Wasm test files recursively and executes each through the same lane.
fn run_wasm_test_directory(args: &TestArgs, state: CliState) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!(
            "test manifest output is only supported for a single {TEST_SOURCE_PATTERN_DESCRIPTION} file"
        );
        return ExitCode::from(1);
    }
    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!(
            "no {TEST_SOURCE_PATTERN_DESCRIPTION} files found in {}",
            args.path
        );
        return ExitCode::from(1);
    }
    let mut success = true;
    for file in files {
        let mut file_args = args.clone();
        file_args.path = file.display().to_string();
        success &= run_wasm_tests(&file_args, state.clone()) == ExitCode::SUCCESS;
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
