use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use crate::commands::artifacts::{collect_syntax_dependency_hashes, fingerprint};
use crate::validation::phase_manifest::{
    create_phase, emit_or_log_phase_manifest_error, emit_phase_manifest,
    PhaseManifestCoreProofCoverage, PhaseManifestDiagnostic, PhaseManifestIdentity,
};
use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
use crate::{CliCommand, CliState};

/// Executes the `check` CLI command for a source path or directory.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing command-local arguments.
/// - `state`: parsed global CLI state, including diagnostics, cache, native
///   policy, and invalidation tracing.
///
/// Output:
/// - `ExitCode::SUCCESS` when checking succeeds.
/// - `ExitCode::from(2)` when command-local arguments are malformed.
/// - `ExitCode::from(1)` or a propagated compile-phase exit code on failures.
///
/// Transformation:
/// - Parses check-local arguments, delegates directory checks to the existing
///   directory checker, and runs single-file sources through the formal compile
///   phase pipeline with optional phase-manifest emission.
pub(crate) fn run(cmd: CliCommand, mut state: CliState) -> ExitCode {
    let (paths, phase_manifest_path, interfaces_only) = match parse_check_args(&cmd.args) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };
    if phase_manifest_path.is_none() {
        state.incremental = true;
    }

    if paths.len() > 1
        && phase_manifest_path
            .as_deref()
            .is_some_and(|path| path.extension().is_some())
    {
        eprintln!("multiple check paths require a phase-manifest directory");
        return ExitCode::from(2);
    }
    if paths.len() > 1 {
        if let Some(directory) = phase_manifest_path.as_deref() {
            if let Err(error) = std::fs::create_dir_all(directory) {
                eprintln!("cannot create phase-manifest directory: {error}");
                return ExitCode::from(1);
            }
        }
    }

    let batch = paths.len() > 1;
    if batch && !interfaces_only && paths.iter().all(|path| !Path::new(path).is_dir()) {
        return run_file_batch(&paths, state, phase_manifest_path.as_deref());
    }
    for path in paths {
        let started = Instant::now();
        if batch {
            eprintln!("terlc check: started {path}");
        }
        let result = run_path(
            &path,
            state.clone(),
            phase_manifest_path.as_deref(),
            interfaces_only,
            batch,
        );
        if batch {
            eprintln!(
                "terlc check: finished {path} in {:.2}s",
                started.elapsed().as_secs_f64()
            );
        }
        if result != ExitCode::SUCCESS {
            return result;
        }
    }
    ExitCode::SUCCESS
}

/// Groups changed files by their compiler-owned project context.
fn run_file_batch(
    paths: &[String],
    state: CliState,
    phase_manifest_path: Option<&Path>,
) -> ExitCode {
    let mut project_groups: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut standalone = Vec::new();
    for path in paths {
        let source_path = Path::new(path);
        let Some(project) = crate::commands::build::owning_project_dir_for_source(source_path)
        else {
            standalone.push(path.clone());
            continue;
        };
        if let Some((_, selected)) = project_groups
            .iter_mut()
            .find(|(candidate, _)| candidate == &project)
        {
            selected.push(path.clone());
        } else {
            project_groups.push((project, vec![path.clone()]));
        }
    }

    for (project, selected) in project_groups {
        let started = Instant::now();
        eprintln!(
            "terlc check: started {} changed file(s) in {}",
            selected.len(),
            project.display()
        );
        let result =
            super::project::run_selected(&project, &selected, state.clone(), phase_manifest_path);
        eprintln!(
            "terlc check: finished {} changed file(s) in {} in {:.2}s",
            selected.len(),
            project.display(),
            started.elapsed().as_secs_f64()
        );
        if result != ExitCode::SUCCESS {
            return result;
        }
    }
    for path in standalone {
        let started = Instant::now();
        eprintln!("terlc check: started {path}");
        let result = run_path(&path, state.clone(), phase_manifest_path, false, true);
        eprintln!(
            "terlc check: finished {path} in {:.2}s",
            started.elapsed().as_secs_f64()
        );
        if result != ExitCode::SUCCESS {
            return result;
        }
    }
    ExitCode::SUCCESS
}

