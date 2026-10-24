use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_hir::{
    expand_syntax_shape_imports, resolve_syntax_module_output_with_interfaces, ModuleInterface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
    parse_script_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprOutput, SyntaxModuleOutput,
};
use crate::terlan_typeck::{expand_syntax_includes, expand_syntax_macros_with_interfaces};

use crate::validation::config_contract::check_config_declarations_syntax_output;
use crate::validation::native_policy::{validate_native_policy, NativePolicy};
use crate::validation::phase_manifest::PhaseManifestDiagnostic;
use crate::validation::target_profile::{
    target_profile_checks_with_options, TargetProfile, TargetProfileCheckOptions,
};
use crate::validation::template_contract::type_check_syntax_module_output_with_template_inputs;

#[path = "formal_pipeline/source_discovery.rs"]
mod source_discovery;
pub(crate) use source_discovery::terlan_check_sources_in_dir;
#[path = "formal_pipeline/interface_contract.rs"]
pub(crate) mod interface_contract;
#[path = "formal_pipeline/interface_dependencies.rs"]
pub(crate) mod interface_dependencies;
#[path = "formal_pipeline/interface_graph.rs"]
pub(crate) mod interface_graph;
#[path = "formal_pipeline/interface_loading.rs"]
mod interface_loading;
#[path = "formal_pipeline/phase_timings.rs"]
mod phase_timings;
#[path = "formal_pipeline/templates.rs"]
mod templates;
use crate::DiagnosticFormat;
#[cfg(test)]
pub(crate) use interface_loading::load_external_interfaces;
pub(crate) use interface_loading::source_dependency_modules;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use interface_loading::{
    load_embedded_std_interfaces, load_external_interfaces_for_module,
    load_required_standard_interfaces,
};

/// Checked artifacts produced by the formal compile pipeline.
///
/// Inputs:
/// - Produced from one source file by
///   `compile_syntax_module_through_phases_with_diagnostics_for_profile`.
///
/// Output:
/// - Formal syntax output, loaded dependency interfaces, and CoreIR.
///
/// Transformation:
/// - Carries the parse, interface loading, and lowered CoreIR artifacts that
///   downstream commands need for backend-agnostic emission and validation.
pub(crate) struct CheckedSyntaxModuleArtifacts {
    pub(crate) syntax_output: SyntaxModuleOutput,
    pub(crate) interfaces: HashMap<String, ModuleInterface>,
    pub(crate) core: crate::terlan_typeck::CoreModule,
}

/// Full formal compile result including phase diagnostics.
///
/// Inputs:
/// - Produced by `compile_syntax_module_through_phases_with_diagnostics_for_profile`.
///
/// Output:
/// - Optional checked artifacts, phase diagnostics, and the command exit code.
///
/// Transformation:
/// - Preserves parse, resolve, and typecheck status so commands can emit phase
///   manifests without rerunning compilation.
pub(crate) struct CompileSyntaxModuleThroughPhasesResult {
    pub(crate) artifacts: Option<CheckedSyntaxModuleArtifacts>,
    pub(crate) checked_interface: Option<CheckedInterfaceArtifacts>,
    pub(crate) parse_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) macro_expansion_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) include_expansion_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) resolve_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) typecheck_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) core_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) exit_code: ExitCode,
}

/// Checked declarations without executable CoreIR or implementation evidence.
pub(crate) struct CheckedInterfaceArtifacts {
    pub(crate) syntax_output: SyntaxModuleOutput,
    pub(crate) interfaces: HashMap<String, ModuleInterface>,
    pub(crate) interface: ModuleInterface,
}

mod embedded_interfaces;
use embedded_interfaces::EMBEDDED_STD_INTERFACES;

/// Lists Terlan implementation sources under a directory.
///
/// Inputs:
/// - `dir`: source root directory to scan.
///
/// Output:
/// - Sorted recursive `.terl` source paths, or a user-facing directory read
///   error.
///
/// Transformation:
/// - Recursively walks deterministic directory entries and keeps files with the
///   `.terl` extension so directory-mode compiler commands can consume
///   package-rooted source layouts. Nested child directories containing
///   `terlan.toml` are treated as project boundaries and are not scanned from
///   the parent source root.
pub(crate) fn terlan_sources_in_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    source_discovery::collect_terlan_sources_recursive(dir, &mut files, false)
        .map_err(|error| error.to_string())?;
    files.sort();
    Ok(files)
}

