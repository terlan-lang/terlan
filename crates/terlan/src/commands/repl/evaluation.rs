/// Validates target evidence for REPL seed or load sources.
///
/// Inputs:
/// - `seed_path`: optional `.terl` file or project directory supplied at REPL
///   startup or through `:load`.
/// - `requested`: global or runtime-selected target profile.
///
/// Output:
/// - `Ok(())` when no seed is present or the seed can execute in the VM REPL.
/// - `Err(message)` when typed evidence requires a non-VM target or conflicts
///   with an explicit target profile.
///
/// Transformation:
/// - Loads the same source files as REPL seed loading, parses imports into the
///   shared target-evidence bag, and preserves normal load diagnostics by
///   ignoring filesystem or parse errors that `load_repl_seed_declarations`
///   will report immediately afterward.
pub(super) fn validate_repl_seed_target_evidence(
    seed_path: Option<&str>,
    requested: TargetProfile,
) -> Result<(), String> {
    let Some(seed_path) = seed_path else {
        return Ok(());
    };

    let Ok(sources) = source::repl_load_sources(Path::new(seed_path)) else {
        return Ok(());
    };
    let mut syntax_outputs = Vec::new();
    for (_path, source) in sources {
        let Ok(syntax) = parse_module_as_syntax_output(&source) else {
            return Ok(());
        };
        syntax_outputs.push(syntax);
    }

    let input = TargetInferenceInput::from_syntax_modules(syntax_outputs.iter());
    let inference = infer_target_profile_from_typed_evidence(&input)
        .map_err(|conflict| format!("terlc repl target inference error: {}", conflict.message))?;

    if requested != TargetProfile::Vm {
        if let Some(message) = explicit_target_profile_override_error(&inference, requested) {
            return Err(format!("terlc repl target inference error: {message}"));
        }
        return Err(format!(
            "terlc repl target inference error: REPL runtime `vm` executes VM programs, but explicit target `{}` was requested",
            requested.as_str()
        ));
    }

    if inference.profile != TargetProfile::Vm {
        return Err(format!(
            "terlc repl target inference error: REPL runtime `vm` executes VM programs, but seed source evidence requires `{}`",
            inference.profile.as_str()
        ));
    }

    Ok(())
}