/// Checks one member of an ordered `terlc check` batch.
///
/// Earlier paths may publish interfaces into the shared cache for later paths.
/// This lets one compiler process validate a project source root once and then
/// validate all changed `.terls` entries without repeating native AOT builds.
fn run_path(
    path: &str,
    state: CliState,
    phase_manifest_path: Option<&Path>,
    interfaces_only: bool,
    batch: bool,
) -> ExitCode {
    if Path::new(path).is_dir() {
        return run_directory_path(path, state, phase_manifest_path, interfaces_only);
    }
    if interfaces_only {
        eprintln!("--interfaces-only requires a source directory");
        return ExitCode::from(2);
    }

    let batch_manifest_path = if batch {
        let module = Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("source");
        phase_manifest_path
            .map(|root| super::directory::check_dir_phase_manifest_path(root, module))
    } else {
        None
    };
    let phase_manifest_path = batch_manifest_path.as_deref().or(phase_manifest_path);

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return emit_or_log_phase_manifest_error(
                phase_manifest_path,
                path,
                0,
                &[create_phase(
                    "parse",
                    "error",
                    vec![PhaseManifestDiagnostic {
                        code: "IO_ERROR",
                        severity: "error",
                        message: message.to_string(),
                        path: path.to_string(),
                        span_start: 0,
                        span_end: 0,
                        ..Default::default()
                    }],
                )],
                &[],
                ExitCode::from(1),
            );
        }
    };
    let source_hash = fingerprint(source.as_bytes());
    let target_profile = match effective_check_target_profile(&source, state.target_profile) {
        Ok(target_profile) => target_profile,
        Err(message) => {
            eprintln!("{message}");
            return emit_or_log_phase_manifest_error(
                phase_manifest_path,
                path,
                source_hash,
                &[create_phase(
                    "target_inference",
                    "error",
                    vec![PhaseManifestDiagnostic {
                        code: "target_inference_error",
                        severity: "error",
                        message,
                        path: path.to_string(),
                        span_start: 0,
                        span_end: 0,
                        ..Default::default()
                    }],
                )],
                &[],
                ExitCode::from(1),
            );
        }
    };
    let compile =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            target_profile,
        );

    finish_check(path, source_hash, &state, phase_manifest_path, compile)
}

/// Checks either a plain source directory or every configured project root.
///
/// Project manifests are compiler input. Keeping source-root interpretation in
/// `terlc` prevents hooks and other callers from duplicating manifest policy.
fn run_directory_path(
    path: &str,
    state: CliState,
    phase_manifest_path: Option<&Path>,
    interfaces_only: bool,
) -> ExitCode {
    let root = Path::new(path);
    let manifest_path = root.join("terlan.toml");
    if !manifest_path.is_file() {
        return super::project::run(path, state, phase_manifest_path, interfaces_only);
    }
    let manifest =
        match crate::commands::build::project_manifest::read_project_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
    for source_root in manifest.source_roots {
        let source_root = root.join(source_root);
        if !source_root.is_dir() {
            eprintln!(
                "project source root does not exist: {}",
                source_root.display()
            );
            return ExitCode::from(1);
        }
        let result = super::project::run(
            &source_root.to_string_lossy(),
            state.clone(),
            phase_manifest_path,
            interfaces_only,
        );
        if result != ExitCode::SUCCESS {
            return result;
        }
    }
    ExitCode::SUCCESS
}

