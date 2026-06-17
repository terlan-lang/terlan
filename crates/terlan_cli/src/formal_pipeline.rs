use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use terlan_hir::{
    load_interfaces_from_dir, load_interfaces_from_file_set,
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    ModuleInterface,
};
use terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
    SyntaxDeclarationPayload, SyntaxExprOutput, SyntaxModuleOutput,
};
use terlan_typeck::{expand_syntax_derives, expand_syntax_raw_macros};

use crate::validation::config_contract::check_config_declarations_syntax_output;
use crate::validation::native_policy::{validate_native_policy, NativePolicy};
use crate::validation::phase_manifest::PhaseManifestDiagnostic;
use crate::validation::target_profile::{
    target_profile_checks_with_options, TargetProfile, TargetProfileCheckOptions,
};
use crate::validation::template_contract::type_check_syntax_module_output_with_templates;
use crate::DiagnosticFormat;

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
    pub(crate) core: terlan_typeck::CoreModule,
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
    pub(crate) parse_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) macro_expansion_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) derive_expansion_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) resolve_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) typecheck_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) core_diagnostics: Vec<PhaseManifestDiagnostic>,
    pub(crate) exit_code: ExitCode,
}

/// Loads external module interfaces visible to a source file.
///
/// Inputs:
/// - `path`: source path used to locate adjacent interface files.
/// - `cache_dir`: optional cache directory containing emitted interfaces.
///
/// Output:
/// - Interface map keyed by module name.
///
/// Transformation:
/// - Starts with interfaces from the source file set, loads cached/generated
///   interfaces when a cache directory is configured, then fills only missing
///   stdlib interfaces from summaries embedded in the compiler binary.
pub(crate) fn load_external_interfaces(
    path: &str,
    cache_dir: Option<&Path>,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = load_interfaces_from_file_set(path);
    if let Some(cache_dir) = cache_dir {
        load_interfaces_from_dir(cache_dir, &mut interfaces);
    }
    load_embedded_std_interfaces(&mut interfaces);
    interfaces
}

/// Loads compiler-embedded stdlib interface summaries as a fallback.
///
/// Inputs:
/// - `interfaces`: mutable interface map already populated from the source
///   file set.
///
/// Output:
/// - `interfaces` contains packaged stdlib summaries for modules not already
///   discovered locally.
///
/// Transformation:
/// - Parses the checked-in `.typi` summaries embedded into the compiler binary
///   and inserts each parsed interface only when the caller has not already
///   supplied an interface for that module.
pub(crate) fn load_embedded_std_interfaces(interfaces: &mut HashMap<String, ModuleInterface>) {
    for summary in EMBEDDED_STD_INTERFACE_SUMMARIES {
        let Some((module_name, interface)) = parse_embedded_std_interface(summary) else {
            continue;
        };
        interfaces.entry(module_name).or_insert(interface);
    }
}

/// Parses one embedded stdlib interface summary.
///
/// Inputs:
/// - `summary`: `.typi` source text embedded at compile time.
///
/// Output:
/// - Parsed module name and compiler interface when the summary is valid.
///
/// Transformation:
/// - Reuses the normal interface parser and HIR interface extraction so
///   embedded std summaries have the same shape as file-loaded summaries.
fn parse_embedded_std_interface(summary: &str) -> Option<(String, ModuleInterface)> {
    let parsed = parse_interface_module_as_syntax_output(summary).ok()?;
    let module_name = parsed.module_name.clone();
    let interface = syntax_module_output_to_interface(&parsed);
    Some((module_name, interface))
}

