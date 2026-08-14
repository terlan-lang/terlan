use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::time::UNIX_EPOCH;

use crossterm::cursor::MoveToColumn;
use crossterm::event::{self as terminal_event, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use serde_json::json;

use crate::commands::terminal::RawModeGuard;
use crate::terlan_syntax::{parse_expr_as_syntax_output, EbnfCompileError};

use crate::runtime::vm::code_server::VmCodeServer;
use crate::validation::target_profile::TargetProfile;
use crate::{CliCommand, CliState, DiagnosticFormat};

use super::bindings::{
    mutable_receiver_binding_name, parse_repl_value_binding, update_repl_value_binding,
};
use super::evaluation::{
    repl_expression_source, repl_generation_run_name, run_repl_expression,
    validate_repl_seed_target_evidence, ReplCompilerService, ReplExpressionRequest,
};
use super::event;
use super::event::{emit_repl_event, emit_repl_result, repl_json_field};
use super::help::{is_repl_help_args, print_repl_help};
use super::source::{load_repl_seed_declarations, parse_repl_declaration_and_log};

/// Parsed REPL command options.
#[derive(Debug)]
pub(super) struct ReplCommandArgs {
    /// Optional seed module or project loaded before the first prompt.
    pub(super) seed_path: Option<String>,
    /// Whether prompt expressions should execute through the VM debugger.
    pub(super) debug: bool,
}

/// Parses command-local REPL options.
///
/// Inputs:
/// - `args`: command-local arguments after `repl`.
/// - `experimental`: whether hidden experimental features are enabled.
///
/// Output:
/// - Parsed seed path and debug selection, or usage error text.
///
/// Transformation:
/// - Accepts one optional seed path and the debugger switch. Runtime
///   selection is intentionally absent because the REPL always executes an
///   admitted Terlan native image.
pub(super) fn parse_repl_command_args(args: &[String]) -> Result<ReplCommandArgs, String> {
    let mut seed_path = None;
    let mut debug = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--debug" => {
                debug = true;
                index += 1;
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

    Ok(ReplCommandArgs { seed_path, debug })
}

/// Returns whether an interactive REPL command requests the debugger surface.
///
/// Inputs:
/// - `command`: trimmed interactive command line beginning with `:`.
///
/// Output:
/// - `true` only for the exact `:debug` entry point.
///
/// Transformation:
/// - Keeps the debugger command explicit so arguments do not parse as source.
pub(super) fn is_repl_debug_command(command: &str) -> bool {
    command == ":debug"
}

/// Returns structured fields for a REPL debugger mode event.
pub(super) fn repl_debug_fields(enabled: bool) -> Vec<event::ReplJsonField> {
    vec![
        repl_json_field("command", "debug"),
        repl_json_field("enabled", enabled),
        repl_json_field("implemented", true),
    ]
}

fn emit_repl_debug_mode(diagnostic_format: DiagnosticFormat, enabled: bool) {
    emit_repl_event(
        diagnostic_format,
        "status",
        &repl_debug_fields(enabled),
        if enabled {
            "VM debugger enabled"
        } else {
            "VM debugger disabled"
        },
    );
}

/// Reads one interactive REPL line with in-memory arrow-key history.
///
/// Inputs:
/// - `prompt`: visible prompt text.
/// - `history`: session-local prompt history.
///
/// Output:
/// - `Ok(Some(line))` when the user presses Enter.
/// - `Ok(None)` on EOF or interrupt.
/// - `Err` when terminal raw mode or event reading fails.
///
/// Transformation:
/// - Uses Crossterm raw keyboard events to support left/right editing,
///   backspace/delete, Home/End, and up/down history traversal without
///   changing the compiler-facing REPL compilation service.
fn read_interactive_repl_line(
    prompt: &str,
    history: &mut Vec<String>,
) -> Result<Option<String>, String> {
    let _raw_mode = RawModeGuard::enable()
        .map_err(|error| format!("failed to enable REPL raw mode: {error}"))?;
    let mut stdout = std::io::stdout();
    print!("{prompt}");
    stdout
        .flush()
        .map_err(|error| format!("failed to flush REPL prompt: {error}"))?;

    let mut buffer = String::new();
    let mut cursor = 0usize;
    let mut history_index = history.len();
    let mut pending_entry = String::new();

    loop {
        match terminal_event::read()
            .map_err(|error| format!("failed to read REPL input: {error}"))?
        {
            Event::Key(key) => match key.code {
                KeyCode::Enter => {
                    print!("{}", interactive_repl_line_break());
                    let entry = buffer.clone();
                    if !entry.trim().is_empty() {
                        history.push(entry.clone());
                    }
                    return Ok(Some(entry));
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    print!("{}", interactive_repl_line_break());
                    return Ok(None);
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if buffer.is_empty() {
                        print!("{}", interactive_repl_line_break());
                        return Ok(None);
                    }
                }
                KeyCode::Char(ch) => {
                    buffer.insert(cursor, ch);
                    cursor += ch.len_utf8();
                    redraw_repl_line(prompt, &buffer, cursor)?;
                }
                KeyCode::Backspace => {
                    if cursor > 0 {
                        let previous = buffer[..cursor]
                            .char_indices()
                            .last()
                            .map(|(index, _)| index)
                            .unwrap_or(0);
                        buffer.drain(previous..cursor);
                        cursor = previous;
                        redraw_repl_line(prompt, &buffer, cursor)?;
                    }
                }
                KeyCode::Delete => {
                    if cursor < buffer.len() {
                        let next = buffer[cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(index, _)| cursor + index)
                            .unwrap_or(buffer.len());
                        buffer.drain(cursor..next);
                        redraw_repl_line(prompt, &buffer, cursor)?;
                    }
                }
                KeyCode::Left => {
                    if cursor > 0 {
                        cursor = buffer[..cursor]
                            .char_indices()
                            .last()
                            .map(|(index, _)| index)
                            .unwrap_or(0);
                        redraw_repl_line(prompt, &buffer, cursor)?;
                    }
                }
                KeyCode::Right => {
                    if cursor < buffer.len() {
                        cursor = buffer[cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(index, _)| cursor + index)
                            .unwrap_or(buffer.len());
                        redraw_repl_line(prompt, &buffer, cursor)?;
                    }
                }
                KeyCode::Home => {
                    cursor = 0;
                    redraw_repl_line(prompt, &buffer, cursor)?;
                }
                KeyCode::End => {
                    cursor = buffer.len();
                    redraw_repl_line(prompt, &buffer, cursor)?;
                }
                KeyCode::Up => {
                    if history_index == history.len() {
                        pending_entry = buffer.clone();
                    }
                    if history_index > 0 {
                        history_index -= 1;
                        buffer = history[history_index].clone();
                        cursor = buffer.len();
                        redraw_repl_line(prompt, &buffer, cursor)?;
                    }
                }
                KeyCode::Down if history_index < history.len() => {
                    history_index += 1;
                    buffer = if history_index == history.len() {
                        pending_entry.clone()
                    } else {
                        history[history_index].clone()
                    };
                    cursor = buffer.len();
                    redraw_repl_line(prompt, &buffer, cursor)?;
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                redraw_repl_line(prompt, &buffer, cursor)?;
            }
            _ => {}
        }
    }
}

/// Returns the line break used while the terminal is in raw mode.
pub(super) fn interactive_repl_line_break() -> &'static str {
    "\r\n"
}

/// Redraws the current interactive REPL prompt buffer.
///
/// Inputs:
/// - `prompt`: visible prompt prefix.
/// - `buffer`: current line content.
/// - `cursor`: byte offset into `buffer`.
///
/// Output:
/// - Terminal is redrawn or an error is returned.
///
/// Transformation:
/// - Clears the current terminal row, prints the prompt and buffer, then moves
///   the cursor back to the source-facing edit position.
fn redraw_repl_line(prompt: &str, buffer: &str, cursor: usize) -> Result<(), String> {
    let mut stdout = std::io::stdout();
    stdout
        .execute(MoveToColumn(0))
        .map_err(|error| format!("failed to redraw REPL line: {error}"))?;
    stdout
        .execute(Clear(ClearType::CurrentLine))
        .map_err(|error| format!("failed to redraw REPL line: {error}"))?;
    print!("{prompt}{buffer}");
    let prompt_width = prompt.chars().count();
    let cursor_width = buffer[..cursor].chars().count();
    stdout
        .execute(MoveToColumn((prompt_width + cursor_width) as u16))
        .map_err(|error| format!("failed to redraw REPL line: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("failed to redraw REPL line: {error}"))
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
    let stdin = std::io::stdin();
    let input_is_terminal = stdin.is_terminal();
    run_with_input(cmd, state, &mut stdin.lock(), input_is_terminal)
}

/// Executes the REPL against an explicit input stream.
///
/// Keeping input ownership at this boundary prevents CLI tests and embedding
/// callers from depending on ambient process stdin. Production still passes a
/// locked stdin stream and separately reports terminal capability so line
/// editing remains a VM-owned CLI concern.
pub(crate) fn run_with_input(
    cmd: CliCommand,
    state: CliState,
    input: &mut dyn BufRead,
    input_is_terminal: bool,
) -> ExitCode {
    match cmd.args.as_slice() {
        args if is_repl_help_args(args) => {
            print_repl_help();
            ExitCode::SUCCESS
        }
        args => {
            let parsed = match parse_repl_command_args(args) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    print_repl_help();
                    return ExitCode::from(2);
                }
            };
            let seed_path = parsed.seed_path;
            if let Err(message) =
                validate_repl_seed_target_evidence(seed_path.as_deref(), state.target_profile)
            {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
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
            let mut code_server = VmCodeServer::default();
            let mut compiler_service = ReplCompilerService::default();
            compiler_service.set_debug_enabled(parsed.debug);
            let repl_target_profile = TargetProfile::Vm;
            emit_repl_event(state.diagnostic_format, "ready", &[], "REPL ready");
            if parsed.debug {
                emit_repl_debug_mode(state.diagnostic_format, true);
            }

            let mut stdout = std::io::stdout();
            let interactive_line_editing =
                !matches!(state.diagnostic_format, DiagnosticFormat::Json)
                    && input_is_terminal
                    && stdout.is_terminal();
            let mut line_history = Vec::new();
            let mut line = String::new();
            if !matches!(state.diagnostic_format, DiagnosticFormat::Json) {
                println!("terlc repl (type :help for commands, :quit to exit)");
            }
            loop {
                let input_line = if interactive_line_editing {
                    match read_interactive_repl_line("repl> ", &mut line_history) {
                        Ok(Some(line)) => line,
                        Ok(None) => {
                            if let Err(err) = fs::remove_dir_all(&temp_dir) {
                                eprintln!("failed to clean REPL temp directory: {err}");
                            }
                            return ExitCode::SUCCESS;
                        }
                        Err(error) => {
                            eprintln!("{error}");
                            if let Err(err) = fs::remove_dir_all(&temp_dir) {
                                eprintln!("failed to clean REPL temp directory: {err}");
                            }
                            return ExitCode::from(1);
                        }
                    }
                } else {
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
                    match input.read_line(&mut line) {
                        Ok(0) => {
                            if let Err(err) = fs::remove_dir_all(&temp_dir) {
                                eprintln!("failed to clean REPL temp directory: {err}");
                            }
                            return ExitCode::SUCCESS;
                        }
                        Ok(_) => line.clone(),
                        Err(error) => {
                            eprintln!("failed to read REPL input: {error}");
                            if let Err(err) = fs::remove_dir_all(&temp_dir) {
                                eprintln!("failed to clean REPL temp directory: {err}");
                            }
                            return ExitCode::from(1);
                        }
                    }
                };

                let trimmed = input_line.trim();
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
                                        json!([":help", ":quit", ":reset", ":debug", ":load"]),
                                    ),
                                ],
                                "help",
                            );
                        } else {
                            println!("REPL supports Terlan entries terminated with '.'.");
                            println!(":help, :quit, :reset, :debug, :load <file.terl|project-dir>");
                        }
                    }
                    command if is_repl_debug_command(command) => {
                        if compiler_service.has_active_generation() {
                            match compiler_service.enter_debugger(matches!(
                                state.diagnostic_format,
                                DiagnosticFormat::Json
                            )) {
                                Ok(Some(value)) => {
                                    emit_repl_result(state.diagnostic_format, &value)
                                }
                                Ok(None) => emit_repl_result(state.diagnostic_format, "Unit"),
                                Err(error) => {
                                    let message = error.to_string();
                                    emit_repl_event(
                                        state.diagnostic_format,
                                        "error",
                                        &[repl_json_field("message", message.as_str())],
                                        &message,
                                    );
                                }
                            }
                        } else {
                            let enabled = !compiler_service.debug_enabled();
                            compiler_service.set_debug_enabled(enabled);
                            emit_repl_debug_mode(state.diagnostic_format, enabled);
                        }
                    }
                    ":reset" => {
                        let debug_enabled = compiler_service.debug_enabled();
                        baseline_declarations.clear();
                        declarations.clear();
                        value_bindings.clear();
                        code_server = VmCodeServer::default();
                        compiler_service = ReplCompilerService::default();
                        compiler_service.set_debug_enabled(debug_enabled);
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

                        if let Err(message) =
                            validate_repl_seed_target_evidence(Some(&path), repl_target_profile)
                        {
                            emit_repl_event(
                                state.diagnostic_format,
                                "error",
                                &[repl_json_field("message", message.as_str())],
                                &message,
                            );
                            continue;
                        }

                        match load_repl_seed_declarations(&path, state.diagnostic_format) {
                            Ok(next_declarations) => {
                                let debug_enabled = compiler_service.debug_enabled();
                                baseline_declarations = next_declarations.clone();
                                declarations = next_declarations;
                                value_bindings.clear();
                                code_server = VmCodeServer::default();
                                compiler_service = ReplCompilerService::default();
                                compiler_service.set_debug_enabled(debug_enabled);
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
                                let mut validation_bindings = value_bindings.clone();
                                validation_bindings.push(binding.clone());
                                let run_name = repl_generation_run_name(
                                    "repl_eval",
                                    "Unit",
                                    &declarations,
                                    &validation_bindings,
                                    &module_name,
                                );
                                match run_repl_expression(
                                    &mut compiler_service,
                                    &mut code_server,
                                    ReplExpressionRequest {
                                        expression: "Unit",
                                        declarations: &declarations,
                                        value_bindings: &validation_bindings,
                                        module_name: &module_name,
                                        run_name: &run_name,
                                        temp_dir: &temp_dir,
                                        diagnostic_format: state.diagnostic_format,
                                        native_policy: state.native_policy,
                                        target_profile: repl_target_profile,
                                    },
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
                                    let mutable_receiver = mutable_receiver_binding_name(
                                        expression_source,
                                        &value_bindings,
                                    );
                                    let expression_to_run =
                                        if let Some(receiver) = mutable_receiver.as_deref() {
                                            format!("{expression_source}; {receiver}")
                                        } else {
                                            expression_source.to_string()
                                        };
                                    let run_name = repl_generation_run_name(
                                        "repl_eval",
                                        &expression_to_run,
                                        &declarations,
                                        &value_bindings,
                                        &module_name,
                                    );
                                    match run_repl_expression(
                                        &mut compiler_service,
                                        &mut code_server,
                                        ReplExpressionRequest {
                                            expression: &expression_to_run,
                                            declarations: &declarations,
                                            value_bindings: &value_bindings,
                                            module_name: &module_name,
                                            run_name: &run_name,
                                            temp_dir: &temp_dir,
                                            diagnostic_format: state.diagnostic_format,
                                            native_policy: state.native_policy,
                                            target_profile: repl_target_profile,
                                        },
                                    ) {
                                        Ok(value) => {
                                            if let Some(receiver) = mutable_receiver {
                                                update_repl_value_binding(
                                                    &mut value_bindings,
                                                    &receiver,
                                                    value,
                                                );
                                                emit_repl_result(state.diagnostic_format, "Unit");
                                            } else {
                                                emit_repl_result(state.diagnostic_format, &value);
                                            }
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