/// Removes the required REPL entry terminator from an expression entry.
///
/// Inputs:
/// - `entry`: raw non-command REPL input.
///
/// Output:
/// - Expression source without the trailing `.` when the entry is terminated.
/// - `None` when the entry does not use normal Terlan termination.
///
/// Transformation:
/// - Trims surrounding whitespace and removes exactly the final source
///   terminator; ordinary expression parsing then uses the same expression
///   parser used by the compiler pipeline.
pub(super) fn repl_expression_source(entry: &str) -> Option<&str> {
    entry
        .trim()
        .strip_suffix('.')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Evaluates REPL prompt inputs for documentation validation.
///
/// Inputs:
/// - `inputs`: ordered prompt entries including normal Terlan `.` terminators.
/// - `diagnostic_format`: diagnostic mode used by compiler phases.
/// - `native_policy`: native-code policy enforced during compilation.
/// - `target_profile`: target-profile gate enforced during compilation.
///
/// Output:
/// - One output-line list per input, including captured console output and the
///   final rendered result value.
/// - Error text when an input is not valid REPL source or cannot evaluate.
///
/// Transformation:
/// - Runs prompt entries through the same declaration, import, persistent
///   binding, expression, native-image compilation, and shard execution path
///   as interactive `terlc repl`, while capturing output instead of printing
///   it.
pub(crate) fn evaluate_repl_prompt_inputs(
    inputs: &[String],
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
) -> Result<Vec<Vec<String>>, String> {
    evaluate_repl_prompt_inputs_with_publication(
        inputs,
        diagnostic_format,
        native_policy,
        target_profile,
    )
    .map(|(outputs, _events)| outputs)
}

/// Evaluates REPL prompt inputs and publishes each generated expression module.
///
/// Inputs:
/// - `inputs`: ordered prompt entries including normal Terlan `.` terminators.
/// - `diagnostic_format`: diagnostic mode used by compiler phases.
/// - `native_policy`: native-code policy enforced during compilation.
/// - `target_profile`: target-profile gate enforced during compilation.
///
/// Output:
/// - One output-line list per input.
/// - Ordered VM code-server event snapshots for generated expression modules.
/// - Error text when an input is not valid REPL source or cannot evaluate.
///
/// Transformation:
/// - Compiles prompt entries through the persistent native-image service and
///   publishes each generated expression source into `VmCodeServer`, proving
///   the prompt path can replace admitted generations without restarting the
///   VM.
pub(crate) fn evaluate_repl_prompt_inputs_with_publication(
    inputs: &[String],
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
) -> Result<(Vec<Vec<String>>, Vec<VmCodeServerEventSnapshot>), String> {
    let (module_name, temp_dir) = repl_generated_workspace("repl_doc")?;
    let mut declarations = Vec::new();
    let mut value_bindings = Vec::new();
    let mut outputs = Vec::new();
    let mut code_server = VmCodeServer::default();
    let mut compiler_service = ReplCompilerService::default();

    let result = (|| {
        for input in inputs {
            let trimmed = input.trim();
            if trimmed.starts_with(':') {
                return Err("REPL doc examples cannot use control commands".to_string());
            }
            let Some(expression_source) = repl_expression_source(trimmed) else {
                return Err(format!(
                    "REPL doc example entries must end with `.`, found `{trimmed}`"
                ));
            };

            let mut output_lines = Vec::new();
            if let Some(binding) = parse_repl_value_binding(expression_source) {
                let mut validation_bindings = value_bindings.clone();
                validation_bindings.push(binding.clone());
                let run_name = repl_generation_run_name(
                    "repl_doc_eval",
                    "Unit",
                    &declarations,
                    &validation_bindings,
                    &module_name,
                );
                run_repl_expression_in_session_with_output(
                    &mut compiler_service,
                    Some(&mut code_server),
                    ReplExpressionRequest {
                        expression: "Unit",
                        declarations: &declarations,
                        value_bindings: &validation_bindings,
                        module_name: &module_name,
                        run_name: &run_name,
                        temp_dir: &temp_dir,
                        diagnostic_format,
                        native_policy,
                        target_profile,
                    },
                    &mut |value| output_lines.push(value.to_string()),
                )?;
                value_bindings.push(binding);
                output_lines.push("Unit".to_string());
                outputs.push(output_lines);
                continue;
            }

            match parse_expr_as_syntax_output(expression_source) {
                Ok(_expr) => {
                    let mutable_receiver =
                        mutable_receiver_binding_name(expression_source, &value_bindings);
                    let expression_to_run = if let Some(receiver) = mutable_receiver.as_deref() {
                        format!("{expression_source}; {receiver}")
                    } else {
                        expression_source.to_string()
                    };
                    let run_name = repl_generation_run_name(
                        "repl_doc_eval",
                        &expression_to_run,
                        &declarations,
                        &value_bindings,
                        &module_name,
                    );
                    let value = run_repl_expression_in_session_with_output(
                        &mut compiler_service,
                        Some(&mut code_server),
                        ReplExpressionRequest {
                            expression: &expression_to_run,
                            declarations: &declarations,
                            value_bindings: &value_bindings,
                            module_name: &module_name,
                            run_name: &run_name,
                            temp_dir: &temp_dir,
                            diagnostic_format,
                            native_policy,
                            target_profile,
                        },
                        &mut |value| output_lines.push(value.to_string()),
                    )?;
                    if let Some(receiver) = mutable_receiver {
                        update_repl_value_binding(&mut value_bindings, &receiver, value);
                        output_lines.push("Unit".to_string());
                    } else {
                        output_lines.push(value);
                    }
                    outputs.push(output_lines);
                }
                Err(EbnfCompileError::Parse(expr_message, _expr_span))
                    if expression_parse_error_blocks_declaration_fallback(&expr_message) =>
                {
                    return Err(format!("REPL doc expression parse error: {expr_message}"));
                }
                Err(_expr_error) => {
                    let mut next_declarations = parse_repl_declaration(&module_name, trimmed)
                        .map_err(|(message, _, _)| {
                            format!("REPL doc declaration parse error: {message}")
                        })?;
                    declarations.append(&mut next_declarations);
                    output_lines.push("Unit".to_string());
                    outputs.push(output_lines);
                }
            }
        }
        Ok((outputs, code_server.event_snapshots()))
    })();

    if let Err(err) = fs::remove_dir_all(&temp_dir) {
        return Err(format!("failed to clean REPL doc temp directory: {err}"));
    }
    result
}

/// Creates a unique temporary workspace for generated REPL modules.
///
/// Inputs:
/// - `prefix`: readable prefix for the generated module and directory names.
///
/// Output:
/// - Generated module name and created temporary directory path.
///
/// Transformation:
/// - Hashes process and clock state into a source-safe module suffix, creates
///   the workspace under the OS temporary directory, and returns both handles.
fn repl_generated_workspace(prefix: &str) -> Result<(String, PathBuf), String> {
    let mut hasher = DefaultHasher::new();
    hasher.write_usize(std::process::id() as usize);
    hasher.write(
        &std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
            .to_le_bytes(),
    );
    let session_hash = hasher.finish();
    let module_name = format!("{}_{}", prefix, session_hash % 1_000_000_000_000_000_000);
    let temp_dir = std::env::temp_dir().join(format!("terlan_{}", module_name));
    fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create REPL temp directory: {err}"))?;
    Ok((module_name, temp_dir))
}

/// Compiles and executes one REPL expression.
///
/// Inputs:
/// - `expression`: Terlan expression source entered by the user.
/// - `declarations`: accumulated session declarations.
/// - `value_bindings`: persistent REPL value bindings entered earlier.
/// - `module_name`: generated REPL module name.
/// - `run_name`: generated function name for this expression.
/// - `temp_dir`: session temporary output directory.
/// - `diagnostic_format`: output format for diagnostics.
/// - `native_policy`: native-code policy enforced during compilation.
/// - `target_profile`: target-profile gate enforced during compilation.
///
/// Output:
/// - Rendered Terlan value text or an error message.
///
/// Transformation:
/// - Builds a synthetic module, runs it through compiler phases, AOT-compiles
///   the complete expression into a native image and rejects any residual
///   interpreter-owned entry.
///   Console output effects are routed through text output or structured REPL
///   events according to the selected diagnostic format.
#[derive(Clone, Copy)]
pub(super) struct ReplExpressionRequest<'a> {
    pub(super) expression: &'a str,
    pub(super) declarations: &'a [String],
    pub(super) value_bindings: &'a [ReplValueBinding],
    pub(super) module_name: &'a str,
    pub(super) run_name: &'a str,
    pub(super) temp_dir: &'a Path,
    pub(super) diagnostic_format: DiagnosticFormat,
    pub(super) native_policy: NativePolicy,
    pub(super) target_profile: crate::validation::target_profile::TargetProfile,
}