/// Returns whether a formal syntax-output module references changed interfaces.
///
/// Inputs:
/// - `module`: syntax-output module being checked.
/// - `changed_interfaces`: module names whose interface hashes changed.
///
/// Output:
/// - `true` when an import, remote call, or nested expression references one
///   of the changed interfaces.
///
/// Transformation:
/// - Walks syntax-output declarations and recursively scans expression trees.
pub(crate) fn syntax_module_imports_changed_interface(
    module: &SyntaxModuleOutput,
    changed_interfaces: &BTreeSet<String>,
) -> bool {
    module
        .declarations
        .iter()
        .any(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import { module_name, .. } => {
                changed_interfaces.contains(module_name)
            }
            SyntaxDeclarationPayload::Function { clauses, .. } => clauses.iter().any(|clause| {
                syntax_expr_uses_remote_module(&clause.body, changed_interfaces)
                    || clause.guard.as_ref().is_some_and(|guard| {
                        syntax_expr_uses_remote_module(guard, changed_interfaces)
                    })
            }),
            SyntaxDeclarationPayload::Constructor { clauses, .. } => clauses
                .iter()
                .any(|clause| syntax_expr_uses_remote_module(&clause.body, changed_interfaces)),
            _ => false,
        })
}

/// Returns whether a syntax-output expression references changed modules.
///
/// Inputs:
/// - `expr`: syntax-output expression to scan.
/// - `modules`: changed module names.
///
/// Output:
/// - `true` when the expression or a nested child references one of the
///   supplied module names through a remote reference.
///
/// Transformation:
/// - Recursively scans children, fields, clauses, and guards.
fn syntax_expr_uses_remote_module(expr: &SyntaxExprOutput, modules: &BTreeSet<String>) -> bool {
    expr.remote
        .as_ref()
        .is_some_and(|module_name| modules.contains(module_name))
        || expr
            .children
            .iter()
            .any(|child| syntax_expr_uses_remote_module(child, modules))
        || expr
            .fields
            .iter()
            .any(|field| syntax_expr_uses_remote_module(&field.value, modules))
        || expr.clauses.iter().any(|clause| {
            syntax_expr_uses_remote_module(&clause.body, modules)
                || clause
                    .guard
                    .as_ref()
                    .is_some_and(|guard| syntax_expr_uses_remote_module(guard, modules))
        })
}

/// Parses source text into formal syntax output.
///
/// Inputs:
/// - `path`: source path used to distinguish `.terl` and `.terli` grammars.
/// - `source`: source text to parse.
///
/// Output:
/// - Syntax module output or an EBNF compile error.
///
/// Transformation:
/// - Dispatches interface files to the interface syntax parser and all other
///   files to the implementation syntax parser.
pub(crate) fn parse_source_as_syntax_output(
    path: &str,
    source: &str,
) -> crate::terlan_syntax::ebnf::EbnfCompileResult<crate::terlan_syntax::SyntaxModuleOutput> {
    if path.ends_with(".terli") {
        parse_interface_module_as_syntax_output(source)
    } else if path.ends_with(".terls") {
        parse_script_as_syntax_output(source, &script_module_name(Path::new(path)))
    } else {
        parse_module_as_syntax_output(source)
    }
}

/// Derives the compiler-owned module identity for a `.terls` source path.
///
/// Paths below a `scripts` directory retain that suffix, so
/// `scripts/release/Seal.terls` becomes `scripts.release.Seal`. Standalone
/// scripts use the stable `script.<stem>` namespace. No absolute build-root
/// component enters the generated identity.
pub(crate) fn script_module_name(path: &Path) -> String {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let script_root = components
        .iter()
        .rposition(|component| *component == "scripts");
    let selected = script_root.map_or_else(
        || components.last().into_iter().copied().collect::<Vec<_>>(),
        |index| components[index..].to_vec(),
    );
    let mut segments = selected
        .iter()
        .enumerate()
        .map(|(index, component)| {
            let raw = if index + 1 == selected.len() {
                component.strip_suffix(".terls").unwrap_or(component)
            } else {
                component
            };
            let normalized = raw
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '_' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            if normalized.is_empty() {
                "Script".to_string()
            } else {
                normalized
            }
        })
        .collect::<Vec<_>>();
    if script_root.is_none() {
        segments.insert(0, "script".to_string());
    }
    segments.join(".")
}

