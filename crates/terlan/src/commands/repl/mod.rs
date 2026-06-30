mod bindings;
mod event;
mod help;
mod source;

use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::UNIX_EPOCH;

use serde_json::json;

use crate::terlan_syntax::{parse_expr_as_syntax_output, EbnfCompileError};

use crate::validation::native_policy::NativePolicy;
use crate::{CliCommand, CliState, DiagnosticFormat};

use bindings::{parse_repl_value_binding, repl_expression_with_bindings, ReplValueBinding};
#[cfg(test)]
use event::render_repl_json_event;
use event::{emit_repl_event, emit_repl_result, repl_json_field};
use help::{is_repl_help_args, print_repl_help};
#[cfg(test)]
use source::repl_load_sources;
use source::{
    load_repl_seed_declarations, parse_repl_declaration, parse_repl_declaration_and_log,
    repl_declarations_to_source,
};

/// Runtime selected for REPL expression execution.
///
/// Inputs:
/// - Parsed from `terlc repl --runtime beam|vm`.
///
/// Output:
/// - Execution mode for generated REPL functions.
///
/// Transformation:
/// - Keeps the stable BEAM-compatible runtime path and the experimental
///   in-process Rust VM path explicit at the command boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplRuntime {
    Beam,
    Vm,
}

/// Parsed REPL command options.
#[derive(Debug)]
struct ReplCommandArgs {
    seed_path: Option<String>,
    runtime: ReplRuntime,
}

/// Parses command-local REPL options.
///
/// Inputs:
/// - `args`: command-local arguments after `repl`.
/// - `experimental`: whether hidden experimental features are enabled.
///
/// Output:
/// - Parsed seed path and runtime selection, or usage error text.
///
/// Transformation:
/// - Accepts one optional seed path and `--runtime beam|vm`; defaults to the
///   stable BEAM-compatible path unless the experimental VM is selected.
fn parse_repl_command_args(args: &[String], experimental: bool) -> Result<ReplCommandArgs, String> {
    let mut seed_path = None;
    let mut runtime = ReplRuntime::Beam;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--runtime" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --runtime".to_string());
                };
                runtime = match value.as_str() {
                    "beam" => ReplRuntime::Beam,
                    "vm" => ReplRuntime::Vm,
                    other => {
                        return Err(format!(
                            "unsupported REPL runtime `{other}`; expected beam or vm"
                        ));
                    }
                };
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return Err(format!("unknown repl option: {arg}"));
            }
            path => {
                if seed_path.is_some() {
                    return Err(
                        "repl accepts at most one <file.terl|project-dir> seed path".to_string()
                    );
                }
                seed_path = Some(path.to_string());
                index += 1;
            }
        }
    }

    if runtime == ReplRuntime::Vm && !experimental {
        return Err(
            "terlc repl --runtime vm is experimental; rerun with --experimental.".to_string(),
        );
    }

    Ok(ReplCommandArgs { seed_path, runtime })
}