pub(super) fn run_repl_expression(
    compiler_service: &mut ReplCompilerService,
    code_server: &mut VmCodeServer,
    request: ReplExpressionRequest<'_>,
) -> Result<String, String> {
    let diagnostic_format = request.diagnostic_format;
    let mut output = |value: &str| match diagnostic_format {
        DiagnosticFormat::Text { .. } => println!("{value}"),
        DiagnosticFormat::Json => emit_repl_event(
            DiagnosticFormat::Json,
            "stdout",
            &[
                repl_json_field("stream", "stdout"),
                repl_json_field("value", value),
            ],
            value,
        ),
    };
    run_repl_expression_in_session_with_output(
        compiler_service,
        Some(code_server),
        request,
        &mut output,
    )
}

#[derive(Debug)]
/// One source generation installed in the persistent REPL execution shard.
pub(super) struct ActiveReplGeneration {
    /// Digest of the complete generated source used for unchanged reuse.
    pub(super) key: String,
    /// Qualified Terlan module containing the active entry.
    pub(super) module: String,
    /// Zero-argument function exported for the active prompt.
    pub(super) run_name: String,
    /// Compiler-emitted source and continuation records for debugger entry.
    pub(super) source_records: Vec<crate::runtime::native_image::debug::TvmNativeDebugRecord>,
    /// Supervised shard owning the admitted native application image.
    pub(super) shard: crate::runtime::vm::pure_native::PureNativeExecutionShard,
}