/// Runs the strict formal compile path for a selected backend profile.
///
/// Inputs:
/// - `path`: source path used for parser dispatch, diagnostics, and templates.
/// - `source`: Terlan source text.
/// - `diagnostic_format`: text or JSON diagnostic output mode.
/// - `cache_dir`: optional cache directory for dependency interfaces.
/// - `native_policy`: native interop policy enforced before parsing.
/// - `target_profile`: backend capability profile for CoreIR validation.
///
/// Output:
/// - Checked artifacts on success, or phase diagnostics and the exit code.
///
/// Transformation:
/// - Delegates to the formal pipeline, then validates lowered CoreIR against
///   selected backend profile constraints before exposing artifacts.
pub(crate) fn compile_syntax_module_through_phases_with_diagnostics_for_profile(
    path: &str,
    source: &str,
    diagnostic_format: DiagnosticFormat,
    cache_dir: Option<&Path>,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
) -> CompileSyntaxModuleThroughPhasesResult {
    compile_syntax_module_through_phases_with_diagnostics_for_profile_options(
        path,
        source,
        diagnostic_format,
        cache_dir,
        native_policy,
        target_profile,
        TargetProfileCheckOptions::default(),
    )
}

/// Runs the formal compile path with explicit target-profile validation options.
///
/// Inputs:
/// - `path`: source path used for parser dispatch, diagnostics, and templates.
/// - `source`: Terlan source text.
/// - `diagnostic_format`: text or JSON diagnostic output mode.
/// - `cache_dir`: optional cache directory for dependency interfaces.
/// - `native_policy`: native interop policy enforced before parsing.
/// - `target_profile`: backend capability profile for CoreIR validation.
/// - `target_profile_options`: command-owned validation options, such as
///   whether asset import resolution is handled by the command.
///
/// Output:
/// - Full phase result with artifacts or diagnostics.
///
/// Transformation:
/// - Preserves the strict parse/resolve/typecheck/CoreIR sequence while letting
///   commands declare narrowly scoped validation capabilities.
pub(crate) fn compile_syntax_module_through_phases_with_diagnostics_for_profile_options(
    path: &str,
    source: &str,
    diagnostic_format: DiagnosticFormat,
    cache_dir: Option<&Path>,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
    target_profile_options: TargetProfileCheckOptions,
) -> CompileSyntaxModuleThroughPhasesResult {
    compile_syntax_module_with_interfaces(
        path,
        source,
        diagnostic_format,
        InterfaceInputs {
            cache_dir,
            source_graph: None,
        },
        native_policy,
        target_profile,
        target_profile_options,
    )
}

/// Interface providers used while checking one source module.
pub(crate) struct InterfaceInputs<'a> {
    pub(crate) cache_dir: Option<&'a Path>,
    pub(crate) source_graph: Option<&'a interface_graph::SourceInterfaceGraph>,
}

