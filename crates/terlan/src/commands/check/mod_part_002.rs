
/// Executes directory checking with incremental interface cache support.
///
/// Inputs:
/// - `path`: directory path to scan for `.terl` source files.
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
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
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
                                create_phase("include_expansion", "skipped", Vec::new()),
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
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
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
                                create_phase("include_expansion", "skipped", Vec::new()),
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
                        create_phase("include_expansion", "skipped", Vec::new()),
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
        if let Err(message) = validate_module_layout(dir, file, &module_name) {
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
                        create_phase("include_expansion", "skipped", Vec::new()),
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

    let target_profile_input = TargetInferenceInput::from_syntax_modules(
        parsed_modules
            .iter()
            .map(|(_, _, syntax_output)| syntax_output),
    );
    let target_profile =
        match effective_check_target_profile_from_input(target_profile_input, state.target_profile)
        {
            Ok(target_profile) => target_profile,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };

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

        let (mut syntax_output, imported_macro_diagnostics) =
            expand_syntax_macros_with_interfaces(syntax_output.clone(), &interfaces);
        if !imported_macro_diagnostics.is_empty() {
            for diag in &imported_macro_diagnostics {
                crate::support::emit_diagnostic(
                    "type_error",
                    &diag.message,
                    &file.to_string_lossy(),
                    diag.span.start,
                    diag.span.end,
                    state.diagnostic_format,
                );
            }
            has_errors = true;
            continue;
        }
        if let Err(error) = expand_syntax_shape_imports(&mut syntax_output, &interfaces) {
            let (message, span_start, span_end) = match error {
                crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span) => {
                    (message, span.start, span.end)
                }
                crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message) => (message, 0, 0),
            };
            crate::support::emit_diagnostic(
                "type_error",
                &message,
                &file.to_string_lossy(),
                span_start,
                span_end,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }

        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
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
        let (syntax_output, include_expansion_diagnostics) =
            expand_syntax_includes(syntax_output, &resolved);
        let include_diagnostics = include_expansion_diagnostics
            .iter()
            .map(|diag| PhaseManifestDiagnostic {
                code: "include_expansion_error",
                severity: "error",
                message: diag.message.clone(),
                path: file.to_string_lossy().into_owned(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        for diag in &include_expansion_diagnostics {
            crate::support::emit_diagnostic(
                "type_error",
                &diag.message,
                &file.to_string_lossy(),
                diag.span.start,
                diag.span.end,
                state.diagnostic_format,
            );
        }
        has_errors = has_errors || !include_expansion_diagnostics.is_empty();

        let diagnostics = if include_expansion_diagnostics.is_empty() {
            let mut diagnostics =
                type_check_syntax_module_output_with_templates(&syntax_output, &resolved, file);
            diagnostics.extend(check_config_declarations_syntax_output(&syntax_output));
            diagnostics
        } else {
            Vec::new()
        };
        for diag in &diagnostics {
            let is_warning = matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning);
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
        let mut core =
            crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        core.source.source_path = Some(file.to_string_lossy().into_owned());
        let has_blocking_errors = !resolve_diagnostics.is_empty()
            || !include_expansion_diagnostics.is_empty()
            || diagnostics
                .iter()
                .any(|diag| !matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning));
        let target_profile_diagnostics = if has_blocking_errors {
            Vec::new()
        } else {
            target_profile_checks_with_options(
                &core,
                target_profile,
                TargetProfileCheckOptions::default(),
            )
            .into_iter()
            .map(|violation| PhaseManifestDiagnostic {
                code: violation.code,
                severity: "error",
                message: violation.message,
                path: file.to_string_lossy().into_owned(),
                span_start: 0,
                span_end: 0,
                ..Default::default()
            })
            .collect::<Vec<_>>()
        };
        for diag in &target_profile_diagnostics {
            crate::support::emit_diagnostic(
                diag.code,
                &diag.message,
                &file.to_string_lossy(),
                diag.span_start,
                diag.span_end,
                state.diagnostic_format,
            );
        }
        has_errors = has_errors || !target_profile_diagnostics.is_empty();
        if let Some(manifest_root) = phase_manifest_root.as_deref() {
            let module_interface = &core.interface;
            let typecheck_diagnostics = diagnostics
                .iter()
                .map(|diag| PhaseManifestDiagnostic {
                    code: if matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning) {
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
                    severity: if matches!(
                        diag.severity,
                        crate::terlan_typeck::DiagSeverity::Warning
                    ) {
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
            let include_output = create_phase(
                "include_expansion",
                if include_expansion_diagnostics.is_empty() {
                    "ok"
                } else {
                    "error"
                },
                include_diagnostics,
            );
            let type_output = create_phase(
                "typecheck",
                diagnostics
                    .iter()
                    .any(|diag| {
                        !matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning)
                    })
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
                .any(|diag| !matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning));
            let core_output = create_phase(
                "core",
                if type_errors {
                    "skipped"
                } else if target_profile_diagnostics.is_empty() {
                    "ok"
                } else {
                    "error"
                },
                target_profile_diagnostics.clone(),
            );
            let core_ir_hash = if type_errors || !target_profile_diagnostics.is_empty() {
                0
            } else {
                fingerprint(core.contract_text().as_bytes())
            };
            let core_proof_coverage = if type_errors || !target_profile_diagnostics.is_empty() {
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
                    include_output,
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
