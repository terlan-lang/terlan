use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sha2::{Digest, Sha256};

use super::arguments::TestArgs;
use super::coverage::DeclarationCoverage;
use super::discovery::{discover_tests, select_tests, DiscoveredTest, TestKind};
use super::execution::{
    compile_imported_std_source_core_modules, finish_declaration_coverage, print_benchmark_report,
    print_runtime_test_report, TestProjectContext,
};
use super::manifest::{print_validation_report, validation_report};
use super::project_closure::{
    compile_project_source_core_closure, compile_project_source_core_modules,
};
use super::project_context::{
    discover_test_project_context, discover_test_project_root, prepare_test_project_context,
    reachable_native_helper_environment, test_target_profile_options,
};
use super::style::TestOutputStyle;
use super::vm_runner::run_discovered_terlan_vm_tests;
use crate::commands::build::vm_artifact::native_debug::NativeDebugInput;
use crate::commands::build::vm_artifact::native_image::compile_test_native_image_owned;
use crate::validation::target_profile::TargetProfile;
use crate::CliState;

#[derive(PartialEq, Eq)]
enum TestFileGroup {
    Project(PathBuf),
    Standalone(PathBuf),
}

fn test_file_group(file: &Path) -> TestFileGroup {
    discover_test_project_root(file).map_or_else(
        || {
            TestFileGroup::Standalone(
                file.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            )
        },
        TestFileGroup::Project,
    )
}

