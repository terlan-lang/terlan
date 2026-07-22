mod breakpoint;
mod script;
mod session;

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::json;

use crate::{CliCommand, DiagnosticFormat};

use breakpoint::validate_breakpoint_spec;
#[cfg(test)]
use script::parse_debug_script;
use script::validate_debug_script_file;
use session::{open_native_debug_session, NativeDebugSessionReport};

pub(crate) const RESERVED_CODE: &str = "debugger_unimplemented";
pub(crate) const RESERVED_MESSAGE: &str = "terlc debug is reserved for VM-owned debugger execution";

/// Parsed command-local arguments for `terlc debug`.
///
/// Inputs:
/// - Produced from command-local CLI arguments after the top-level dispatcher
///   selects `debug`.
///
/// Output:
/// - Target, breakpoint, script, and machine-readable event preferences for a
///   native-image debugger session.
///
/// Transformation:
/// - Keeps parser coverage separate from native-image admission so
///   command-surface tests stay stable while VM internals evolve.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DebugArgs {
    target: Option<PathBuf>,
    breakpoints: Vec<String>,
    script: Option<PathBuf>,
    json_events: bool,
}

/// Stable command-line parser error for `terlc debug`.
///
/// Inputs:
/// - `code`: stable diagnostic code used by tests and editor integrations.
/// - `message`: user-facing explanation for the malformed invocation.
///
/// Output:
/// - Renderable text or JSON diagnostic.
///
/// Transformation:
/// - Separates error identity from presentation so global diagnostic format can
///   switch rendering without changing parser behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DebugCliError {
    pub(super) code: &'static str,
    pub(super) message: String,
}

/// Executes the `debug` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command with debugger-local arguments.
/// - `diagnostic_format`: global diagnostic rendering mode.
///
/// Output:
/// - Success for command-local help.
/// - Exit 1 for native-image admission or breakpoint-resolution failures.
/// - Exit 2 for malformed debugger arguments.
///
/// Transformation:
/// - Parses the debugger command, admits one native application image through
///   the execution shard, and exposes descriptor, continuation, and source-map
///   metadata without retaining executable CoreIR.
pub(crate) fn run(cmd: CliCommand, diagnostic_format: DiagnosticFormat) -> ExitCode {
    if matches!(cmd.args.as_slice(), [flag] if matches!(flag.as_str(), "--help" | "-h")) {
        crate::print_command_usage("debug");
        return ExitCode::SUCCESS;
    }

    let args = match parse_debug_args(&cmd.args) {
        Ok(args) => args,
        Err(err) => {
            print_debug_error(&err, diagnostic_format);
            return ExitCode::from(2);
        }
    };
    let script_commands = match args.script.as_ref() {
        Some(script) => match validate_debug_script_file(script) {
            Ok(commands) => Some(commands.len()),
            Err(err) => {
                print_debug_error(&err, diagnostic_format);
                return ExitCode::from(2);
            }
        },
        None => None,
    };

    let report = match open_native_debug_session(&args, script_commands) {
        Ok(report) => report,
        Err(err) => {
            print_debug_error(&err, diagnostic_format);
            return ExitCode::from(1);
        }
    };
    print_native_debug_report(&report, diagnostic_format);
    ExitCode::SUCCESS
}

/// Parses command-local arguments for `terlc debug`.
///
/// Inputs:
/// - `args`: raw arguments after `debug`.
///
/// Output:
/// - `Ok(DebugArgs)` when the invocation names a native image target or a
///   debugger script.
/// - `Err(DebugCliError)` for missing targets, missing option values, unknown
///   options, duplicate scripts, or extra positional targets.
///
/// Transformation:
/// - Accepts one optional positional target plus repeated `--break` values,
///   one `--script` file, and a `--json-events` marker for machine-readable
///   debugger event output.
fn parse_debug_args(args: &[String]) -> Result<DebugArgs, DebugCliError> {
    let mut parsed = DebugArgs {
        target: None,
        breakpoints: Vec::new(),
        script: None,
        json_events: false,
    };

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--break" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| DebugCliError {
                    code: "debug_missing_option_value",
                    message: "--break requires a breakpoint spec".to_string(),
                })?;
                if value.starts_with('-') {
                    return Err(DebugCliError {
                        code: "debug_missing_option_value",
                        message: "--break requires a breakpoint spec".to_string(),
                    });
                }
                validate_breakpoint_spec(value)?;
                parsed.breakpoints.push(value.clone());
            }
            "--script" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| DebugCliError {
                    code: "debug_missing_option_value",
                    message: "--script requires a debugger script path".to_string(),
                })?;
                if value.starts_with('-') {
                    return Err(DebugCliError {
                        code: "debug_missing_option_value",
                        message: "--script requires a debugger script path".to_string(),
                    });
                }
                if parsed.script.is_some() {
                    return Err(DebugCliError {
                        code: "debug_duplicate_script",
                        message: "terlc debug accepts at most one --script".to_string(),
                    });
                }
                parsed.script = Some(PathBuf::from(value));
            }
            "--json-events" => {
                parsed.json_events = true;
            }
            flag if flag.starts_with('-') => {
                return Err(DebugCliError {
                    code: "debug_unknown_option",
                    message: format!("unknown terlc debug option: {flag}"),
                });
            }
            target => {
                if parsed.target.is_some() {
                    return Err(DebugCliError {
                        code: "debug_too_many_targets",
                        message: "terlc debug accepts at most one native image".to_string(),
                    });
                }
                parsed.target = Some(PathBuf::from(target));
            }
        }
        index += 1;
    }

    if parsed.target.is_none() && parsed.script.is_none() {
        return Err(DebugCliError {
            code: "debug_missing_target",
            message: "terlc debug requires a native image or --script".to_string(),
        });
    }

    Ok(parsed)
}

