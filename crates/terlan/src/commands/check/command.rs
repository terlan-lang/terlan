use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::artifacts::{collect_syntax_dependency_hashes, fingerprint};
use crate::validation::phase_manifest::{
    create_phase, emit_or_log_phase_manifest_error, emit_phase_manifest,
    PhaseManifestCoreProofCoverage, PhaseManifestDiagnostic, PhaseManifestIdentity,
};
use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
use crate::{formal_pipeline::CheckedSyntaxModuleArtifacts, CliCommand, CliState};

use super::directory::run_check_dir;

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
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let (path, phase_manifest_path) = match parse_check_args(&cmd.args) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    if Path::new(&path).is_dir() {
        return run_check_dir(&path, state, phase_manifest_path.as_deref());
    }

    let source = match crate::support::read_file(&path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return emit_or_log_phase_manifest_error(
                phase_manifest_path.as_deref(),
                &path,
                0,
                &[create_phase(
                    "parse",
                    "error",
                    vec![PhaseManifestDiagnostic {
                        code: "IO_ERROR",
                        severity: "error",
                        message: message.to_string(),
                        path: path.clone(),
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
                phase_manifest_path.as_deref(),
                &path,
                source_hash,
                &[create_phase(
                    "target_inference",
                    "error",
                    vec![PhaseManifestDiagnostic {
                        code: "target_inference_error",
                        severity: "error",
                        message,
                        path: path.clone(),
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
            &path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            target_profile,
        );

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
    let core_output = if !compile.parse_diagnostics.is_empty()
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

    if let Some(manifest_path) = phase_manifest_path.as_deref() {
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
                    source_path: &path,
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
        } else {
            if let Err(err) = emit_phase_manifest(
                Path::new(manifest_path),
                PhaseManifestIdentity {
                    source_path: &path,
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

    let CheckedSyntaxModuleArtifacts { syntax_output, .. } = compile
        .artifacts
        .expect("compile module should produce artifacts on success");

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
fn effective_check_target_profile(
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
/// - Source path plus optional phase-manifest output path.
/// - `Err(String)` for missing path, duplicate phase-manifest flag, missing flag
///   value, or extra positional arguments.
///
/// Transformation:
/// - Scans positional source path and `--emit-phase-manifest <path>` while
///   rejecting unsupported argument shapes.
pub(crate) fn parse_check_args(args: &[String]) -> Result<(String, Option<PathBuf>), String> {
    let mut path = None;
    let mut emit_phase_manifest = None;
    let mut i = 0;

    while i < args.len() {
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

        if path.is_none() {
            path = Some(args[i].clone());
            i += 1;
            continue;
        }

        return Err(format!("unexpected positional argument: {}", args[i]));
    }

    let path = path.ok_or_else(|| "missing path argument".to_string())?;
    Ok((path, emit_phase_manifest))
}