const EMBEDDED_STD_INTERFACE_SUMMARIES: &[&str] = &[
    include_str!("../../../std/summaries/std.beam.Agent.typi"),
    include_str!("../../../std/summaries/std.beam.Backpressure.typi"),
    include_str!("../../../std/summaries/std.beam.GenServer.typi"),
    include_str!("../../../std/summaries/std.beam.Message.typi"),
    include_str!("../../../std/summaries/std.beam.NativeBridge.typi"),
    include_str!("../../../std/summaries/std.beam.Process.typi"),
    include_str!("../../../std/summaries/std.beam.Supervisor.typi"),
    include_str!("../../../std/summaries/std.beam.Task.typi"),
    include_str!("../../../std/summaries/std.core.Atom.typi"),
    include_str!("../../../std/summaries/std.core.Bool.typi"),
    include_str!("../../../std/summaries/std.core.Equal.typi"),
    include_str!("../../../std/summaries/std.core.Error.typi"),
    include_str!("../../../std/summaries/std.core.Float.typi"),
    include_str!("../../../std/summaries/std.core.Int.typi"),
    include_str!("../../../std/summaries/std.collections.Enumerable.typi"),
    include_str!("../../../std/summaries/std.collections.Iterable.typi"),
    include_str!("../../../std/summaries/std.collections.Iterator.typi"),
    include_str!("../../../std/summaries/std.collections.List.typi"),
    include_str!("../../../std/summaries/std.collections.Map.typi"),
    include_str!("../../../std/summaries/std.data.Json.typi"),
    include_str!("../../../std/summaries/std.encoding.Base64.typi"),
    include_str!("../../../std/summaries/std.native.collections.Vector.typi"),
    include_str!("../../../std/summaries/std.core.Option.typi"),
    include_str!("../../../std/summaries/std.core.Ordering.typi"),
    include_str!("../../../std/summaries/std.core.Result.typi"),
    include_str!("../../../std/summaries/std.collections.Set.typi"),
    include_str!("../../../std/summaries/std.core.String.typi"),
    include_str!("../../../std/summaries/std.core.Task.typi"),
    include_str!("../../../std/summaries/std.core.Unit.typi"),
    include_str!("../../../std/summaries/std.core.typi"),
    include_str!("../../../std/summaries/std.io.Console.typi"),
    include_str!("../../../std/summaries/std.io.File.typi"),
    include_str!("../../../std/summaries/std.io.Path.typi"),
    include_str!("../../../std/summaries/std.io.typi"),
    include_str!("../../../std/summaries/std.net.Uri.typi"),
    include_str!("../../../std/summaries/std.test.Test.typi"),
];

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
///   `tl` extension so directory-mode compiler commands can consume package-
///   rooted source layouts.
pub(crate) fn terlan_sources_in_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_terlan_sources_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects Terlan implementation sources.
///
/// Inputs:
/// - `dir`: directory currently being scanned.
/// - `files`: mutable collection of discovered `.terl` source paths.
///
/// Output:
/// - `Ok(())` when the directory and all nested directories are scanned.
/// - `Err(message)` when a directory entry or metadata read fails.
///
/// Transformation:
/// - Reads one directory level, sorts child paths for stable traversal, appends
///   `.terl` files, and recurses into child directories without following
///   symlinked directories.
fn collect_terlan_sources_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read dir {}: {}", dir.display(), err))?;
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read dir entry: {err}"))?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "failed to read file type for {}: {err}",
                entry.path().display()
            )
        })?;
        children.push((entry.path(), file_type));
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));

    for (path, file_type) in children {
        if file_type.is_dir() {
            collect_terlan_sources_recursive(&path, files)?;
        } else if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("terl")
        {
            files.push(path);
        }
    }
    Ok(())
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
) -> terlan_syntax::ebnf::EbnfCompileResult<terlan_syntax::SyntaxModuleOutput> {
    if path.ends_with(".terli") {
        parse_interface_module_as_syntax_output(source)
    } else {
        parse_module_as_syntax_output(source)
    }
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
    let mut result = CompileSyntaxModuleThroughPhasesResult {
        artifacts: None,
        parse_diagnostics: Vec::new(),
        macro_expansion_diagnostics: Vec::new(),
        derive_expansion_diagnostics: Vec::new(),
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

    let syntax_output = match parse_source_as_syntax_output(path, source) {
        Ok(output) => output,
        Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
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
        Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
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

    let interfaces = load_external_interfaces(path, cache_dir);
    let (syntax_output, macro_expansion_diagnostics) = expand_syntax_raw_macros(syntax_output);
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

    let resolved = resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
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

    let (syntax_output, derive_expansion_diagnostics) =
        expand_syntax_derives(syntax_output, &resolved);
    for diag in derive_expansion_diagnostics.iter() {
        crate::support::emit_diagnostic(
            "type_error",
            &diag.message,
            path,
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
        result
            .derive_expansion_diagnostics
            .push(PhaseManifestDiagnostic {
                code: "derive_expansion_error",
                severity: "error",
                message: diag.message.clone(),
                path: path.to_string(),
                span_start: diag.span.start,
                span_end: diag.span.end,
                ..Default::default()
            });
    }

    if !result.derive_expansion_diagnostics.is_empty() {
        result.exit_code = ExitCode::from(1);
        return result;
    }

    let mut typecheck_diagnostics =
        type_check_syntax_module_output_with_templates(&syntax_output, &resolved, Path::new(path));
    typecheck_diagnostics.extend(check_config_declarations_syntax_output(&syntax_output));
    let mut has_type_errors = false;
    for diag in typecheck_diagnostics {
        let is_warning = matches!(diag.severity, terlan_typeck::DiagSeverity::Warning);
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
    } else {
        let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        let target_profile_violations =
            target_profile_checks_with_options(&core, target_profile, target_profile_options);
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
    result.artifacts.ok_or_else(|| ExitCode::from(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::validation::target_profile::TargetProfile;

    #[test]
    fn compile_syntax_module_with_erlang_profile_accepts_float() {
        let source = "\
module target_profile_accept.

pub f(): Float ->
  1.0.
";

        let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
            "src/target_profile_accept.terl",
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            TargetProfile::Erlang,
        );

        assert_eq!(result.exit_code, ExitCode::SUCCESS);
        assert!(result.artifacts.is_some());
        assert!(result.core_diagnostics.is_empty());
    }

    #[test]
    fn compile_syntax_module_with_profile_argument_accepts_float() {
        let source = "\
module target_profile_reject.

pub f(): Float ->
  1.0.
";

        let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
            "src/target_profile_reject.terl",
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            TargetProfile::Erlang,
        );

        assert_eq!(result.exit_code, ExitCode::SUCCESS);
        assert!(result.artifacts.is_some());
        assert!(result.core_diagnostics.is_empty());
    }

    /// Verifies the strict formal compile path accepts the portable CoreIR v0
    /// target subset for a Lean-covered body.
    ///
    /// Inputs:
    /// - Source text whose function body lowers to typed integer subtraction.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are written.
    ///
    /// Transformation:
    /// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
    ///   `TargetProfile::CoreV0` and asserts no profile diagnostics are emitted.
    #[test]
    fn compile_syntax_module_with_core_v0_profile_accepts_covered_subset() {
        let source = "\
module target_profile_core_v0_accept.

pub f(x: Int, y: Int): Int ->
  x - y.
";

        let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
            "src/target_profile_core_v0_accept.terl",
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            TargetProfile::CoreV0,
        );

        assert_eq!(result.exit_code, ExitCode::SUCCESS);
        assert!(result.artifacts.is_some());
        assert!(result.core_diagnostics.is_empty());
    }

    /// Verifies the strict formal compile path rejects CoreIR outside the
    /// portable CoreIR v0 target subset.
    ///
    /// Inputs:
    /// - Source text whose function body lowers to a typed map expression.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are written.
    ///
    /// Transformation:
    /// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
    ///   `TargetProfile::CoreV0` and asserts target-profile diagnostics abort
    ///   compilation before artifacts are returned.
    #[test]
    fn compile_syntax_module_with_core_v0_profile_rejects_broad_coreir() {
        let source = "\
module target_profile_core_v0_reject.

pub f(): Map ->
  #{a := 1}.
";

        let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
            "src/target_profile_core_v0_reject.terl",
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            TargetProfile::CoreV0,
        );

        assert_ne!(result.exit_code, ExitCode::SUCCESS);
        assert!(result.artifacts.is_none());
        assert!(
            result
                .core_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "target_profile_unsupported"),
            "Core v0 profile should report target-profile violations"
        );
    }

    /// Verifies native vector interface summaries are embedded with stdlib.
    ///
    /// Inputs:
    /// - Empty interface map.
    ///
    /// Output:
    /// - Test passes when `std.native.collections.Vector` is loaded from the
    ///   compiler-embedded std summary list.
    ///
    /// Transformation:
    /// - Exercises normal embedded summary parsing so native std modules are
    ///   available for import resolution before target-capability diagnostics
    ///   decide whether the active backend may compile them.
    #[test]
    fn embedded_std_interfaces_include_native_vector_contract() {
        let mut interfaces = HashMap::new();

        load_embedded_std_interfaces(&mut interfaces);

        let interface = interfaces
            .get("std.native.collections.Vector")
            .expect("embedded native vector interface");
        assert!(interface.opaque_types.contains("Vector"));
        assert!(interface.functions.contains_key(&("new".to_string(), 0)));
        let length = interface
            .functions
            .get(&("length".to_string(), 1))
            .expect("Vector.length receiver method");
        assert!(length.receiver_method);
        assert!(!length.receiver_mutable);
        let set_at = interface
            .functions
            .get(&("set_at".to_string(), 3))
            .expect("Vector.set_at mutable receiver method");
        assert!(set_at.receiver_method);
        assert!(set_at.receiver_mutable);
    }

    /// Verifies embedded std summaries include the portable task contract.
    ///
    /// Inputs:
    /// - Compiler-embedded std interface summaries.
    ///
    /// Output:
    /// - Test passes when `std.core.Task` is loaded from the embedded summary
    ///   list with its opaque type and receiver composition methods.
    ///
    /// Transformation:
    /// - Exercises normal embedded summary parsing so project imports can
    ///   resolve the typed async contract before target profiles decide whether
    ///   a backend can execute it.
    #[test]
    fn embedded_std_interfaces_include_core_task_contract() {
        let mut interfaces = HashMap::new();

        load_embedded_std_interfaces(&mut interfaces);

        let interface = interfaces
            .get("std.core.Task")
            .expect("embedded core task interface");
        assert!(interface.opaque_types.contains("Task"));
        assert!(interface.functions.contains_key(&("done".to_string(), 1)));
        assert!(interface.functions.contains_key(&("spawn".to_string(), 1)));
        let then = interface
            .functions
            .get(&("then".to_string(), 2))
            .expect("Task.then receiver method");
        assert!(then.receiver_method);
        assert!(!then.receiver_mutable);
        let result = interface
            .functions
            .get(&("result".to_string(), 1))
            .expect("Task.result receiver method");
        assert!(result.receiver_method);
    }

    /// Verifies embedded std summaries include the portable JSON contract.
    ///
    /// Inputs:
    /// - Compiler-embedded std interface summaries.
    ///
    /// Output:
    /// - Test passes when `std.data.Json` is loaded from the embedded summary
    ///   list with its opaque type, derived error type, and receiver accessors.
    ///
    /// Transformation:
    /// - Exercises normal embedded summary parsing so project imports can
    ///   resolve the JSON API before target profiles decide whether a backend
    ///   can execute the Rust/SafeNative implementation.
    #[test]
    fn embedded_std_interfaces_include_data_json_contract() {
        let mut interfaces = HashMap::new();

        load_embedded_std_interfaces(&mut interfaces);

        let interface = interfaces
            .get("std.data.Json")
            .expect("embedded data json interface");
        assert!(interface.opaque_types.contains("Json"));
        assert!(interface.public_types.contains("JsonError"));
        assert!(interface.functions.contains_key(&("parse".to_string(), 1)));
        assert!(interface
            .functions
            .contains_key(&("stringify".to_string(), 1)));
        let get = interface
            .functions
            .get(&("get".to_string(), 2))
            .expect("Json.get receiver method");
        assert!(get.receiver_method);
        assert!(!get.receiver_mutable);
        let is_null = interface
            .functions
            .get(&("is_null".to_string(), 1))
            .expect("Json.is_null receiver method");
        assert!(is_null.receiver_method);
        assert!(!is_null.receiver_mutable);
    }

    /// Verifies embedded std summaries include Rust-backed web/data utilities.
    ///
    /// Inputs:
    /// - Compiler-embedded std interface summaries.
    ///
    /// Output:
    /// - Test passes when `std.encoding.Base64`, `std.io.Path`, and
    ///   `std.net.Uri` are loaded from the embedded summary list with their
    ///   public contract surfaces.
    ///
    /// Transformation:
    /// - Exercises normal embedded summary parsing so project imports can
    ///   resolve portable web/data utility APIs before target profiles decide
    ///   whether a backend can execute their Rust/SafeNative implementations.
    #[test]
    fn embedded_std_interfaces_include_web_data_utility_contracts() {
        let mut interfaces = HashMap::new();

        load_embedded_std_interfaces(&mut interfaces);

        let base64 = interfaces
            .get("std.encoding.Base64")
            .expect("embedded Base64 interface");
        assert!(base64.public_types.contains("Base64Error"));
        assert!(base64.functions.contains_key(&("encode".to_string(), 1)));
        assert!(base64.functions.contains_key(&("decode".to_string(), 1)));

        let path = interfaces
            .get("std.io.Path")
            .expect("embedded Path interface");
        assert!(path.opaque_types.contains("Path"));
        assert!(path.public_types.contains("PathError"));
        assert!(path.functions.contains_key(&("from_string".to_string(), 1)));
        let join = path
            .functions
            .get(&("join".to_string(), 2))
            .expect("Path.join receiver method");
        assert!(join.receiver_method);

        let uri = interfaces
            .get("std.net.Uri")
            .expect("embedded Uri interface");
        assert!(uri.opaque_types.contains("Uri"));
        assert!(uri.public_types.contains("UriError"));
        assert!(uri.functions.contains_key(&("parse".to_string(), 1)));
        let host = uri
            .functions
            .get(&("host".to_string(), 1))
            .expect("Uri.host receiver method");
        assert!(host.receiver_method);
    }

    /// Verifies embedded std summaries include the BEAM bridge contracts.
    ///
    /// Inputs:
    /// - Compiler-embedded std interface summaries.
    ///
    /// Output:
    /// - Test passes when the first BEAM bridge and Agent contract modules are
    ///   loaded from the embedded summary list with their target-gated types,
    ///   traits, and receiver methods.
    ///
    /// Transformation:
    /// - Exercises normal embedded summary parsing so BEAM supervision,
    ///   process, message, backpressure, and native-bridge contracts can be
    ///   resolved without adding BEAM-specific grammar to Terlan source.
    #[test]
    fn embedded_std_interfaces_include_beam_bridge_contracts() {
        let mut interfaces = HashMap::new();

        load_embedded_std_interfaces(&mut interfaces);

        let agent = interfaces
            .get("std.beam.Agent")
            .expect("embedded BEAM Agent interface");
        assert!(agent.opaque_types.contains("Agent"));
        assert!(agent.functions.contains_key(&("start".to_string(), 1)));
        let get = agent
            .functions
            .get(&("get".to_string(), 1))
            .expect("Agent.get receiver method");
        assert!(get.receiver_method);
        assert!(!get.receiver_mutable);
        let update = agent
            .functions
            .get(&("update".to_string(), 2))
            .expect("Agent.update mutable receiver method");
        assert!(update.receiver_method);
        assert!(update.receiver_mutable);
        let get_and_update = agent
            .functions
            .get(&("get_and_update".to_string(), 2))
            .expect("Agent.get_and_update receiver method");
        assert!(get_and_update.receiver_method);
        assert!(!get_and_update.receiver_mutable);

        let process = interfaces
            .get("std.beam.Process")
            .expect("embedded BEAM process interface");
        assert!(process.opaque_types.contains("Process"));
        let process_like = process
            .traits
            .get("ProcessLike")
            .expect("embedded ProcessLike trait contract");
        assert!(process_like.methods.contains_key("send"));
        assert!(process_like.methods.contains_key("stop"));

        let message = interfaces
            .get("std.beam.Message")
            .expect("embedded BEAM message interface");
        assert!(message.opaque_types.contains("Message"));
        let message_codec = message
            .traits
            .get("MessageCodec")
            .expect("embedded MessageCodec trait contract");
        assert!(message_codec.methods.contains_key("wrap"));
        assert!(message_codec.methods.contains_key("unwrap"));

        let backpressure = interfaces
            .get("std.beam.Backpressure")
            .expect("embedded BEAM backpressure interface");
        assert!(backpressure.public_types.contains("Credit"));
        let backpressure_trait = backpressure
            .traits
            .get("Backpressure")
            .expect("embedded Backpressure trait contract");
        assert!(backpressure_trait.methods.contains_key("available"));
        assert!(backpressure_trait.methods.contains_key("request"));
        assert!(backpressure_trait.methods.contains_key("release"));

        let supervisor = interfaces
            .get("std.beam.Supervisor")
            .expect("embedded BEAM supervisor interface");
        assert!(supervisor.opaque_types.contains("Supervisor"));
        assert!(supervisor.opaque_types.contains("ChildSpec"));
        assert!(supervisor
            .functions
            .contains_key(&("child_spec".to_string(), 1)));
        let supervisor_start = supervisor
            .functions
            .get(&("start".to_string(), 2))
            .expect("Supervisor.start receiver method");
        assert!(supervisor_start.receiver_method);
        assert!(!supervisor_start.receiver_mutable);
        let supervisor_stop = supervisor
            .functions
            .get(&("stop".to_string(), 2))
            .expect("Supervisor.stop mutable receiver method");
        assert!(supervisor_stop.receiver_method);
        assert!(supervisor_stop.receiver_mutable);
        assert!(supervisor.traits.contains_key("Supervised"));

        let gen_server = interfaces
            .get("std.beam.GenServer")
            .expect("embedded BEAM GenServer interface");
        assert!(gen_server.public_types.contains("CallReply"));
        assert!(gen_server.opaque_types.contains("ServerRef"));
        assert!(gen_server.functions.contains_key(&("start".to_string(), 1)));
        let call = gen_server
            .functions
            .get(&("call".to_string(), 2))
            .expect("GenServer.call receiver method");
        assert!(call.receiver_method);
        assert!(!call.receiver_mutable);
        let cast = gen_server
            .functions
            .get(&("cast".to_string(), 2))
            .expect("GenServer.cast mutable receiver method");
        assert!(cast.receiver_method);
        assert!(cast.receiver_mutable);
        let stop = gen_server
            .functions
            .get(&("stop".to_string(), 1))
            .expect("GenServer.stop mutable receiver method");
        assert!(stop.receiver_method);
        assert!(stop.receiver_mutable);
        let gen_server_trait = gen_server
            .traits
            .get("GenServer")
            .expect("embedded GenServer trait contract");
        assert!(gen_server_trait.methods.contains_key("init"));
        assert!(gen_server_trait.methods.contains_key("handle_call"));
        assert!(gen_server_trait.methods.contains_key("handle_cast"));
        assert!(
            gen_server_trait
                .methods
                .get("terminate")
                .expect("GenServer terminate callback")
                .has_default
        );

        let native_bridge = interfaces
            .get("std.beam.NativeBridge")
            .expect("embedded BEAM native bridge interface");
        assert!(native_bridge.opaque_types.contains("NativeBridge"));
        assert!(native_bridge
            .functions
            .contains_key(&("start".to_string(), 1)));
        let native_call = native_bridge
            .functions
            .get(&("call".to_string(), 2))
            .expect("NativeBridge.call receiver method");
        assert!(native_call.receiver_method);
        assert!(!native_call.receiver_mutable);
        let dispose = native_bridge
            .functions
            .get(&("dispose".to_string(), 1))
            .expect("NativeBridge.dispose mutable receiver method");
        assert!(dispose.receiver_method);
        assert!(dispose.receiver_mutable);
        let native_stop = native_bridge
            .functions
            .get(&("stop".to_string(), 1))
            .expect("NativeBridge.stop mutable receiver method");
        assert!(native_stop.receiver_method);
        assert!(native_stop.receiver_mutable);
        let native_bridge_runtime = native_bridge
            .traits
            .get("NativeBridgeRuntime")
            .expect("embedded NativeBridgeRuntime trait contract");
        assert!(native_bridge_runtime
            .super_traits
            .contains(&"Supervised[NativeBridge[Resource]]".to_string()));
        assert!(native_bridge_runtime
            .super_traits
            .contains(&"Backpressure[NativeBridge[Resource]]".to_string()));
        assert!(native_bridge_runtime
            .super_traits
            .contains(&"MessageCodec[Command]".to_string()));
        assert!(native_bridge_runtime
            .super_traits
            .contains(&"MessageCodec[Reply]".to_string()));

        let task = interfaces
            .get("std.beam.Task")
            .expect("embedded BEAM Task interface");
        assert!(task.opaque_types.contains("Task"));
        assert!(task.functions.contains_key(&("start".to_string(), 1)));
        let result = task
            .functions
            .get(&("result".to_string(), 1))
            .expect("Task.result receiver method");
        assert!(result.receiver_method);
        assert!(!result.receiver_mutable);
        let cancel = task
            .functions
            .get(&("cancel".to_string(), 1))
            .expect("Task.cancel mutable receiver method");
        assert!(cancel.receiver_method);
        assert!(cancel.receiver_mutable);
    }
}