/// Presents a shared pipeline result without treating interface declarations as
/// executable CoreIR. Directory contract checking uses the same diagnostics.
pub(super) fn finish_check(
    path: &str,
    source_hash: u64,
    state: &CliState,
    phase_manifest_path: Option<&Path>,
    compile: crate::formal_pipeline::CompileSyntaxModuleThroughPhasesResult,
) -> ExitCode {
    let parse_output = if compile.parse_diagnostics.is_empty() {
        create_phase("parse", "ok", Vec::new())
    } else {
        create_phase("parse", "error", compile.parse_diagnostics.clone())
    };
    let macro_output = if !compile.parse_diagnostics.is_empty() {
        create_phase("macro_expansion", "skipped", Vec::new())
    } else {
        create_phase(
            "macro_expansion",
            if compile.macro_expansion_diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            },
            compile.macro_expansion_diagnostics.clone(),
        )
    };
    let include_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
    {
        create_phase("include_expansion", "skipped", Vec::new())
    } else {
        create_phase(
            "include_expansion",
            if compile.include_expansion_diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            },
            compile.include_expansion_diagnostics.clone(),
        )
    };
    let resolve_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
        || !compile.include_expansion_diagnostics.is_empty()
    {
        create_phase("resolve", "skipped", Vec::new())
    } else {
        create_phase(
            "resolve",
            if compile.resolve_diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            },
            compile.resolve_diagnostics.clone(),
        )
    };
    let has_type_errors = compile
        .typecheck_diagnostics
        .iter()
        .any(|diag| diag.severity != "warning");
    let type_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
        || !compile.include_expansion_diagnostics.is_empty()
    {
        create_phase("typecheck", "skipped", Vec::new())
    } else if has_type_errors {
        create_phase("typecheck", "error", compile.typecheck_diagnostics.clone())
    } else {
        create_phase("typecheck", "ok", compile.typecheck_diagnostics.clone())
    };
    let core_output = if compile.checked_interface.is_some()
        || !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
        || !compile.include_expansion_diagnostics.is_empty()
        || !compile.resolve_diagnostics.is_empty()
        || has_type_errors
    {
        create_phase("core", "skipped", Vec::new())
    } else {
        create_phase(
            "core",
            if compile.core_diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            },
            compile.core_diagnostics.clone(),
        )
    };

    if let Some(manifest_path) = phase_manifest_path {
        if let Some(artifacts) = &compile.artifacts {
            let interface = &artifacts.core.interface;
            let dependency_hashes = collect_syntax_dependency_hashes(
                &artifacts.syntax_output,
                &artifacts.interfaces,
                Some(Path::new(&path)),
                None,
            );
            let interface_hash = fingerprint(interface.to_terlan_interface_type_text().as_bytes());
            let interface_doc_hash =
                fingerprint(interface.to_terlan_interface_doc_text().as_bytes());
            let core_ir_hash = fingerprint(artifacts.core.contract_text().as_bytes());
            let core_proof_coverage =
                PhaseManifestCoreProofCoverage::from_core_metadata(&artifacts.core.metadata);
            if let Err(err) = emit_phase_manifest(
                Path::new(manifest_path),
                PhaseManifestIdentity {
                    source_path: path,
                    module_name: Some(artifacts.syntax_output.module_name.as_str()),
                    source_hash,
                    interface_hash,
                    interface_doc_hash,
                    core_ir_hash,
                },
                core_proof_coverage,
                &dependency_hashes,
                &[
                    parse_output.clone(),
                    macro_output.clone(),
                    include_output.clone(),
                    resolve_output.clone(),
                    type_output.clone(),
                    core_output.clone(),
                ],
            ) {
                eprintln!("failed to write phase manifest: {}", err);
                return ExitCode::from(1);
            }
        } else if let Some(checked) = &compile.checked_interface {
            let dependencies = collect_syntax_dependency_hashes(
                &checked.syntax_output,
                &checked.interfaces,
                Some(Path::new(path)),
                None,
            );
            if let Err(err) = emit_phase_manifest(
                manifest_path,
                PhaseManifestIdentity {
                    source_path: path,
                    module_name: Some(&checked.syntax_output.module_name),
                    source_hash,
                    interface_hash: fingerprint(
                        checked.interface.to_terlan_interface_type_text().as_bytes(),
                    ),
                    interface_doc_hash: fingerprint(
                        checked.interface.to_terlan_interface_doc_text().as_bytes(),
                    ),
                    core_ir_hash: 0,
                },
                PhaseManifestCoreProofCoverage::default(),
                &dependencies,
                &[
                    parse_output,
                    macro_output,
                    include_output,
                    resolve_output,
                    type_output,
                    core_output,
                ],
            ) {
                eprintln!("failed to write phase manifest: {err}");
                return ExitCode::from(1);
            }
        } else {
            if let Err(err) = emit_phase_manifest(
                Path::new(manifest_path),
                PhaseManifestIdentity {
                    source_path: path,
                    module_name: None,
                    source_hash,
                    interface_hash: 0,
                    interface_doc_hash: 0,
                    core_ir_hash: 0,
                },
                PhaseManifestCoreProofCoverage::default(),
                &[],
                &[
                    parse_output,
                    macro_output,
                    include_output,
                    resolve_output,
                    type_output,
                    core_output,
                ],
            ) {
                eprintln!("failed to write phase manifest: {}", err);
                return ExitCode::from(1);
            }
            return ExitCode::from(1);
        }
    }

    if compile.exit_code != ExitCode::SUCCESS {
        return compile.exit_code;
    }

    let syntax_output = if let Some(artifacts) = compile.artifacts {
        artifacts.syntax_output
    } else if let Some(checked) = compile.checked_interface {
        checked.syntax_output
    } else {
        return ExitCode::from(1);
    };

    if state.trace_invalidation {
        println!("CHECK {}", syntax_output.module_name);
        if let Some(cache_dir) = state.cache_dir.as_deref() {
            let interface_target = cache_dir.join(format!("{}.typi", syntax_output.module_name));
            if interface_target.exists() {
                println!("INTERFACE_CACHE_HIT {}", syntax_output.module_name);
            } else {
                println!("INTERFACE_CACHE_MISS {}", syntax_output.module_name);
            }
        }
    }

    ExitCode::SUCCESS
}