impl ActiveReplGeneration {
    /// Executes the generation's zero-argument entry on its admitted shard.
    fn execute(&mut self, output: &mut dyn FnMut(&str)) -> Result<String, String> {
        use crate::runtime::native_image::control::TvmTransitionOperation;
        use crate::runtime::vm::package_native_helper::dispatch_vm_capability;
        use crate::runtime::vm::pure_native::PureNativeExecution;
        use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

        let (owner, mut execution) = self
            .shard
            .begin_call(&format!("{}.{}", self.module, self.run_name), &[])?;
        loop {
            execution = match execution {
                PureNativeExecution::Complete(value) => {
                    self.shard.finish_completed_call(owner)?;
                    return Ok(value.render());
                }
                PureNativeExecution::HttpResponse(_) => {
                    return Err(cancel_repl_generation_call(
                        &mut self.shard,
                        owner,
                        "REPL expression returned an HTTP response",
                    ));
                }
                PureNativeExecution::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Capability =>
                {
                    let wait = match self.shard.begin_capability_call(owner, &suspension) {
                        Ok(wait) => wait,
                        Err(error) => {
                            return Err(cancel_repl_generation_call(&mut self.shard, owner, &error))
                        }
                    };
                    let reply = if wait.request().operation == "std.io.console.println" {
                        match wait.request().arguments.as_slice() {
                            [NativeBoundaryTerm::Text(value)] => {
                                output(value);
                                NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
                            }
                            arguments => {
                                let error = format!(
                                    "error[repl.console_arguments]: println expected one text argument, received {}",
                                    arguments.len()
                                );
                                return Err(cancel_repl_generation_call(
                                    &mut self.shard,
                                    owner,
                                    &error,
                                ));
                            }
                        }
                    } else {
                        match dispatch_vm_capability(wait.request()) {
                            Ok(reply) => reply,
                            Err(error) => {
                                return Err(cancel_repl_generation_call(
                                    &mut self.shard,
                                    owner,
                                    &error,
                                ))
                            }
                        }
                    };
                    self.shard
                        .resume_capability_call(owner, *suspension, wait, reply)?
                }
                PureNativeExecution::Suspended(suspension) => {
                    self.shard.resume_call(owner, *suspension)?
                }
            };
        }
    }
    /// Returns the number of completed calls in the active native generation.
    #[cfg(test)]
    pub(super) fn completed_native_call_count(&self) -> u64 {
        self.shard.completed_call_count()
    }
}

/// Cancels a failed REPL generation call without hiding cleanup failures.
fn cancel_repl_generation_call(
    shard: &mut crate::runtime::vm::pure_native::PureNativeExecutionShard,
    owner: crate::runtime::vm::process::VmProcessId,
    error: &str,
) -> String {
    match shard.cancel_call(owner, error) {
        Ok(()) => error.to_string(),
        Err(cleanup) => format!("{error}; error[execution_shard.cleanup]: {cleanup}"),
    }
}

/// Persistent compiler/runtime state for one REPL session.
#[derive(Debug, Default)]
pub(super) struct ReplCompilerService {
    /// Currently admitted source generation, absent before the first prompt.
    pub(super) active: Option<ActiveReplGeneration>,
    /// Whether prompt expressions execute through the VM debugger controller.
    pub(super) debug_enabled: bool,
}

impl ReplCompilerService {
    /// Enables or disables debugger execution for subsequent prompt entries.
    pub(super) fn set_debug_enabled(&mut self, enabled: bool) {
        self.debug_enabled = enabled;
    }

    /// Returns whether subsequent prompt entries use debugger execution.
    pub(super) fn debug_enabled(&self) -> bool {
        self.debug_enabled
    }

    /// Returns whether a compiled generation is available for debugger attach.
    pub(super) fn has_active_generation(&self) -> bool {
        self.active.is_some()
    }

    /// Enters the shared debugger command loop on the active AOT generation.
    pub(super) fn enter_debugger(
        &mut self,
        json_events: bool,
    ) -> Result<Option<String>, crate::commands::debug::DebugCliError> {
        let active = self.active.as_mut().ok_or_else(|| {
            crate::commands::debug::DebugCliError::from(
                "error[repl.debug_generation]: evaluate an expression before attaching the debugger",
            )
        })?;
        crate::commands::debug::execute_repl_interactive_debug(
            &mut active.shard,
            &active.source_records,
            &active.module,
            &active.run_name,
            json_events,
        )
    }
}