/// Prints a debugger parser error in the requested diagnostic format.
///
/// Inputs:
/// - `err`: stable command parser error.
/// - `diagnostic_format`: global text or JSON renderer selection.
///
/// Output:
/// - Error diagnostic written to stderr.
///
/// Transformation:
/// - Delegates to text or JSON renderers while preserving exit-code decisions
///   in the caller.
fn print_debug_error(err: &DebugCliError, diagnostic_format: DiagnosticFormat) {
    match diagnostic_format {
        DiagnosticFormat::Json => eprintln!("{}", render_debug_error_json(err)),
        DiagnosticFormat::Text { .. } => eprintln!("{}", render_debug_error_text(err)),
    }
}

/// Prints an admitted native debugger report in the requested format.
///
/// Inputs:
/// - `report`: admitted native-image debugger payload.
/// - `diagnostic_format`: global text or JSON renderer selection.
///
/// Output:
/// - Debugger session report written to stdout.
///
/// Transformation:
/// - Keeps text and machine-readable views over the same admitted metadata.
fn print_native_debug_report(
    report: &NativeDebugSessionReport,
    diagnostic_format: DiagnosticFormat,
) {
    match diagnostic_format {
        DiagnosticFormat::Json => println!("{}", render_native_debug_report_json(report)),
        DiagnosticFormat::Text { .. } => println!("{}", render_native_debug_report_text(report)),
    }
}

/// Renders a debugger parser error as text.
///
/// Inputs:
/// - `err`: stable parser error.
///
/// Output:
/// - One-line text diagnostic.
///
/// Transformation:
/// - Formats the error with a stable diagnostic code prefix.
fn render_debug_error_text(err: &DebugCliError) -> String {
    format!("error[{}]: {}", err.code, err.message)
}

/// Renders a debugger parser error as JSON.
///
/// Inputs:
/// - `err`: stable parser error.
///
/// Output:
/// - JSON diagnostic string.
///
/// Transformation:
/// - Uses `serde_json` so quoting and escaping are handled by a structured
///   serializer rather than hand-built strings.
fn render_debug_error_json(err: &DebugCliError) -> String {
    json!({
        "code": err.code,
        "message": err.message,
        "command": "debug",
        "kind": "error"
    })
    .to_string()
}

/// Renders an admitted native debugger report as text.
///
/// Inputs:
/// - `report`: admitted native-image debugger report.
///
/// Output:
/// - Multi-line debugger inventory with stable field order.
///
/// Transformation:
/// - Exposes the admitted image identity and native execution metadata without
///   rendering resident compiler IR.
fn render_native_debug_report_text(report: &NativeDebugSessionReport) -> String {
    let script_commands = report
        .script_commands
        .map_or_else(|| "<none>".to_string(), |count| count.to_string());
    let exports = if report.exports.is_empty() {
        "<none>".to_string()
    } else {
        report.exports.join(", ")
    };
    let breakpoints = report
        .breakpoints
        .iter()
        .map(|breakpoint| format!("{} => {}", breakpoint.spec, breakpoint.functions.join(", ")))
        .collect::<Vec<_>>();
    let breakpoints = if breakpoints.is_empty() {
        "<none>".to_string()
    } else {
        breakpoints.join("; ")
    };
    format!(
        concat!(
            "Terlan native debugger image admitted\n",
            "  target: {}\n",
            "  format: {}\n",
            "  architecture: {}\n",
            "  target_triple: {}\n",
            "  compiler: {}\n",
            "  build: {}\n",
            "  package: {}\n",
            "  module: {}\n",
            "  descriptor_digest: {}\n",
            "  exports: {}\n",
            "  continuations: {}\n",
            "  source_records: {}\n",
            "  breakpoints: {}\n",
            "  script_commands: {}\n",
            "  json_events: {}"
        ),
        report.target,
        report.format,
        report.architecture,
        report.target_triple,
        report.compiler,
        report.build,
        report.package,
        report.module,
        report.descriptor_digest,
        exports,
        report.continuation_ids.len(),
        report.source_record_count,
        breakpoints,
        script_commands,
        report.json_events
    )
}

/// Renders an admitted native debugger report as JSON.
///
/// Inputs:
/// - `report`: admitted native-image debugger report.
///
/// Output:
/// - JSON diagnostic string.
///
/// Transformation:
/// - Uses structured serialization so editor integrations receive exact image,
///   continuation, export, and source-map inventory.
fn render_native_debug_report_json(report: &NativeDebugSessionReport) -> String {
    let breakpoints = report
        .breakpoints
        .iter()
        .map(|breakpoint| {
            json!({
                "spec": breakpoint.spec,
                "functions": breakpoint.functions,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "command": "debug",
        "kind": "native_image_admitted",
        "target": report.target,
        "format": report.format,
        "architecture": report.architecture,
        "target_triple": report.target_triple,
        "compiler": report.compiler,
        "build": report.build,
        "package": report.package,
        "module": report.module,
        "descriptor_digest": report.descriptor_digest,
        "exports": report.exports,
        "continuation_ids": report.continuation_ids,
        "source_record_count": report.source_record_count,
        "breakpoints": breakpoints,
        "script_commands": report.script_commands,
        "json_events": report.json_events,
        "live_execution": false,
    })
    .to_string()
}

#[cfg(test)]
#[path = "debug_test.rs"]
mod debug_test;
