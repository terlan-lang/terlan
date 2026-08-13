use std::fs;
use std::path::PathBuf;

use super::breakpoint::validate_breakpoint_spec;
use super::DebugCliError;

/// Debugger script commands accepted by the VM-owned runtime.
///
/// Inputs:
/// - None; this is the parser-owned command vocabulary.
///
/// Output:
/// - Ordered command names that may appear in `.terldbg` scripts.
///
/// Transformation:
/// - Keeps the debugger command surface auditable while individual
///   command argument validation remains command-specific.
pub(super) const DEBUG_SCRIPT_COMMANDS: &[&str] = &[
    "help",
    "run",
    "list",
    "break",
    "remove",
    "enable",
    "disable",
    "pause",
    "continue",
    "step",
    "next",
    "finish",
    "bt",
    "frame",
    "locals",
    "args",
    "print",
    "eval",
    "processes",
    "process",
    "mailbox",
    "resources",
    "trace",
    "untrace",
    "restarts",
    "restart",
    "use",
    "abort",
    "quit",
];

/// Parsed debugger script command.
///
/// Inputs:
/// - Created from one non-empty, non-comment `.terldbg` script line.
///
/// Output:
/// - Line number, command name, and optional argument text.
///
/// Transformation:
/// - Keeps script validation structured without committing to a runtime
///   debugger executor yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DebugScriptCommand {
    pub(super) line: usize,
    pub(super) name: String,
    pub(super) argument: Option<String>,
}

pub(super) fn required_argument(command: &DebugScriptCommand) -> Result<&str, DebugCliError> {
    command.argument.as_deref().ok_or_else(|| {
        format!(
            "error[vm.debugger.command_argument]: `{}` requires an argument",
            command.name
        )
        .into()
    })
}

/// Reads and validates one debugger script file.
///
/// Inputs:
/// - `path`: `.terldbg` script path supplied through `--script`.
///
/// Output:
/// - Parsed script commands, or a stable CLI diagnostic.
///
/// Transformation:
/// - Validates script files before VM execution so malformed CI
///   and editor scripts fail deterministically even before stepping exists.
pub(super) fn validate_debug_script_file(
    path: &PathBuf,
) -> Result<Vec<DebugScriptCommand>, DebugCliError> {
    let contents = fs::read_to_string(path).map_err(|err| DebugCliError {
        code: "debug_script_read_failed",
        message: format!("failed to read debugger script `{}`: {err}", path.display()),
    })?;

    parse_debug_script(&contents)
}

/// Parses a debugger script into validated commands.
///
/// Inputs:
/// - `contents`: UTF-8 script text.
///
/// Output:
/// - Parsed commands, or a stable malformed-script diagnostic.
///
/// Transformation:
/// - Supports comments and blank lines, then validates each command against
///   the debugger command vocabulary.
pub(super) fn parse_debug_script(contents: &str) -> Result<Vec<DebugScriptCommand>, DebugCliError> {
    let mut commands = Vec::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        commands.push(parse_debug_script_line(line_number, line)?);
    }

    if commands.is_empty() {
        return Err(DebugCliError {
            code: "debug_script_empty",
            message: "debugger script must contain at least one command".to_string(),
        });
    }

    Ok(commands)
}

/// Parses one debugger script command line.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `line`: trimmed non-empty script line.
///
/// Output:
/// - Structured command name and argument text.
///
/// Transformation:
/// - Splits only the first whitespace boundary so expression-like arguments
///   remain intact for future debugger evaluation.
pub(super) fn parse_debug_script_line(
    line_number: usize,
    line: &str,
) -> Result<DebugScriptCommand, DebugCliError> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let argument = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    validate_debug_script_command(line_number, line, name, argument.as_deref())?;
    Ok(DebugScriptCommand {
        line: line_number,
        name: name.to_string(),
        argument,
    })
}

/// Validates one debugger script command.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `line`: original trimmed line.
/// - `name`: command name.
/// - `argument`: optional argument text.
///
/// Output:
/// - `Ok(())` for a supported command shape.
/// - `Err(DebugCliError)` for unknown commands or invalid arguments.
///
/// Transformation:
/// - Locks the script grammar around the documented debugger verbs while
///   leaving actual command execution for the VM debugger runtime.
fn validate_debug_script_command(
    line_number: usize,
    line: &str,
    name: &str,
    argument: Option<&str>,
) -> Result<(), DebugCliError> {
    if !DEBUG_SCRIPT_COMMANDS.contains(&name) {
        return Err(DebugCliError {
            code: "debug_script_unknown_command",
            message: format!("unknown debugger script command on line {line_number}: `{name}`"),
        });
    }

    match name {
        "help" | "run" | "continue" | "step" | "next" | "finish" | "pause" | "bt" | "locals"
        | "args" | "processes" | "mailbox" | "resources" | "restarts" | "list" | "abort"
        | "quit" => reject_debug_script_argument(line_number, line, argument),
        "break" => {
            let Some(spec) = argument else {
                return Err(debug_script_missing_argument(line_number, name));
            };
            validate_breakpoint_spec(spec).map_err(|err| DebugCliError {
                code: "debug_script_invalid_breakpoint",
                message: format!("line {line_number}: {}", err.message),
            })
        }
        "frame" | "process" => {
            validate_debug_script_positive_integer_arg(line_number, name, argument)
        }
        "remove" | "enable" | "disable" => {
            validate_debug_script_breakpoint_selector_arg(line_number, name, argument)
        }
        "print" | "eval" | "trace" | "untrace" | "use" | "restart" => {
            require_debug_script_argument(line_number, name, argument)
        }
        _ => unreachable!("debugger script command must have validation"),
    }
}