/// Compiles or reuses one session generation and executes its native export.
///
/// The complete generated source is the reuse key. A changed key compiles and
/// replaces the shard image; an unchanged key executes the already admitted
/// export without retaining executable CoreIR.
pub(super) fn run_repl_expression_in_session_with_output(
    compiler_service: &mut ReplCompilerService,
    mut code_server: Option<&mut VmCodeServer>,
    request: ReplExpressionRequest<'_>,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let ReplExpressionRequest {
        expression,
        declarations,
        value_bindings,
        module_name,
        run_name,
        temp_dir,
        diagnostic_format,
        native_policy,
        target_profile,
    } = request;
    let source = repl_expression_module_source(
        expression,
        declarations,
        value_bindings,
        module_name,
        run_name,
    );
    let key = Sha256::digest(source.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if compiler_service
        .active
        .as_ref()
        .is_some_and(|active| active.key == key)
    {
        return Ok(execute_active_repl_generation(compiler_service, output)?);
    }

    let source_path = temp_dir.join(format!("{}.terl", module_name));
    if let Err(err) = fs::write(&source_path, source.as_bytes()) {
        return Err(format!("failed to write REPL module: {err}"));
    }

    let source_path_text = source_path.to_string_lossy().into_owned();
    let compile =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            &source_path_text,
            &source,
            diagnostic_format,
            None,
            native_policy,
            target_profile,
        );
    if compile.artifacts.is_none() {
        return Err(repl_compile_error_message(&compile));
    }
    let compiled = compile
        .artifacts
        .expect("compile artifacts checked immediately above");
    let native_image =
        crate::commands::build::vm_artifact::native_image::compile_repl_native_image(
            temp_dir,
            module_name,
            &source_path_text,
            &source,
            &compiled.syntax_output,
            &compiled.core,
        )?;
    if let Some(code_server) = code_server.as_mut() {
        code_server.publish_compiled_source(&source_path_text, &source, &compiled.core);
        code_server.purge_retired_generations(module_name)?;
    }
    let compiled_module = compiled.core.module.clone();
    let native_image = native_image.ok_or_else(|| {
        format!(
            "error[repl.aot_required]: `{compiled_module}.{run_name}/0` did not produce a native image; runtime CoreIR interpretation has been removed"
        )
    })?;
    let debug_bytes =
        fs::read(&native_image).map_err(|error| format!("error[repl.debug_map_read]: {error}"))?;
    let source_records =
        crate::runtime::native_image::debug::inspect_tvm_native_debug(&debug_bytes)
            .map_err(|error| format!("error[repl.debug_map]: {error}"))?;
    let qualified_run_name = format!("{compiled_module}.{run_name}");
    if let Some(active) = compiler_service.active.as_mut() {
        active.shard.replace_image(&native_image)?;
        if !active.shard.has_export(&qualified_run_name, 0) {
            return Err(format!(
                "error[repl.aot_export_missing]: native image does not contain `{qualified_run_name}/0`; runtime CoreIR interpretation has been removed"
            ));
        }
        active.key = key;
        active.module = compiled_module;
        active.run_name = run_name.to_string();
        active.source_records = source_records;
        return Ok(execute_active_repl_generation(compiler_service, output)?);
    }
    let shard =
        crate::runtime::vm::pure_native::PureNativeExecutionShard::load_image(&native_image)?;
    if !shard.has_export(&qualified_run_name, 0) {
        return Err(format!(
            "error[repl.aot_export_missing]: native image does not contain `{qualified_run_name}/0`; runtime CoreIR interpretation has been removed"
        ));
    }
    compiler_service.active = Some(ActiveReplGeneration {
        key,
        module: compiled_module.clone(),
        run_name: run_name.to_string(),
        source_records,
        shard,
    });
    Ok(execute_active_repl_generation(compiler_service, output)?)
}