/// Checks a source against an explicitly collected project interface graph.
/// Supplied interfaces override caches and inferred implementation summaries.
/// The caller owns validation of the graph's explicit source modules; reachable
/// external declarations remain subject to validation here.
pub(crate) fn compile_syntax_module_with_interfaces(
    path: &str,
    source: &str,
    diagnostic_format: DiagnosticFormat,
    interfaces: InterfaceInputs<'_>,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
    target_profile_options: TargetProfileCheckOptions,
) -> CompileSyntaxModuleThroughPhasesResult {
    let InterfaceInputs {
        cache_dir,
        source_graph: supplied_interfaces,
    } = interfaces;
    let mut timings = phase_timings::FormalPipelineTimings::new(path);
    let mut result = CompileSyntaxModuleThroughPhasesResult {
        artifacts: None,
        checked_interface: None,
        parse_diagnostics: Vec::new(),
        macro_expansion_diagnostics: Vec::new(),
        include_expansion_diagnostics: Vec::new(),
        resolve_diagnostics: Vec::new(),
        typecheck_diagnostics: Vec::new(),
        core_diagnostics: Vec::new(),
        exit_code: ExitCode::SUCCESS,
    };

    if let Err(message) = validate_native_policy(source, native_policy) {
        eprintln!("{}", message);
        result.parse_diagnostics.push(PhaseManifestDiagnostic {
            code: "NATIVE_POLICY",
            severity: "error",
            message,
            path: path.to_string(),
            span_start: 0,
            span_end: 0,
            ..Default::default()
        });
        result.exit_code = ExitCode::from(1);
        return result;
    }
    timings.mark("native-policy");

    let syntax_output = match parse_source_as_syntax_output(path, source) {
        Ok(output) => output,
        Err(crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                path,
                span.start,
                span.end,
                diagnostic_format,
            );
            result.parse_diagnostics.push(PhaseManifestDiagnostic {
                code: "parse_error",
                severity: "error",
                message,
                path: path.to_string(),
                span_start: span.start,
                span_end: span.end,
                ..Default::default()
            });
            result.exit_code = ExitCode::from(1);
            return result;
        }
        Err(crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
            eprintln!("{}", message);
            result.parse_diagnostics.push(PhaseManifestDiagnostic {
                code: "SYNTAX_OUTPUT_ERROR",
                severity: "error",
                message,
                path: path.to_string(),
                span_start: 0,
                span_end: 0,
                ..Default::default()
            });
            result.exit_code = ExitCode::from(1);
            return result;
        }
    };
    timings.mark("parse");

    let mut interfaces = supplied_interfaces.map_or_else(
        || interface_loading::load_external_interfaces_for_module(path, cache_dir, &syntax_output),
        |graph| graph.interfaces.clone(),
    );
    let dependency_diagnostics = interface_dependencies::check_interface_dependencies(
        Path::new(path),
        &syntax_output,
        cache_dir,
        &mut interfaces,
        supplied_interfaces,
    );
    timings.mark("interfaces");
    let (mut syntax_output, macro_expansion_diagnostics) =
        expand_syntax_macros_with_interfaces(syntax_output, &interfaces);
    timings.mark("macros");
    for diag in macro_expansion_diagnostics.iter() {
        crate::support::emit_diagnostic(
            "type_error",
            &diag.message,
            path,
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
        result
            .macro_expansion_diagnostics
            .push(PhaseManifestDiagnostic {
                code: "macro_expansion_error",
                severity: "error",
                message: diag.message.clone(),
                path: path.to_string(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            });
    }

    if !result.macro_expansion_diagnostics.is_empty() {
        result.exit_code = ExitCode::from(1);
        return result;
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
            path,
            span_start,
            span_end,
            diagnostic_format,
        );
        result.resolve_diagnostics.push(PhaseManifestDiagnostic {
            code: "shape_expansion_error",
            severity: "error",
            message,
            path: path.to_string(),
            span_start,
            span_end,
            ..Default::default()
        });
        result.exit_code = ExitCode::from(1);
        return result;
    }
    timings.mark("shapes");

    let resolved = resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
    timings.mark("resolve");
    for diag in resolved.diagnostics.iter() {
        crate::support::emit_diagnostic(
            "type_error",
            &diag.message,
            path,
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
        result.resolve_diagnostics.push(PhaseManifestDiagnostic {
            code: "resolve_error",
            severity: "error",
            message: diag.message.clone(),
            path: path.to_string(),
            span_start: diag.span.start,
            span_end: diag.span.end,
            ..Default::default()
        });
    }

    let (syntax_output, include_expansion_diagnostics) =
        expand_syntax_includes(syntax_output, &resolved);
    timings.mark("includes");
    for diag in include_expansion_diagnostics.iter() {
        crate::support::emit_diagnostic(
            "type_error",
            &diag.message,
            path,
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
        result
            .include_expansion_diagnostics
            .push(PhaseManifestDiagnostic {
                code: "include_expansion_error",
                severity: "error",
                message: diag.message.clone(),
                path: path.to_string(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            });
    }

    if !result.include_expansion_diagnostics.is_empty() {
        result.exit_code = ExitCode::from(1);
        return result;
    }

    let template_typecheck = type_check_syntax_module_output_with_template_inputs(
        &syntax_output,
        &resolved,
        Path::new(path),
    );
    timings.mark("typecheck");
    let checked_template_inputs = template_typecheck.template_inputs;
    let mut typecheck_diagnostics = template_typecheck.diagnostics;
    typecheck_diagnostics.extend(dependency_diagnostics);
    typecheck_diagnostics.extend(check_config_declarations_syntax_output(&syntax_output));
    let (interface_diagnostics, declared_interface) =
        interface_contract::check_declared_interface_and_surface(
            Path::new(path),
            &syntax_output,
            &interfaces,
        );
    typecheck_diagnostics.extend(interface_diagnostics);
    let mut has_type_errors = false;
    for diag in typecheck_diagnostics {
        let is_warning = matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning);
        has_type_errors = has_type_errors || !is_warning;
        let kind = crate::support::diagnostic_kind_for_message(
            if is_warning { "warning" } else { "type_error" },
            &diag.message,
        );
        crate::support::emit_diagnostic(
            kind,
            &diag.message,
            path,
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
        result.typecheck_diagnostics.push(PhaseManifestDiagnostic {
            code: if is_warning {
                "type_warning"
            } else if kind == "module_import" {
                "module_import"
            } else {
                "type_error"
            },
            severity: if is_warning { "warning" } else { "error" },
            message: diag.message,
            path: path.to_string(),
            span_start: diag.span.start,
            span_end: diag.span.end,
            ..Default::default()
        });
    }
    if has_type_errors || !result.resolve_diagnostics.is_empty() {
        result.exit_code = ExitCode::from(1);
    } else if syntax_output.source_kind == crate::terlan_syntax::SyntaxSourceKind::Interface {
        result.checked_interface = Some(CheckedInterfaceArtifacts {
            interface: crate::terlan_hir::syntax_module_output_to_interface(&syntax_output),
            syntax_output,
            interfaces,
        });
    } else {
        let mut core =
            crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        if let Some(interface) = declared_interface {
            interface_contract::restrict_module_exports(&mut core, &interface);
        }
        timings.mark("core-lowering");
        core.source.source_path = Some(path.to_string());
        match templates::core_template_render_plans(checked_template_inputs, &core) {
            Ok(templates) => core.templates = templates,
            Err(message) => {
                crate::support::emit_diagnostic(
                    "type_error",
                    &message,
                    path,
                    0,
                    0,
                    diagnostic_format,
                );
                result.core_diagnostics.push(PhaseManifestDiagnostic {
                    code: "template_render_plan",
                    severity: "error",
                    message,
                    path: path.to_string(),
                    span_start: 0,
                    span_end: 0,
                    ..Default::default()
                });
                result.exit_code = ExitCode::from(1);
                return result;
            }
        }
        timings.mark("templates");
        if let Err(message) = core.binding_identities.validate() {
            let message = message.to_string();
            crate::support::emit_diagnostic("type_error", &message, path, 0, 0, diagnostic_format);
            result.core_diagnostics.push(PhaseManifestDiagnostic {
                code: "binding_identity_evidence",
                severity: "error",
                message,
                path: path.to_string(),
                span_start: 0,
                span_end: 0,
                ..Default::default()
            });
            result.exit_code = ExitCode::from(1);
            return result;
        }
        timings.mark("binding-identities");
        if let Err(message) = crate::terlan_typeck::validate_core_termination_evidence(&core) {
            let message = message.to_string();
            crate::support::emit_diagnostic("type_error", &message, path, 0, 0, diagnostic_format);
            result.core_diagnostics.push(PhaseManifestDiagnostic {
                code: "termination_evidence",
                severity: "error",
                message,
                path: path.to_string(),
                span_start: 0,
                span_end: 0,
                ..Default::default()
            });
            result.exit_code = ExitCode::from(1);
            return result;
        }
        timings.mark("termination");
        let target_profile_violations =
            target_profile_checks_with_options(&core, target_profile, target_profile_options);
        timings.mark("target-profile");
        if !target_profile_violations.is_empty() {
            for violation in target_profile_violations {
                crate::support::emit_diagnostic(
                    "type_error",
                    &violation.message,
                    path,
                    0,
                    0,
                    diagnostic_format,
                );
                result.core_diagnostics.push(PhaseManifestDiagnostic {
                    code: violation.code,
                    severity: "error",
                    message: violation.message,
                    path: path.to_string(),
                    span_start: 0,
                    span_end: 0,
                    ..Default::default()
                });
            }
            result.exit_code = ExitCode::from(1);
            return result;
        }

        result.core_diagnostics = Vec::new();
        result.artifacts = Some(CheckedSyntaxModuleArtifacts {
            syntax_output,
            interfaces,
            core,
        });
        return result;
    }

    result
}

/// Runs the strict formal compile path with an explicit backend target profile.
///
/// Inputs:
/// - `path`: source path used for parser dispatch, diagnostics, and templates.
/// - `source`: Terlan source text.
/// - `diagnostic_format`: text or JSON diagnostic output mode.
/// - `cache_dir`: optional cache directory for dependency interfaces.
/// - `native_policy`: native interop policy enforced before parsing.
/// - `target_profile`: backend capability profile for CoreIR validation.
///
/// Output:
/// - `Ok(CheckedSyntaxModuleArtifacts)` when compilation passes all phases, or
///   `Err(ExitCode)` when any phase fails.
///
/// Transformation:
/// - Delegates to the diagnostic pipeline and enforces the backend target-profile
///   gate before returning artifacts.
pub(crate) fn compile_syntax_module_through_phases_with_profile(
    path: &str,
    source: &str,
    diagnostic_format: DiagnosticFormat,
    cache_dir: Option<&Path>,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
) -> Result<CheckedSyntaxModuleArtifacts, ExitCode> {
    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        path,
        source,
        diagnostic_format,
        cache_dir,
        native_policy,
        target_profile,
    );
    compile_result_to_artifacts(result)
}

/// Runs the strict formal compile path with command-owned target validation
/// options.
///
/// Inputs:
/// - Same source, diagnostics, cache, native-policy, and target profile inputs
///   as `compile_syntax_module_through_phases_with_profile`.
/// - `target_profile_options`: options for command-owned capabilities.
///
/// Output:
/// - `Ok(CheckedSyntaxModuleArtifacts)` when compilation passes all phases, or
///   `Err(ExitCode)` when any phase fails.
///
/// Transformation:
/// - Delegates to the diagnostic pipeline with explicit target-profile options
///   and unwraps successful artifacts for command handlers.
pub(crate) fn compile_syntax_module_through_phases_with_profile_options(
    path: &str,
    source: &str,
    diagnostic_format: DiagnosticFormat,
    cache_dir: Option<&Path>,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
    target_profile_options: TargetProfileCheckOptions,
) -> Result<CheckedSyntaxModuleArtifacts, ExitCode> {
    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile_options(
        path,
        source,
        diagnostic_format,
        cache_dir,
        native_policy,
        target_profile,
        target_profile_options,
    );
    compile_result_to_artifacts(result)
}

/// Extracts successful checked artifacts from a full phase result.
///
/// Inputs:
/// - `result`: full formal pipeline result.
///
/// Output:
/// - Checked artifacts on success, or an exit code on failure.
///
/// Transformation:
/// - Converts the diagnostic-rich pipeline result into the compact command API
///   used by emit, test, REPL, and static-site commands.
fn compile_result_to_artifacts(
    result: CompileSyntaxModuleThroughPhasesResult,
) -> Result<CheckedSyntaxModuleArtifacts, ExitCode> {
    if result.exit_code != ExitCode::SUCCESS {
        return Err(result.exit_code);
    }
    if result.checked_interface.is_some() {
        eprintln!(
            "error[interface_not_executable]: an interface declares an API; compile its .terl implementation to produce executable artifacts"
        );
        return Err(ExitCode::from(1));
    }
    result.artifacts.ok_or_else(|| ExitCode::from(1))
}

#[cfg(test)]
#[path = "formal_pipeline_test.rs"]
#[cfg(test)]
mod formal_pipeline_test;