/// Validates a breakpoint selector argument.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `name`: command name.
/// - `argument`: optional parsed argument.
///
/// Output:
/// - `Ok(())` when the argument is a positive breakpoint id or a valid
///   breakpoint spec.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Lets scripts manage breakpoints by stable id or source shape before the
///   runtime breakpoint table exists.
fn validate_debug_script_breakpoint_selector_arg(
    line_number: usize,
    name: &str,
    argument: Option<&str>,
) -> Result<(), DebugCliError> {
    let Some(value) = argument else {
        return Err(debug_script_missing_argument(line_number, name));
    };
    if value.parse::<usize>().is_ok_and(|number| number > 0) {
        return Ok(());
    }
    validate_breakpoint_spec(value).map_err(|_| DebugCliError {
        code: "debug_script_invalid_breakpoint_selector",
        message: format!(
            "debugger script command `{name}` on line {line_number} needs a breakpoint id or spec"
        ),
    })
}

/// Rejects arguments for no-argument debugger script commands.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `line`: original trimmed line.
/// - `argument`: optional parsed argument.
///
/// Output:
/// - `Ok(())` when no argument was supplied.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Keeps commands such as `step` and `quit` unambiguous.
fn reject_debug_script_argument(
    line_number: usize,
    line: &str,
    argument: Option<&str>,
) -> Result<(), DebugCliError> {
    if argument.is_some() {
        return Err(DebugCliError {
            code: "debug_script_unexpected_argument",
            message: format!("unexpected argument on debugger script line {line_number}: `{line}`"),
        });
    }

    Ok(())
}

/// Requires an argument for one debugger script command.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `name`: command name.
/// - `argument`: optional parsed argument.
///
/// Output:
/// - `Ok(())` when an argument is present.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Keeps future expression and trace commands from receiving empty payloads.
fn require_debug_script_argument(
    line_number: usize,
    name: &str,
    argument: Option<&str>,
) -> Result<(), DebugCliError> {
    if argument.is_none() {
        return Err(debug_script_missing_argument(line_number, name));
    }

    Ok(())
}

/// Validates a positive integer debugger script argument.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `name`: command name.
/// - `argument`: optional parsed argument.
///
/// Output:
/// - `Ok(())` when the argument is a positive integer.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Gives `frame` and `process` a stable numeric selector contract.
fn validate_debug_script_positive_integer_arg(
    line_number: usize,
    name: &str,
    argument: Option<&str>,
) -> Result<(), DebugCliError> {
    let Some(value) = argument else {
        return Err(debug_script_missing_argument(line_number, name));
    };
    let Ok(number) = value.parse::<usize>() else {
        return Err(invalid_positive_integer_argument(line_number, name));
    };
    if number == 0 {
        return Err(invalid_positive_integer_argument(line_number, name));
    }

    Ok(())
}

/// Builds an invalid positive-integer argument diagnostic.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `name`: command name.
///
/// Output:
/// - Stable CLI parser diagnostic.
///
/// Transformation:
/// - Shares the same diagnostic across parse failure and zero values.
fn invalid_positive_integer_argument(line_number: usize, name: &str) -> DebugCliError {
    DebugCliError {
        code: "debug_script_invalid_argument",
        message: format!(
            "debugger script command `{name}` on line {line_number} requires a positive integer"
        ),
    }
}

/// Builds a missing-argument debugger script diagnostic.
///
/// Inputs:
/// - `line_number`: one-based source line number.
/// - `name`: command name.
///
/// Output:
/// - Stable CLI parser diagnostic.
///
/// Transformation:
/// - Centralizes argument diagnostics so command-specific validators remain
///   small and consistent.
fn debug_script_missing_argument(line_number: usize, name: &str) -> DebugCliError {
    DebugCliError {
        code: "debug_script_missing_argument",
        message: format!(
            "debugger script command `{name}` on line {line_number} needs an argument"
        ),
    }
}