/// Executes the `repl` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing optional `--help` or seed file path.
/// - `state`: parsed global CLI state, including diagnostic format and native
///   policy.
///
/// Output:
/// - `ExitCode::SUCCESS` for help output, EOF, or explicit quit.
/// - `ExitCode::from(2)` for malformed command arguments.
/// - `ExitCode::from(1)` for temp-dir, seed-load, input, prompt, cleanup, or
///   compiler/runtime failures that end the session.
///
/// Transformation:
/// - Creates a temporary REPL module, optionally loads seed declarations, then
///   reads interactive commands and expressions until the session exits.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    match cmd.args.as_slice() {
        args if is_repl_help_args(args) => {
            print_repl_help();
            ExitCode::SUCCESS
        }
        args => {
            let parsed = match parse_repl_command_args(args, state.experimental) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    print_repl_help();
                    return ExitCode::from(2);
                }
            };
            if parsed.runtime == ReplRuntime::Vm && !state.experimental {
                eprintln!("terlc repl --runtime vm is experimental; rerun with --experimental.");
                return ExitCode::from(2);
            }

            let seed_path = parsed.seed_path;
            let mut hasher = DefaultHasher::new();
            hasher.write_usize(std::process::id() as usize);
            hasher.write(
                &std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos())
                    .to_le_bytes(),
            );
            let session_hash = hasher.finish();
            let module_name = format!("repl_{}", session_hash % 1_000_000_000_000_000_000);
            let temp_dir = std::env::temp_dir().join(format!("terlan_repl_{}", module_name));
            if let Err(err) = fs::create_dir_all(&temp_dir) {
                eprintln!("failed to create REPL temp directory: {err}");
                return ExitCode::from(1);
            }

            let mut baseline_declarations = Vec::new();
            if let Some(path) = seed_path.as_deref() {
                match load_repl_seed_declarations(path, state.diagnostic_format) {
                    Ok(declarations) => baseline_declarations = declarations,
                    Err(exit_code) => {
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                        }
                        return exit_code;
                    }
                }
            }
            let mut declarations = baseline_declarations.clone();
            let mut value_bindings = Vec::new();
            let mut eval_counter = 0usize;
            emit_repl_event(state.diagnostic_format, "ready", &[], "REPL ready");

            let stdin = std::io::stdin();
            let mut stdout = std::io::stdout();
            let mut line = String::new();
            if !matches!(state.diagnostic_format, DiagnosticFormat::Json) {
                println!("terlc repl (type :help for commands, :quit to exit)");
            }
            loop {
                if !matches!(state.diagnostic_format, DiagnosticFormat::Json) {
                    print!("repl> ");
                    if let Err(error) = stdout.flush() {
                        eprintln!("failed to flush REPL prompt: {error}");
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                        }
                        return ExitCode::from(1);
                    }
                }

                line.clear();
                match stdin.read_line(&mut line) {
                    Ok(0) => {
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                        }
                        return ExitCode::SUCCESS;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("failed to read REPL input: {error}");
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                        }
                        return ExitCode::from(1);
                    }
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match trimmed {
                    ":quit" => {
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                            return ExitCode::from(1);
                        }
                        emit_repl_event(
                            state.diagnostic_format,
                            "ready",
                            &[repl_json_field("status", "ready_to_exit")],
                            "REPL exiting",
                        );
                        return ExitCode::SUCCESS;
                    }
                    ":help" => {
                        if matches!(state.diagnostic_format, DiagnosticFormat::Json) {
                            emit_repl_event(
                                state.diagnostic_format,
                                "status",
                                &[
                                    repl_json_field(
                                        "message",
                                        "REPL supports expression evaluation and session declarations.",
                                    ),
                                    repl_json_field(
                                        "commands",
                                        json!([":help", ":quit", ":reset", ":load"]),
                                    ),
                                ],
                                "help",
                            );
                        } else {
                            println!("REPL supports Terlan entries terminated with '.'.");
                            println!(":help, :quit, :reset, :load <file.terl|project-dir>");
                        }
                    }
                    ":reset" => {
                        baseline_declarations.clear();
                        declarations.clear();
                        value_bindings.clear();
                        emit_repl_result(state.diagnostic_format, "Unit");
                    }
                    command if command.starts_with(":load") => {
                        let explicit_path = command.strip_prefix(":load").unwrap_or("").trim();
                        let path = match explicit_path {
                            "" => {
                                emit_repl_event(
                                    state.diagnostic_format,
                                    "error",
                                    &[repl_json_field(
                                        "message",
                                        ":load requires a path: :load <file.terl|project-dir>",
                                    )],
                                    ":load requires a path: :load <file.terl|project-dir>",
                                );
                                continue;
                            }
                            path => path.to_string(),
                        };

                        match load_repl_seed_declarations(&path, state.diagnostic_format) {
                            Ok(next_declarations) => {
                                baseline_declarations = next_declarations.clone();
                                declarations = next_declarations;
                                value_bindings.clear();
                                emit_repl_result(state.diagnostic_format, "Unit");
                            }
                            Err(_code) => {}
                        }
                    }
                    command if command.starts_with(':') => {
                        emit_repl_event(
                            state.diagnostic_format,
                            "error",
                            &[repl_json_field(
                                "message",
                                format!("unknown REPL command: {command}"),
                            )],
                            &format!("unknown REPL command: {command}"),
                        );
                    }
                    _ => match repl_expression_source(trimmed) {
                        Some(expression_source) => {
                            if let Some(binding) = parse_repl_value_binding(expression_source) {
                                eval_counter += 1;
                                let run_name = format!("repl_eval_{}", eval_counter);
                                let mut validation_bindings = value_bindings.clone();
                                validation_bindings.push(binding.clone());
                                match run_repl_expression(
                                    "Unit",
                                    &declarations,
                                    &validation_bindings,
                                    &module_name,
                                    &run_name,
                                    &temp_dir,
                                    state.diagnostic_format,
                                    state.native_policy,
                                    state.target_profile,
                                    parsed.runtime,
                                ) {
                                    Ok(_value) => {
                                        value_bindings.push(binding);
                                        emit_repl_result(state.diagnostic_format, "Unit");
                                    }
                                    Err(message) => emit_repl_event(
                                        state.diagnostic_format,
                                        "error",
                                        &[repl_json_field("message", message.as_str())],
                                        &message,
                                    ),
                                }
                                continue;
                            }

                            match parse_expr_as_syntax_output(expression_source) {
                                Ok(_expr) => {
                                    eval_counter += 1;
                                    let run_name = format!("repl_eval_{}", eval_counter);
                                    match run_repl_expression(
                                        expression_source,
                                        &declarations,
                                        &value_bindings,
                                        &module_name,
                                        &run_name,
                                        &temp_dir,
                                        state.diagnostic_format,
                                        state.native_policy,
                                        state.target_profile,
                                        parsed.runtime,
                                    ) {
                                        Ok(value) => {
                                            emit_repl_result(state.diagnostic_format, &value)
                                        }
                                        Err(message) => emit_repl_event(
                                            state.diagnostic_format,
                                            "error",
                                            &[repl_json_field("message", message.as_str())],
                                            &message,
                                        ),
                                    }
                                }
                                Err(EbnfCompileError::Parse(expr_message, expr_span)) => {
                                    parse_repl_declaration_and_log(
                                        &module_name,
                                        trimmed,
                                        state.diagnostic_format,
                                        &mut declarations,
                                        Some((&expr_message, expr_span.start, expr_span.end)),
                                    );
                                }
                                Err(EbnfCompileError::Serialize(message)) => {
                                    parse_repl_declaration_and_log(
                                        &module_name,
                                        trimmed,
                                        state.diagnostic_format,
                                        &mut declarations,
                                        Some((
                                            &format!("parse serialization error: {message}"),
                                            0,
                                            0,
                                        )),
                                    );
                                }
                            }
                        }
                        None => emit_repl_event(
                            state.diagnostic_format,
                            "error",
                            &[repl_json_field("message", "REPL entries must end with '.'")],
                            "REPL entries must end with '.'",
                        ),
                    },
                }
            }
        }
    }
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
fn repl_expression_source(entry: &str) -> Option<&str> {
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
///   binding, expression, CoreIR lowering, and evaluator path as interactive
///   `terlc repl`, while capturing output instead of printing it.
pub(crate) fn evaluate_repl_prompt_inputs(
    inputs: &[String],
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
) -> Result<Vec<Vec<String>>, String> {
    let (module_name, temp_dir) = repl_generated_workspace("repl_doc")?;
    let mut declarations = Vec::new();
    let mut value_bindings = Vec::new();
    let mut eval_counter = 0usize;
    let mut outputs = Vec::new();

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
                eval_counter += 1;
                let run_name = format!("repl_doc_eval_{}", eval_counter);
                let mut validation_bindings = value_bindings.clone();
                validation_bindings.push(binding.clone());
                run_repl_expression_with_output(
                    "Unit",
                    &declarations,
                    &validation_bindings,
                    &module_name,
                    &run_name,
                    &temp_dir,
                    diagnostic_format,
                    native_policy,
                    target_profile,
                    ReplRuntime::Vm,
                    &mut |value| output_lines.push(value.to_string()),
                )?;
                value_bindings.push(binding);
                output_lines.push("Unit".to_string());
                outputs.push(output_lines);
                continue;
            }

            match parse_expr_as_syntax_output(expression_source) {
                Ok(_expr) => {
                    eval_counter += 1;
                    let run_name = format!("repl_doc_eval_{}", eval_counter);
                    let value = run_repl_expression_with_output(
                        expression_source,
                        &declarations,
                        &value_bindings,
                        &module_name,
                        &run_name,
                        &temp_dir,
                        diagnostic_format,
                        native_policy,
                        target_profile,
                        ReplRuntime::Vm,
                        &mut |value| output_lines.push(value.to_string()),
                    )?;
                    output_lines.push(value);
                    outputs.push(output_lines);
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
        Ok(outputs)
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
/// - Builds a synthetic module, runs it through compiler phases, then evaluates
///   the generated function through the compiler-owned CoreIR evaluator without
///   invoking a target runtime. Console output effects are routed through text
///   output or structured REPL events according to the selected diagnostic
///   format.
fn run_repl_expression(
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
    run_name: &str,
    temp_dir: &Path,
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
    runtime: ReplRuntime,
) -> Result<String, String> {
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
    run_repl_expression_with_output(
        expression,
        declarations,
        value_bindings,
        module_name,
        run_name,
        temp_dir,
        diagnostic_format,
        native_policy,
        target_profile,
        runtime,
        &mut output,
    )
}

/// Compiles and executes one REPL expression with captured output.
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
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Rendered Terlan value text or an error message.
///
/// Transformation:
/// - Builds a synthetic module, compiles it through formal phases, loads the
///   resulting CoreIR into the Rust VM, and executes the generated entrypoint
///   while routing console effects through `output`.
fn run_repl_expression_with_output(
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
    run_name: &str,
    temp_dir: &Path,
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
    runtime: ReplRuntime,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let mut source = repl_declarations_to_source(module_name, declarations);
    let body = repl_expression_with_bindings(expression, value_bindings);
    source.push_str(&format!("pub {}(): Dynamic ->\n    {}.\n", run_name, body));

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

    match runtime {
        ReplRuntime::Vm => run_compiled_repl_expression_in_vm(compiled.core, run_name, output),
        ReplRuntime::Beam => run_compiled_repl_expression_on_beam(
            &source_path_text,
            temp_dir,
            &compiled,
            run_name,
            output,
        ),
    }
}

/// Executes a compiled REPL expression through the in-process Rust VM.
///
/// Inputs:
/// - `core`: checked CoreIR module.
/// - `run_name`: generated zero-arity function to execute.
/// - `output`: callback for console output effects.
///
/// Output:
/// - Rendered result text or VM error.
///
/// Transformation:
/// - Loads the module into `TerlanVm` and executes the generated entrypoint.
fn run_compiled_repl_expression_in_vm(
    core: crate::terlan_typeck::CoreModule,
    run_name: &str,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let module_name = core.module.clone();
    let mut vm = crate::runtime::vm::TerlanVm::new();
    vm.load_module(core);
    vm.execute_zero_arity(&module_name, run_name, output)
        .map(|value| value.render())
}

/// Executes a compiled REPL expression through the regular BEAM path.
///
/// Inputs:
/// - `source_path_text`: generated REPL source path.
/// - `temp_dir`: workspace receiving Erlang/BEAM artifacts.
/// - `compiled`: checked compiler artifacts.
/// - `run_name`: generated zero-arity function to execute.
/// - `output`: callback for console output lines.
///
/// Output:
/// - Rendered return value text or BEAM compile/run error.
///
/// Transformation:
/// - Emits Erlang source from CoreIR, compiles it with `erlc`, runs the selected
///   function with `erl`, captures stdout, and splits user output from the
///   sentinel-delimited return value.
fn run_compiled_repl_expression_on_beam(
    source_path_text: &str,
    temp_dir: &Path,
    compiled: &crate::formal_pipeline::CheckedSyntaxModuleArtifacts,
    run_name: &str,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let code = emit_repl_erlang_source(source_path_text, compiled)?;
    let module_atom = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let erl_path = temp_dir.join(format!("{module_atom}.erl"));
    fs::write(&erl_path, code)
        .map_err(|err| format!("failed to write REPL Erlang source: {err}"))?;
    let erlc_output = Command::new("erlc")
        .arg("-o")
        .arg(temp_dir)
        .arg(&erl_path)
        .output()
        .map_err(|err| format!("failed to run erlc for REPL BEAM runtime: {err}"))?;
    if !erlc_output.status.success() {
        return Err(format!(
            "erlc failed for REPL BEAM runtime: {}",
            String::from_utf8_lossy(&erlc_output.stderr).trim()
        ));
    }

    let marker = "__TERLAN_REPL_RESULT__";
    let eval = format!(
        "Render = fun(unit) -> \"Unit\"; (true) -> \"true\"; (false) -> \"false\"; (Value) when is_integer(Value) -> integer_to_list(Value); (Value) when is_binary(Value) -> binary_to_list(Value); (Value) -> lists:flatten(io_lib:format(\"~tp\", [Value])) end, Result = '{}':'{}'(), io:format(\"~n{}~ts~n\", [Render(Result)]), halt(0).",
        module_atom, run_name, marker
    );
    let beam_output = Command::new("erl")
        .arg("-noshell")
        .arg("-pa")
        .arg(temp_dir)
        .arg("-eval")
        .arg(eval)
        .output()
        .map_err(|err| format!("failed to run erl for REPL BEAM runtime: {err}"))?;
    if !beam_output.status.success() {
        return Err(format!(
            "erl failed for REPL BEAM runtime: {}",
            String::from_utf8_lossy(&beam_output.stderr).trim()
        ));
    }
    split_beam_repl_output(
        &String::from_utf8_lossy(&beam_output.stdout),
        marker,
        output,
    )
}

/// Emits Erlang source for a generated REPL module.
fn emit_repl_erlang_source(
    source_path_text: &str,
    compiled: &crate::formal_pipeline::CheckedSyntaxModuleArtifacts,
) -> Result<String, String> {
    let file_imports = crate::commands::artifacts::collect_syntax_file_import_bytes(
        &compiled.syntax_output,
        Path::new(source_path_text),
    )?;
    let templates = crate::commands::artifacts::collect_syntax_template_inputs(
        &compiled.syntax_output,
        Path::new(source_path_text),
    )?;
    let markdown_imports = crate::commands::artifacts::collect_syntax_markdown_inputs(
        &compiled.syntax_output,
        Path::new(source_path_text),
    )?;
    let code = crate::terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &compiled
            .interfaces
            .iter()
            .map(|(name, interface)| (name.clone(), interface.clone()))
            .collect::<BTreeMap<_, _>>(),
        &file_imports,
        &templates,
        &markdown_imports,
    )?;
    Ok(code)
}

/// Splits BEAM REPL stdout into user output lines and return value text.
fn split_beam_repl_output(
    stdout: &str,
    marker: &str,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let Some((before, after)) = stdout.split_once(marker) else {
        return Err("REPL BEAM runtime did not emit result marker".to_string());
    };
    for line in before.lines().filter(|line| !line.is_empty()) {
        output(line);
    }
    let result = after
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "REPL BEAM runtime did not emit a result value".to_string())?;
    Ok(result.trim().to_string())
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

#[cfg(test)]
#[path = "repl_test.rs"]
mod repl_test;