/// Validates a JS test directory once per owning project and reuses unchanged
/// module validation evidence.
pub(super) fn run_js_test_files(
    args: &TestArgs,
    mut state: CliState,
    profile: TargetProfile,
    files: Vec<PathBuf>,
) -> ExitCode {
    state.target_profile = profile;
    let mut groups: Vec<(TestFileGroup, Vec<PathBuf>)> = Vec::new();
    for file in files {
        let context = test_file_group(&file);
        if let Some((_, grouped)) = groups
            .iter_mut()
            .find(|(candidate, _)| candidate == &context)
        {
            grouped.push(file);
        } else {
            groups.push((context, vec![file]));
        }
    }

    let cache_enabled = args.test_names.is_empty()
        && std::env::var("TERLAN_TEST_FORCE_RERUN").as_deref() != Ok("1");
    let target = format!("js-{}", profile.as_str());
    for (_, files) in groups {
        let discovered_context =
            match discover_test_project_context(&files[0], state.cache_dir.as_ref()) {
                Ok(context) => context,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
        let cache_root = result_cache_root(discovered_context.as_ref(), &state);
        let fingerprint_base = if cache_enabled {
            match test_result_fingerprint_base(discovered_context.as_ref(), &state, &target, &files)
            {
                Ok(base) => Some(base),
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        } else {
            None
        };
        let mut pending = Vec::new();
        for file in files {
            let fingerprint = if let Some(base) = fingerprint_base.as_ref() {
                match test_result_fingerprint(&file, base) {
                    Ok(fingerprint) => Some(fingerprint),
                    Err(message) => {
                        eprintln!("{message}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                None
            };
            if fingerprint.as_ref().is_some_and(|fingerprint| {
                cached_test_result_exists(&cache_root, &target, fingerprint)
            }) {
                println!("validated test module {} ... cached", file.display());
            } else {
                pending.push((file, fingerprint));
            }
        }
        let Some((first, _)) = pending.first() else {
            continue;
        };
        let first_path = first.to_string_lossy().into_owned();
        let (_, project_state) =
            match prepare_test_project_context(&first_path, state.clone(), profile, true) {
                Ok(result) => result,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
        for (file, fingerprint) in pending {
            let path = file.to_string_lossy().into_owned();
            let source = match crate::support::read_file(&path) {
                Ok(source) => source,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            let compiled = match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
                &path,
                &source,
                project_state.diagnostic_format,
                project_state.cache_dir.as_deref(),
                project_state.native_policy,
                profile,
                test_target_profile_options(&project_state, true),
            ) {
                Ok(compiled) => compiled,
                Err(exit_code) => return exit_code,
            };
            let tests = match discover_tests(&compiled.syntax_output)
                .map_err(|messages| messages.join("\n"))
                .and_then(|tests| select_tests(tests, &args.test_names, &path, TestKind::Test))
            {
                Ok(tests) => tests,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            if tests.is_empty() {
                eprintln!("no @test declarations found in {path}");
                return ExitCode::from(1);
            }
            let report = validation_report(&tests);
            print_validation_report(
                &report,
                TestOutputStyle::from_diagnostic_format(project_state.diagnostic_format),
            );
            if let Some(fingerprint) = fingerprint.as_deref() {
                if let Err(message) = store_cached_test_result(&cache_root, &target, fingerprint) {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        }
    }
    eprintln!("error[test.js.execution_unavailable]: JavaScript test declarations compiled but were not executed; compilation is not a passing test run");
    ExitCode::from(1)
}

/// Executes a discovered directory as one shared native suite per project.
///
/// Each project source closure and each test module is compiled once, and all
/// selected test exports are linked into one cached native image. This avoids
/// the former `test files × project modules` serial compilation cost while
/// preserving per-module discovery, execution reports, and coverage evidence.
pub(super) fn run_terlan_vm_test_files(
    args: &TestArgs,
    state: CliState,
    files: Vec<PathBuf>,
    exhaustive_project: bool,
) -> ExitCode {
    let mut groups: Vec<(TestFileGroup, Vec<PathBuf>)> = Vec::new();
    for file in files {
        let context = test_file_group(&file);
        if let Some((_, grouped)) = groups
            .iter_mut()
            .find(|(candidate, _)| candidate == &context)
        {
            grouped.push(file);
        } else {
            groups.push((context, vec![file]));
        }
    }

    let mut failed = false;
    let mut coverage = args.coverage.then(DeclarationCoverage::default);
    for (_, files) in groups {
        if run_terlan_vm_test_file_group(
            args,
            state.clone(),
            &files,
            exhaustive_project,
            coverage.as_mut(),
        ) != ExitCode::SUCCESS
        {
            failed = true;
        }
    }
    let status = if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    };
    finish_declaration_coverage(status, args.coverage_threshold, coverage.as_ref())
}

struct SuiteTestModule {
    module_name: String,
    core: crate::terlan_typeck::CoreModule,
    native_debug: NativeDebugInput,
    tests: Vec<DiscoveredTest>,
    result_fingerprint: Option<String>,
}

struct RunnableTestModule {
    module_name: String,
    tests: Vec<DiscoveredTest>,
    result_fingerprint: Option<String>,
}

#[derive(Debug)]
struct TestCacheError(String);

impl fmt::Display for TestCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type TestCacheResult<T> = Result<T, TestCacheError>;

const TEST_RESULT_CACHE_SCHEMA: &str = "terlan-test-result-v1";

fn hash_framed(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn result_cache_root(context: Option<&TestProjectContext>, state: &CliState) -> PathBuf {
    context
        .map(|context| context.cache_dir.clone())
        .or_else(|| state.cache_dir.clone())
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("test-results")
}

fn test_result_fingerprint_base(
    context: Option<&TestProjectContext>,
    state: &CliState,
    target: &str,
    active_tests: &[PathBuf],
) -> TestCacheResult<Sha256> {
    let mut digest = Sha256::new();
    for value in [
        TEST_RESULT_CACHE_SCHEMA.as_bytes(),
        env!("CARGO_PKG_VERSION").as_bytes(),
        env!("TERLAN_CHECKED_FRONTEND_REVISION_SHA256").as_bytes(),
        env!("TERLAN_NATIVE_CODEGEN_REVISION_SHA256").as_bytes(),
        env!("TERLAN_TEST_RUNNER_REVISION_SHA256").as_bytes(),
        env!("TERLAN_NATIVE_BUILD_POLICY_SHA256").as_bytes(),
        target.as_bytes(),
        std::env::consts::OS.as_bytes(),
        std::env::consts::ARCH.as_bytes(),
        format!("{:?}", state.native_policy).as_bytes(),
    ] {
        hash_framed(&mut digest, value);
    }

    let active_tests = active_tests
        .iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect::<BTreeSet<_>>();
    let mut inputs = Vec::new();
    if let Some(context) = context {
        inputs.push(context.project_root.join("terlan.toml"));
        let root_lock = context.project_root.join("terlan.lock");
        if root_lock.is_file() {
            inputs.push(root_lock);
        }
        for root in &context.compilation_source_roots {
            inputs.extend(
                crate::formal_pipeline::terlan_sources_in_dir(root).map_err(TestCacheError)?,
            );
            if let Some(package_root) = root
                .ancestors()
                .find(|candidate| candidate.join("terlan.toml").is_file())
            {
                inputs.push(package_root.join("terlan.toml"));
                let lock = package_root.join("terlan.lock");
                if lock.is_file() {
                    inputs.push(lock);
                }
            }
        }
        for (_, helper) in &context.native_helper_environment {
            inputs.push(helper.clone());
        }
        for candidate in &context.native_helper_candidates {
            inputs.extend(candidate.fingerprint_paths());
        }
    }
    inputs.sort();
    inputs.dedup();
    for input in inputs {
        if fs::canonicalize(&input)
            .ok()
            .is_some_and(|path| active_tests.contains(&path))
        {
            continue;
        }
        hash_framed(&mut digest, input.to_string_lossy().as_bytes());
        let bytes = fs::read(&input).map_err(|error| {
            TestCacheError(format!(
                "error[test.cache.input]: cannot read `{}`: {error}",
                input.display()
            ))
        })?;
        hash_framed(&mut digest, &bytes);
    }
    Ok(digest)
}

fn test_result_fingerprint(test_path: &Path, base: &Sha256) -> TestCacheResult<String> {
    let mut digest = base.clone();
    hash_framed(&mut digest, test_path.to_string_lossy().as_bytes());
    let source = fs::read(test_path).map_err(|error| {
        TestCacheError(format!(
            "error[test.cache.input]: cannot read `{}`: {error}",
            test_path.display()
        ))
    })?;
    hash_framed(&mut digest, &source);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn result_cache_path(cache_root: &Path, target: &str, fingerprint: &str) -> PathBuf {
    cache_root
        .join(target)
        .join(format!("{fingerprint}.success"))
}

fn cached_test_result_exists(cache_root: &Path, target: &str, fingerprint: &str) -> bool {
    fs::read_to_string(result_cache_path(cache_root, target, fingerprint))
        .is_ok_and(|contents| contents == TEST_RESULT_CACHE_SCHEMA)
}

fn store_cached_test_result(
    cache_root: &Path,
    target: &str,
    fingerprint: &str,
) -> TestCacheResult<()> {
    let path = result_cache_path(cache_root, target, fingerprint);
    let parent = path.parent().ok_or_else(|| {
        TestCacheError("error[test.cache.path]: result cache has no parent".to_string())
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        TestCacheError(format!(
            "error[test.cache.create]: cannot create `{}`: {error}",
            parent.display()
        ))
    })?;
    let temporary = path.with_extension(format!("success.{}.tmp", std::process::id()));
    fs::write(&temporary, TEST_RESULT_CACHE_SCHEMA).map_err(|error| {
        TestCacheError(format!(
            "error[test.cache.write]: cannot write `{}`: {error}",
            temporary.display()
        ))
    })?;
    match fs::rename(&temporary, &path) {
        Ok(()) => Ok(()),
        Err(_) if cached_test_result_exists(cache_root, target, fingerprint) => {
            let _ = fs::remove_file(&temporary);
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(TestCacheError(format!(
                "error[test.cache.publish]: cannot publish `{}`: {error}",
                path.display()
            )))
        }
    }
}

fn run_terlan_vm_test_file_group(
    args: &TestArgs,
    state: CliState,
    files: &[PathBuf],
    exhaustive_project: bool,
    coverage: Option<&mut DeclarationCoverage>,
) -> ExitCode {
    let Some(first_file) = files.first() else {
        return ExitCode::SUCCESS;
    };
    let discovered_context =
        match discover_test_project_context(first_file, state.cache_dir.as_ref()) {
            Ok(context) => context,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
    let cache_enabled = !args.coverage
        && !args.benchmark
        && args.test_names.is_empty()
        && std::env::var("TERLAN_TEST_FORCE_RERUN").as_deref() != Ok("1");
    let cache_root = result_cache_root(discovered_context.as_ref(), &state);
    let fingerprint_base = if cache_enabled {
        match test_result_fingerprint_base(discovered_context.as_ref(), &state, "terlan-vm", files)
        {
            Ok(base) => Some(base),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };
    let mut pending = Vec::new();
    for file in files {
        let fingerprint = if let Some(base) = fingerprint_base.as_ref() {
            match test_result_fingerprint(file, base) {
                Ok(fingerprint) => Some(fingerprint),
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        } else {
            None
        };
        if fingerprint.as_ref().is_some_and(|fingerprint| {
            cached_test_result_exists(&cache_root, "terlan-vm", fingerprint)
        }) {
            println!("test module {} ... cached", file.display());
        } else {
            pending.push((file.clone(), fingerprint));
        }
    }
    if pending.is_empty() {
        println!("test result: ok. {} modules cached", files.len());
        return ExitCode::SUCCESS;
    }

    let first_path = pending[0].0.to_string_lossy().into_owned();
    let (project_context, mut state) = match prepare_test_project_context(
        &first_path,
        state,
        TargetProfile::Vm,
        exhaustive_project,
    ) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    state.target_profile = TargetProfile::Vm;

    let kind = if args.benchmark {
        TestKind::Benchmark
    } else {
        TestKind::Test
    };
    let mut test_modules = Vec::with_capacity(pending.len());
    for (file, result_fingerprint) in pending {
        let path = file.to_string_lossy().into_owned();
        let source = match crate::support::read_file(&path) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let compiled = match crate::commands::build::vm_artifact::compile_checked_vm_artifacts(
            &path,
            &source,
            &state,
            test_target_profile_options(&state, true),
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };
        let tests = match discover_tests(&compiled.syntax_output)
            .map_err(|messages| messages.join("\n"))
            .and_then(|tests| select_tests(tests, &args.test_names, &path, kind))
        {
            Ok(tests) => tests,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        if tests.is_empty() {
            eprintln!("no {} declarations found in {path}", kind.annotation());
            return ExitCode::from(1);
        }
        let module_name = compiled.syntax_output.module_name.clone();
        let native_debug =
            NativeDebugInput::capture_owned(path, source, &compiled.core, &compiled.syntax_output);
        test_modules.push(SuiteTestModule {
            module_name,
            core: compiled.core,
            native_debug,
            tests,
            result_fingerprint,
        });
    }

    let test_import_roots = test_modules
        .iter()
        .map(|module| &module.core)
        .collect::<Vec<_>>();
    let project_core_modules = match project_context.as_ref() {
        Some(context) if exhaustive_project => {
            match compile_project_source_core_modules(context, &state, files) {
                Ok(modules) => modules,
                Err(exit_code) => return exit_code,
            }
        }
        Some(context) => {
            match compile_project_source_core_closure(context, &state, files, &test_import_roots) {
                Ok(modules) => modules,
                Err(exit_code) => return exit_code,
            }
        }
        None => Vec::new(),
    };
    let std_import_roots = test_modules
        .iter()
        .map(|module| &module.core)
        .chain(project_core_modules.iter().map(|module| &module.core))
        .collect::<Vec<_>>();
    let imported_std_core_modules =
        match compile_imported_std_source_core_modules(&std_import_roots, files, &state) {
            Ok(modules) => modules,
            Err(exit_code) => return exit_code,
        };
    let test_roots = test_modules
        .iter()
        .flat_map(|module| {
            module
                .tests
                .iter()
                .map(|test| (module.module_name.clone(), test.name.clone(), 0))
        })
        .collect::<Vec<_>>();
    let package = test_modules[0]
        .module_name
        .split('.')
        .next()
        .unwrap_or("terlan");
    let module_stem = format!("{package}_test_suite");
    let mut application_cores = Vec::with_capacity(
        test_modules.len() + project_core_modules.len() + imported_std_core_modules.len(),
    );
    let mut debug_inputs = Vec::with_capacity(application_cores.capacity());
    let mut runnable_modules = Vec::with_capacity(test_modules.len());
    for module in test_modules {
        application_cores.push(module.core);
        debug_inputs.push(module.native_debug);
        runnable_modules.push(RunnableTestModule {
            module_name: module.module_name,
            tests: module.tests,
            result_fingerprint: module.result_fingerprint,
        });
    }
    for module in project_core_modules
        .into_iter()
        .chain(imported_std_core_modules)
    {
        application_cores.push(module.core);
        debug_inputs.push(module.native_debug);
    }
    let native_helper_environment =
        match reachable_native_helper_environment(project_context.as_ref(), &application_cores) {
            Ok(environment) => environment,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
    let test_aot_workspace = state.out_dir.join("test-aot").join(&module_stem);
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let native_image = match compile_test_native_image_owned(
        &test_aot_workspace,
        &native_cache_root,
        &module_stem,
        application_cores,
        &test_roots,
        &debug_inputs,
        true,
    ) {
        Ok(image) => image,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    let output_style = TestOutputStyle::from_diagnostic_format(state.diagnostic_format);
    let mut failed = false;
    let mut covered_callables = BTreeSet::new();
    for module in &runnable_modules {
        let report = match run_discovered_terlan_vm_tests(
            &module.module_name,
            &module.tests,
            native_image.as_deref(),
            &native_helper_environment,
            args.benchmark
                .then_some((args.benchmark_warmup, args.benchmark_samples)),
            false,
        ) {
            Ok(report) => report,
            Err(message) => {
                eprintln!("{message}");
                failed = true;
                continue;
            }
        };
        covered_callables.extend(&report.covered_callables);
        if args.benchmark {
            print_benchmark_report(&report, output_style);
        } else {
            print_runtime_test_report(&report, output_style);
        }
        if !report.is_success() {
            failed = true;
        } else if let Some(fingerprint) = module.result_fingerprint.as_deref() {
            if let Err(message) = store_cached_test_result(&cache_root, "terlan-vm", fingerprint) {
                eprintln!("{message}");
                failed = true;
            }
        }
    }

    if let Some(coverage) = coverage {
        let Some(native_image) = native_image.as_deref() else {
            eprintln!("error[test.coverage.image]: VM tests did not produce a native image");
            return ExitCode::from(1);
        };
        let standalone_root = [PathBuf::from(&args.path)];
        let coverage_source_roots = if args.coverage_sources.is_empty() {
            project_context
                .as_ref()
                .map_or(standalone_root.as_slice(), |context| {
                    context.coverage_source_roots.as_slice()
                })
        } else {
            args.coverage_sources.as_slice()
        };
        if let Err(message) =
            coverage.record_image(native_image, coverage_source_roots, &covered_callables)
        {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }

    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
