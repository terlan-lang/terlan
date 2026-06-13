use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use terlan_hir::{
    load_interfaces_from_dir, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface, ModuleInterface,
};
use terlan_typeck::{expand_syntax_derives, expand_syntax_raw_macros};

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, fingerprint, read_manifest, DependencyManifest,
};
use crate::validation::native_policy::validate_native_policy;
use crate::validation::phase_manifest::{
    create_phase, current_syntax_contract_identity, emit_or_log_phase_manifest_error,
    emit_phase_manifest, PhaseManifestCoreProofCoverage, PhaseManifestDiagnostic,
};
use crate::validation::{
    config_contract::check_config_declarations_syntax_output,
    template_contract::type_check_syntax_module_output_with_templates,
};
use crate::{formal_pipeline::CheckedSyntaxModuleArtifacts, CliCommand, CliState};

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
///   phase pipeline with optional phase-manifest and cache-interface emission.
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
    let source_hash = fingerprint(source.as_bytes());
    let compile =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            &path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
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
    let derive_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
    {
        create_phase("derive_expansion", "skipped", Vec::new())
    } else {
        create_phase(
            "derive_expansion",
            if compile.derive_expansion_diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            },
            compile.derive_expansion_diagnostics.clone(),
        )
    };
    let resolve_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
        || !compile.derive_expansion_diagnostics.is_empty()
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
        || !compile.derive_expansion_diagnostics.is_empty()
    {
        create_phase("typecheck", "skipped", Vec::new())
    } else if has_type_errors {
        create_phase("typecheck", "error", compile.typecheck_diagnostics.clone())
    } else {
        create_phase("typecheck", "ok", compile.typecheck_diagnostics.clone())
    };
    let core_output = if !compile.parse_diagnostics.is_empty()
        || !compile.macro_expansion_diagnostics.is_empty()
        || !compile.derive_expansion_diagnostics.is_empty()
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
                &path,
                Some(artifacts.syntax_output.module_name.as_str()),
                source_hash,
                interface_hash,
                interface_doc_hash,
                core_ir_hash,
                core_proof_coverage,
                &dependency_hashes,
                &[
                    parse_output.clone(),
                    macro_output.clone(),
                    derive_output.clone(),
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
                &path,
                None,
                source_hash,
                0,
                0,
                0,
                PhaseManifestCoreProofCoverage::default(),
                &[],
                &[
                    parse_output,
                    macro_output,
                    derive_output,
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

    let artifacts = compile
        .artifacts
        .expect("compile module should produce artifacts on success");
    if let Some(cache_dir) = state.cache_dir.as_deref() {
        if let Err(err) = write_single_file_interface_cache(
            cache_dir,
            &path,
            source_hash,
            &artifacts,
            state.incremental,
        ) {
            eprintln!("{}", err);
            return ExitCode::from(1);
        }
    }

    if state.trace_invalidation {
        println!("CHECK {}", artifacts.syntax_output.module_name);
        if let Some(cache_dir) = state.cache_dir.as_deref() {
            let interface_target =
                cache_dir.join(format!("{}.typi", artifacts.syntax_output.module_name));
            if interface_target.exists() {
                println!(
                    "INTERFACE_CACHE_HIT {}",
                    artifacts.syntax_output.module_name
                );
            } else {
                println!(
                    "INTERFACE_CACHE_MISS {}",
                    artifacts.syntax_output.module_name
                );
            }
        }
    }

    ExitCode::SUCCESS
}

/// Writes generated interface cache files for one successfully checked source.
///
/// Inputs:
/// - `cache_dir`: target directory for generated `.typi` and `.typi.deps`
///   files.
/// - `path`: source path used for dependency-manifest path-sensitive hashes.
/// - `source_hash`: stable hash of the checked source text.
/// - `artifacts`: successful formal pipeline output for the checked source.
/// - `incremental`: whether unchanged cache files may be left untouched.
///
/// Output:
/// - `Ok(())` when both cache files are written or already current.
/// - `Err(String)` when syntax-contract identity lookup, directory creation,
///   or cache-file writing fails.
///
/// Transformation:
/// - Projects the formal module interface into `.typi` text and writes a
///   matching dependency manifest so single-file checks can serve as the
///   canonical stdlib summary generator without weakening directory layout
///   validation.
fn write_single_file_interface_cache(
    cache_dir: &Path,
    path: &str,
    source_hash: u64,
    artifacts: &CheckedSyntaxModuleArtifacts,
    incremental: bool,
) -> Result<(), String> {
    fs::create_dir_all(cache_dir)
        .map_err(|err| format!("cannot create cache directory: {}", err))?;
    let syntax_contract_identity = current_syntax_contract_identity()?;
    let module_name = &artifacts.syntax_output.module_name;
    let interface = &artifacts.core.interface;
    let interface_text = interface.to_terlan_interface_text();
    let interface_target = cache_dir.join(format!("{module_name}.typi"));
    crate::support::write_if_changed_or_forced(
        &interface_target,
        interface_text.as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write interface output: {}", err))?;

    let dependency_hashes = collect_syntax_dependency_hashes(
        &artifacts.syntax_output,
        &artifacts.interfaces,
        Some(Path::new(path)),
        None,
    );
    let manifest = DependencyManifest {
        module: module_name.clone(),
        syntax_contract_identity,
        source_hash,
        interface_hash: fingerprint(interface.to_terlan_interface_type_text().as_bytes()),
        interface_doc_hash: fingerprint(interface.to_terlan_interface_doc_text().as_bytes()),
        dependencies: dependency_hashes,
    };
    let manifest_target = cache_dir.join(format!("{module_name}.typi.deps"));
    crate::support::write_if_changed_or_forced(
        &manifest_target,
        manifest.encode().as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write dependency manifest: {}", err))?;
    Ok(())
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

/// Executes directory checking with incremental interface cache support.
///
/// Inputs:
/// - `path`: directory path to scan for `.tl` source files.
/// - `state`: parsed global CLI state, including cache, incremental mode,
///   diagnostic format, native policy, and invalidation tracing.
/// - `phase_manifest_path`: optional manifest file or directory path.
///
/// Output:
/// - `ExitCode::SUCCESS` when all selected modules check successfully.
/// - `ExitCode::from(1)` for source discovery, parse, resolve, typecheck,
///   cache, manifest, native policy, or write failures.
///
/// Transformation:
/// - Discovers Terlan source files, builds interface cache entries, selects
///   modules requiring recheck, typechecks those modules, writes dependency
///   manifests, and emits optional phase manifests.
pub(crate) fn run_check_dir(
    path: &str,
    state: CliState,
    phase_manifest_path: Option<&Path>,
) -> ExitCode {
    let dir = Path::new(path);
    let cache_dir = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| dir.join(".terlan"));
    if let Err(err) = fs::create_dir_all(&cache_dir) {
        eprintln!("cannot create cache directory: {}", err);
        return ExitCode::from(1);
    }
    let syntax_contract_identity = match current_syntax_contract_identity() {
        Ok(identity) => identity,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let phase_manifest_root = match phase_manifest_path {
        Some(path) => {
            if path.extension().is_none() {
                if let Err(err) = fs::create_dir_all(path) {
                    eprintln!(
                        "cannot create phase manifest directory {}: {}",
                        path.display(),
                        err
                    );
                    return ExitCode::from(1);
                }
            }
            Some(path.to_owned())
        }
        None => None,
    };

    let files = match crate::formal_pipeline::terlan_sources_in_dir(dir) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };

    let mut parsed_modules = Vec::new();
    let mut new_interfaces: HashMap<String, String> = HashMap::new();
    let mut previous_interfaces: HashMap<String, String> = HashMap::new();
    let mut changed_sources = BTreeSet::new();
    let mut changed_interfaces = BTreeSet::new();

    for file in &files {
        let path_text = file.to_string_lossy().to_string();
        let source = match crate::support::read_file(&path_text) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if let Err(message) = validate_native_policy(&source, state.native_policy) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
        let syntax_output =
            match crate::formal_pipeline::parse_source_as_syntax_output(&path_text, &source) {
                Ok(output) => output,
                Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
                    crate::support::emit_diagnostic(
                        "parse_error",
                        &message,
                        &path_text,
                        span.start,
                        span.end,
                        state.diagnostic_format,
                    );
                    if let Some(manifest_root) = phase_manifest_root.as_deref() {
                        let module_name = file
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("unparsed");
                        let manifest_path =
                            check_dir_phase_manifest_path(manifest_root, module_name);
                        let parse_output = create_phase(
                            "parse",
                            "error",
                            vec![PhaseManifestDiagnostic {
                                code: "parse_error",
                                severity: "error",
                                message: message.clone(),
                                path: path_text.clone(),
                                span_start: span.start,
                                span_end: span.end,
                                ..Default::default()
                            }],
                        );
                        if let Err(manifest_err) = emit_phase_manifest(
                            &manifest_path,
                            &path_text,
                            None,
                            fingerprint(source.as_bytes()),
                            0,
                            0,
                            0,
                            PhaseManifestCoreProofCoverage::default(),
                            &[],
                            &[
                                parse_output,
                                create_phase("macro_expansion", "skipped", Vec::new()),
                                create_phase("derive_expansion", "skipped", Vec::new()),
                                create_phase("resolve", "skipped", Vec::new()),
                                create_phase("typecheck", "skipped", Vec::new()),
                                create_phase("core", "skipped", Vec::new()),
                            ],
                        ) {
                            eprintln!("failed to write phase manifest: {}", manifest_err);
                            return ExitCode::from(1);
                        }
                    }
                    return ExitCode::from(1);
                }
                Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
                    eprintln!("{}", message);
                    if let Some(manifest_root) = phase_manifest_root.as_deref() {
                        let module_name = file
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("unparsed");
                        let manifest_path =
                            check_dir_phase_manifest_path(manifest_root, module_name);
                        let parse_output = create_phase(
                            "parse",
                            "error",
                            vec![PhaseManifestDiagnostic {
                                code: "SYNTAX_OUTPUT_ERROR",
                                severity: "error",
                                message,
                                path: path_text.clone(),
                                span_start: 0,
                                span_end: 0,
                                ..Default::default()
                            }],
                        );
                        if let Err(manifest_err) = emit_phase_manifest(
                            &manifest_path,
                            &path_text,
                            None,
                            fingerprint(source.as_bytes()),
                            0,
                            0,
                            0,
                            PhaseManifestCoreProofCoverage::default(),
                            &[],
                            &[
                                parse_output,
                                create_phase("macro_expansion", "skipped", Vec::new()),
                                create_phase("derive_expansion", "skipped", Vec::new()),
                                create_phase("resolve", "skipped", Vec::new()),
                                create_phase("typecheck", "skipped", Vec::new()),
                                create_phase("core", "skipped", Vec::new()),
                            ],
                        ) {
                            eprintln!("failed to write phase manifest: {}", manifest_err);
                            return ExitCode::from(1);
                        }
                    }
                    return ExitCode::from(1);
                }
            };
        let (syntax_output, macro_expansion_diagnostics) = expand_syntax_raw_macros(syntax_output);
        if !macro_expansion_diagnostics.is_empty() {
            for diag in &macro_expansion_diagnostics {
                crate::support::emit_diagnostic(
                    "type_error",
                    &diag.message,
                    &path_text,
                    diag.span.start,
                    diag.span.end,
                    state.diagnostic_format,
                );
            }
            if let Some(manifest_root) = phase_manifest_root.as_deref() {
                let module_name = file
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unparsed");
                let manifest_path = check_dir_phase_manifest_path(manifest_root, module_name);
                let macro_output = create_phase(
                    "macro_expansion",
                    "error",
                    macro_expansion_diagnostics
                        .iter()
                        .map(|diag| PhaseManifestDiagnostic {
                            code: "macro_expansion_error",
                            severity: "error",
                            message: diag.message.clone(),
                            path: path_text.clone(),
                            span_start: diag.span.start,
                            span_end: diag.span.end,
                            ..Default::default()
                        })
                        .collect(),
                );
                if let Err(manifest_err) = emit_phase_manifest(
                    &manifest_path,
                    &path_text,
                    None,
                    fingerprint(source.as_bytes()),
                    0,
                    0,
                    0,
                    PhaseManifestCoreProofCoverage::default(),
                    &[],
                    &[
                        create_phase("parse", "ok", Vec::new()),
                        macro_output,
                        create_phase("derive_expansion", "skipped", Vec::new()),
                        create_phase("resolve", "skipped", Vec::new()),
                        create_phase("typecheck", "skipped", Vec::new()),
                        create_phase("core", "skipped", Vec::new()),
                    ],
                ) {
                    eprintln!("failed to write phase manifest: {}", manifest_err);
                    return ExitCode::from(1);
                }
            }
            return ExitCode::from(1);
        }

        let module_name = syntax_output.module_name.clone();
        if let Err(message) = validate_directory_module_layout(dir, file, &module_name) {
            crate::support::emit_diagnostic(
                "module_layout_error",
                &message,
                &path_text,
                0,
                0,
                state.diagnostic_format,
            );
            if let Some(manifest_root) = phase_manifest_root.as_deref() {
                let manifest_path = check_dir_phase_manifest_path(manifest_root, &module_name);
                let layout_output = create_phase(
                    "resolve",
                    "error",
                    vec![PhaseManifestDiagnostic {
                        code: "module_layout_error",
                        severity: "error",
                        message,
                        path: path_text.clone(),
                        span_start: 0,
                        span_end: 0,
                        ..Default::default()
                    }],
                );
                if let Err(manifest_err) = emit_phase_manifest(
                    &manifest_path,
                    &path_text,
                    Some(module_name.as_str()),
                    fingerprint(source.as_bytes()),
                    0,
                    0,
                    0,
                    PhaseManifestCoreProofCoverage::default(),
                    &[],
                    &[
                        create_phase("parse", "ok", Vec::new()),
                        create_phase("macro_expansion", "ok", Vec::new()),
                        create_phase("derive_expansion", "skipped", Vec::new()),
                        layout_output,
                        create_phase("typecheck", "skipped", Vec::new()),
                        create_phase("core", "skipped", Vec::new()),
                    ],
                ) {
                    eprintln!("failed to write phase manifest: {}", manifest_err);
                    return ExitCode::from(1);
                }
            }
            return ExitCode::from(1);
        }

        let interface = syntax_module_output_to_interface(&syntax_output);
        let interface_text = interface.to_terlan_interface_text();
        let interface_type_hash = fingerprint(interface.to_terlan_interface_type_text().as_bytes());
        let interface_target = cache_dir.join(format!("{}.typi", module_name));
        let previous = fs::read_to_string(&interface_target).unwrap_or_default();
        previous_interfaces.insert(module_name.clone(), previous);
        new_interfaces.insert(module_name.clone(), interface_text);

        let manifest_target = cache_dir.join(format!("{}.typi.deps", module_name));
        let previous_manifest = read_manifest(&manifest_target);
        if previous_manifest
            .as_ref()
            .is_none_or(|manifest| manifest.interface_hash != interface_type_hash)
        {
            changed_interfaces.insert(module_name.clone());
        }
        let source_hash = fingerprint(source.as_bytes());
        if previous_manifest.as_ref().is_none_or(|manifest| {
            manifest.source_hash != source_hash
                || manifest.syntax_contract_identity != syntax_contract_identity
        }) {
            changed_sources.insert(module_name.clone());
        }

        parsed_modules.push((file.clone(), source, syntax_output));
    }

    let mut modules_to_check = changed_sources.clone();
    if !state.incremental {
        modules_to_check.extend(
            parsed_modules
                .iter()
                .map(|(_, _, syntax_output)| syntax_output.module_name.clone()),
        );
    }
    if !changed_interfaces.is_empty() {
        for (_, _, syntax_output) in &parsed_modules {
            if crate::formal_pipeline::syntax_module_imports_changed_interface(
                syntax_output,
                &changed_interfaces,
            ) {
                modules_to_check.insert(syntax_output.module_name.clone());
            }
        }
    }

    for (module_name, interface_text) in &new_interfaces {
        let target = cache_dir.join(format!("{}.typi", module_name));
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            interface_text.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write interface output: {}", err);
            return ExitCode::from(1);
        }
    }

    let mut interfaces: HashMap<String, ModuleInterface> = HashMap::new();
    load_interfaces_from_dir(&cache_dir, &mut interfaces);
    crate::formal_pipeline::load_embedded_std_interfaces(&mut interfaces);

    let mut has_errors = false;
    for (file, source, syntax_output) in &parsed_modules {
        let module_name = &syntax_output.module_name;
        if !modules_to_check.contains(module_name) {
            continue;
        }
        if state.trace_invalidation {
            println!("RECHECK {}", module_name);
        }
        if changed_interfaces.contains(module_name)
            && previous_interfaces
                .get(module_name)
                .is_some_and(|previous| !previous.is_empty())
            && state.trace_invalidation
        {
            println!("INTERFACE_CHANGED {}", module_name);
        }

        let resolved =
            resolve_syntax_module_output_with_interfaces(syntax_output, &interfaces).module;
        let resolve_diagnostics = resolved
            .diagnostics
            .iter()
            .map(|diag| PhaseManifestDiagnostic {
                code: "resolve_error",
                severity: "error",
                message: diag.message.clone(),
                path: file.to_string_lossy().into_owned(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        has_errors = has_errors || !resolve_diagnostics.is_empty();
        let (syntax_output, derive_expansion_diagnostics) =
            expand_syntax_derives(syntax_output.clone(), &resolved);
        let derive_diagnostics = derive_expansion_diagnostics
            .iter()
            .map(|diag| PhaseManifestDiagnostic {
                code: "derive_expansion_error",
                severity: "error",
                message: diag.message.clone(),
                path: file.to_string_lossy().into_owned(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        for diag in &derive_expansion_diagnostics {
            crate::support::emit_diagnostic(
                "type_error",
                &diag.message,
                &file.to_string_lossy(),
                diag.span.start,
                diag.span.end,
                state.diagnostic_format,
            );
        }
        has_errors = has_errors || !derive_expansion_diagnostics.is_empty();

        let diagnostics = if derive_expansion_diagnostics.is_empty() {
            let mut diagnostics =
                type_check_syntax_module_output_with_templates(&syntax_output, &resolved, file);
            diagnostics.extend(check_config_declarations_syntax_output(&syntax_output));
            diagnostics
        } else {
            Vec::new()
        };
        for diag in &diagnostics {
            let is_warning = matches!(diag.severity, terlan_typeck::DiagSeverity::Warning);
            has_errors = has_errors || !is_warning;
            let kind = crate::support::diagnostic_kind_for_message(
                if is_warning { "warning" } else { "type_error" },
                &diag.message,
            );
            crate::support::emit_diagnostic(
                kind,
                &diag.message,
                &file.to_string_lossy(),
                diag.span.start,
                diag.span.end,
                state.diagnostic_format,
            );
        }
        let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        if let Some(manifest_root) = phase_manifest_root.as_deref() {
            let module_interface = &core.interface;
            let typecheck_diagnostics = diagnostics
                .iter()
                .map(|diag| PhaseManifestDiagnostic {
                    code: if matches!(diag.severity, terlan_typeck::DiagSeverity::Warning) {
                        "type_warning"
                    } else if crate::support::diagnostic_kind_for_message(
                        "type_error",
                        &diag.message,
                    ) == "module_import"
                    {
                        "module_import"
                    } else {
                        "type_error"
                    },
                    severity: if matches!(diag.severity, terlan_typeck::DiagSeverity::Warning) {
                        "warning"
                    } else {
                        "error"
                    },
                    message: diag.message.clone(),
                    path: file.to_string_lossy().into_owned(),
                    span_start: diag.span.start,
                    span_end: diag.span.end,
                    ..Default::default()
                })
                .collect::<Vec<_>>();
            let derive_output = create_phase(
                "derive_expansion",
                if derive_expansion_diagnostics.is_empty() {
                    "ok"
                } else {
                    "error"
                },
                derive_diagnostics,
            );
            let type_output = create_phase(
                "typecheck",
                diagnostics
                    .iter()
                    .any(|diag| !matches!(diag.severity, terlan_typeck::DiagSeverity::Warning))
                    .then_some("failed")
                    .unwrap_or(if diagnostics.is_empty() {
                        "ok"
                    } else {
                        "warning"
                    }),
                typecheck_diagnostics,
            );
            let resolve_output = create_phase(
                "resolve",
                if resolve_diagnostics.is_empty() {
                    "ok"
                } else {
                    "error"
                },
                resolve_diagnostics,
            );
            let macro_output = create_phase("macro_expansion", "ok", Vec::new());
            let type_errors = diagnostics
                .iter()
                .any(|diag| !matches!(diag.severity, terlan_typeck::DiagSeverity::Warning));
            let core_output = create_phase(
                "core",
                if type_errors { "skipped" } else { "ok" },
                Vec::new(),
            );
            let core_ir_hash = if type_errors {
                0
            } else {
                fingerprint(core.contract_text().as_bytes())
            };
            let core_proof_coverage = if type_errors {
                PhaseManifestCoreProofCoverage::default()
            } else {
                PhaseManifestCoreProofCoverage::from_core_metadata(&core.metadata)
            };
            let manifest_path = check_dir_phase_manifest_path(manifest_root, module_name);
            if let Err(err) = emit_phase_manifest(
                &manifest_path,
                &file.to_string_lossy(),
                Some(syntax_output.module_name.as_str()),
                fingerprint(source.as_bytes()),
                fingerprint(module_interface.to_terlan_interface_type_text().as_bytes()),
                fingerprint(module_interface.to_terlan_interface_doc_text().as_bytes()),
                core_ir_hash,
                core_proof_coverage,
                &collect_syntax_dependency_hashes(&syntax_output, &interfaces, Some(file), None),
                &[
                    create_phase("parse", "ok", Vec::new()),
                    macro_output,
                    derive_output,
                    resolve_output,
                    type_output,
                    core_output,
                ],
            ) {
                eprintln!("failed to write phase manifest: {}", err);
                return ExitCode::from(1);
            }
        }

        let dependency_hashes =
            collect_syntax_dependency_hashes(&syntax_output, &interfaces, Some(file), None);
        let manifest = DependencyManifest {
            module: module_name.clone(),
            syntax_contract_identity: syntax_contract_identity.clone(),
            source_hash: fingerprint(source.as_bytes()),
            interface_hash: fingerprint(core.interface.to_terlan_interface_type_text().as_bytes()),
            interface_doc_hash: fingerprint(
                core.interface.to_terlan_interface_doc_text().as_bytes(),
            ),
            dependencies: dependency_hashes,
        };
        let target = cache_dir.join(format!("{}.typi.deps", module_name));
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            manifest.encode().as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write dependency manifest: {}", err);
            return ExitCode::from(1);
        }
    }

    if has_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Validates that a directory-mode source path matches its declared module.
///
/// Inputs:
/// - `root`: source root passed to `terlc check <dir>`.
/// - `file`: discovered `.tl` source file under `root`.
/// - `module_name`: parsed Terlan module declaration.
///
/// Output:
/// - `Ok(())` when the path stem maps exactly to the declared module.
/// - `Err(message)` when the file is outside the root, contains non-UTF-8 path
///   segments, has no `.tl` stem, or declares a different module.
///
/// Transformation:
/// - Strips the source root and `.tl` extension, converts path separators into
///   dots, and compares that expected module identity with the parsed module
///   declaration.
fn validate_directory_module_layout(
    root: &Path,
    file: &Path,
    module_name: &str,
) -> Result<(), String> {
    let expected = expected_module_name_for_source_path(root, file)?;
    if expected == module_name {
        return Ok(());
    }
    Err(format!(
        "module declaration `{module_name}` does not match source path `{}`; expected `module {expected}.`",
        file.display()
    ))
}

/// Computes the module name implied by a source-root-relative file path.
///
/// Inputs:
/// - `root`: source root used for directory compilation.
/// - `file`: implementation source path.
///
/// Output:
/// - Dotted module name implied by the relative `.tl` path.
/// - `Err(message)` when the path cannot be represented as canonical source
///   layout input.
///
/// Transformation:
/// - Removes the source root prefix, drops the `.tl` extension from the final
///   path segment, validates UTF-8 path segments, and joins all relative
///   segments with dots.
fn expected_module_name_for_source_path(root: &Path, file: &Path) -> Result<String, String> {
    let relative = file.strip_prefix(root).map_err(|_| {
        format!(
            "source file `{}` is not under source root `{}`",
            file.display(),
            root.display()
        )
    })?;
    let mut segments = Vec::new();
    for component in relative.components() {
        let value = component.as_os_str().to_str().ok_or_else(|| {
            format!(
                "source path `{}` contains a non-UTF-8 module segment",
                file.display()
            )
        })?;
        segments.push(value.to_string());
    }
    let last = segments
        .last_mut()
        .ok_or_else(|| format!("source path `{}` has no module file name", file.display()))?;
    if !last.ends_with(".tl") {
        return Err(format!(
            "source path `{}` is not a Terlan implementation source",
            file.display()
        ));
    }
    last.truncate(last.len() - ".tl".len());
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(format!(
            "source path `{}` contains an empty module segment",
            file.display()
        ));
    }
    Ok(segments.join("."))
}

/// Computes the per-module phase-manifest path for a directory check.
///
/// Inputs:
/// - `root`: requested phase-manifest path, either a directory-like path or a
///   file path.
/// - `module`: module name used in the generated file name.
///
/// Output:
/// - Concrete path for this module's manifest.
///
/// Transformation:
/// - Treats extensionless roots as directories and roots with extensions as
///   filename stems.
fn check_dir_phase_manifest_path(root: &Path, module: &str) -> PathBuf {
    if root.extension().is_none() {
        root.join(format!("{module}.phase-manifest.json"))
    } else {
        let stem = root
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("check");
        root.with_file_name(format!("{stem}.{module}.phase-manifest.json"))
    }
}