/// Resolves the effective target profile for single-file `terlc check`.
///
/// Inputs:
/// - `source`: Terlan source text.
/// - `requested`: global CLI target profile.
///
/// Output:
/// - Effective target profile used for formal validation.
/// - Target-evidence diagnostic when an explicit non-VM override conflicts.
///
/// Transformation:
/// - Parses source imports into shared target evidence, infers the narrowest
///   profile when the global target is the default VM, and treats non-default
///   profiles as checked overrides. Parse failures keep the requested profile
///   so normal parser diagnostics remain the source-facing error.
pub(crate) fn effective_check_target_profile(
    source: &str,
    requested: TargetProfile,
) -> Result<TargetProfile, String> {
    let Ok(syntax) = crate::terlan_syntax::parse_module_as_syntax_output(source) else {
        return Ok(requested);
    };

    let input = TargetInferenceInput::from_syntax_modules([&syntax]);
    effective_check_target_profile_from_input(input, requested)
}

/// Resolves a `check` target profile from already-collected target evidence.
///
/// Inputs:
/// - `input`: target evidence collected from one or more parsed syntax modules.
/// - `requested`: global CLI target profile.
///
/// Output:
/// - Effective target profile used for validation.
/// - Target-evidence diagnostic when an explicit non-VM override conflicts.
///
/// Transformation:
/// - Applies the command-level `check` override policy consistently for
///   single-file and directory checks.
pub(super) fn effective_check_target_profile_from_input(
    input: TargetInferenceInput,
    requested: TargetProfile,
) -> Result<TargetProfile, String> {
    let inference = infer_target_profile_from_typed_evidence(&input)
        .map_err(|conflict| format!("terlc check target inference error: {}", conflict.message))?;

    if requested == TargetProfile::Vm {
        return Ok(inference.profile);
    }

    if let Some(message) = explicit_target_profile_override_error(&inference, requested) {
        return Err(format!("terlc check target inference error: {message}"));
    }

    Ok(requested)
}

/// Parses command-local flags for `check`.
///
/// Inputs:
/// - `args`: command-local arguments after the `check` verb.
///
/// Output:
/// - One or more ordered source paths plus optional phase-manifest output path.
/// - `Err(String)` for missing path, duplicate phase-manifest flag, missing flag
///   value, or extra positional arguments.
///
/// Transformation:
/// - Collects positional source paths and `--emit-phase-manifest <path>` while
///   rejecting unsupported argument shapes.
pub(crate) fn parse_check_args(
    args: &[String],
) -> Result<(Vec<String>, Option<PathBuf>, bool), String> {
    let mut paths = Vec::new();
    let mut emit_phase_manifest = None;
    let mut interfaces_only = false;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--interfaces-only" {
            if interfaces_only {
                return Err("duplicate --interfaces-only".to_string());
            }
            interfaces_only = true;
            i += 1;
            continue;
        }
        if args[i].as_str() == "--emit-phase-manifest" {
            if i + 1 >= args.len() {
                return Err("--emit-phase-manifest requires a path".to_string());
            }
            if emit_phase_manifest.is_some() {
                return Err("duplicate --emit-phase-manifest".to_string());
            }
            emit_phase_manifest = Some(PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }

        paths.push(args[i].clone());
        i += 1;
    }

    if paths.is_empty() {
        return Err("missing path argument".to_string());
    }
    Ok((paths, emit_phase_manifest, interfaces_only))
}