fn execute_active_repl_generation(
    compiler_service: &mut ReplCompilerService,
    output: &mut dyn FnMut(&str),
) -> Result<String, crate::commands::debug::DebugCliError> {
    let debug_enabled = compiler_service.debug_enabled;
    let active = compiler_service
        .active
        .as_mut()
        .expect("REPL generation must be installed before execution");
    if !debug_enabled {
        return Ok(active.execute(output)?);
    }
    let (result, events) = crate::commands::debug::execute_repl_debug_entry(
        &mut active.shard,
        &active.source_records,
        &active.module,
        &active.run_name,
    )?;
    for event in events {
        output(&format!("debug: {event}"));
    }
    result.ok_or_else(|| {
        "error[repl.debug_result]: debugger execution ended without a value"
            .to_string()
            .into()
    })
}

/// Returns the stable native entry identity for one complete REPL generation.
///
/// Unchanged expressions in unchanged session state receive the same entry
/// name, which keeps the NativeIR fingerprint stable and lets the AOT cache
/// reuse the already linked image. Any expression, declaration, binding, or
/// session-module change receives a different identity.
pub(super) fn repl_generation_run_name(
    prefix: &str,
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
) -> String {
    let source = repl_expression_module_source(
        expression,
        declarations,
        value_bindings,
        module_name,
        "repl_generation_identity",
    );
    let digest = Sha256::digest(source.as_bytes());
    let suffix = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}_{suffix}")
}

/// Builds the complete synthetic source for one REPL expression entry.
///
/// Inputs:
/// - `expression`: Terlan expression source entered by the user.
/// - `declarations`: accumulated session declarations.
/// - `value_bindings`: persistent REPL value bindings entered earlier.
/// - `module_name`: generated REPL module name.
/// - `run_name`: generated function name for this expression.
///
/// Output:
/// - Complete Terlan module source for the REPL generation.
///
/// Transformation:
/// - Reuses the same source shape for execution and hot-reload publication so
///   REPL-generated VM generations cannot drift from evaluated REPL entries.
pub(super) fn repl_expression_module_source(
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
    run_name: &str,
) -> String {
    let mut source = repl_declarations_to_source(module_name, declarations);
    let body = repl_expression_with_bindings(expression, value_bindings);
    source.push_str(&format!("pub {}(): Dynamic ->\n    {}.\n", run_name, body));
    source
}

/// Formats the first compiler diagnostic from a failed REPL compile.
///
/// Inputs:
/// - `compile`: formal compiler result with failed phase diagnostics.
///
/// Output:
/// - Stable `code: message` text for the first error-like diagnostic.
///
/// Transformation:
/// - Walks phase diagnostics in compiler order and returns the first available
///   diagnostic so REPL docs can match expected-error examples.
fn repl_compile_error_message(
    compile: &crate::formal_pipeline::CompileSyntaxModuleThroughPhasesResult,
) -> String {
    for diagnostics in [
        compile.parse_diagnostics.as_slice(),
        compile.macro_expansion_diagnostics.as_slice(),
        compile.include_expansion_diagnostics.as_slice(),
        compile.resolve_diagnostics.as_slice(),
        compile.typecheck_diagnostics.as_slice(),
        compile.core_diagnostics.as_slice(),
    ] {
        if let Some(diagnostic) = diagnostics.iter().find(|diag| diag.severity == "error") {
            return format!("{}: {}", diagnostic.code, diagnostic.message);
        }
        if let Some(diagnostic) = diagnostics.first() {
            return format!("{}: {}", diagnostic.code, diagnostic.message);
        }
    }
    "failed to compile REPL expression".to_string()
}
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::runtime::vm::code_server::{VmCodeServer, VmCodeServerEventSnapshot};
use crate::terlan_syntax::{
    parse_expr_as_syntax_output, parse_module_as_syntax_output, EbnfCompileError,
};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
use crate::DiagnosticFormat;

use super::bindings::{
    mutable_receiver_binding_name, parse_repl_value_binding, repl_expression_with_bindings,
    update_repl_value_binding, ReplValueBinding,
};
use super::event::{emit_repl_event, repl_json_field};
use super::source;
use super::source::{
    expression_parse_error_blocks_declaration_fallback, parse_repl_declaration,
    repl_declarations_to_source,
};
