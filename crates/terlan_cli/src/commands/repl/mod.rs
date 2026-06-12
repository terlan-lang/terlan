use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::UNIX_EPOCH;

use terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge;
use terlan_syntax::{
    parse_expr_as_syntax_output, parse_module_as_syntax_output, EbnfCompileError,
    SyntaxDeclarationOutput, SyntaxModuleOutput,
};
use terlan_typeck::{infer_syntax_expression_type, pretty_type};

use crate::commands::json::json_string;
use crate::validation::native_policy::NativePolicy;
use crate::{CliCommand, CliState, DiagnosticFormat};

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
        [arg] if arg == "--help" => {
            println!("terlc repl [--help] [<file.tl>]");
            println!("Interactive mode accepts one line at a time and supports :quit/:q to exit.");
            println!("Available commands: :help, :quit, :q, :reset, :reload [<file.tl>], :module <file.tl>, :type <expr>");
            ExitCode::SUCCESS
        }
        args => {
            if args.len() > 1 {
                eprintln!("repl accepts only --help and optional <file.tl>");
                return ExitCode::from(2);
            }

            let seed_path = args.first().cloned();
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

            let mut seed_source_path = seed_path.clone();
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
            let mut eval_counter = 0usize;
            emit_repl_event(state.diagnostic_format, "ready", None, "REPL ready");

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
                    ":q" | ":quit" => {
                        if let Err(err) = fs::remove_dir_all(&temp_dir) {
                            eprintln!("failed to clean REPL temp directory: {err}");
                            return ExitCode::from(1);
                        }
                        emit_repl_event(
                            state.diagnostic_format,
                            "ready",
                            Some("\"status\":\"ready_to_exit\""),
                            "REPL exiting",
                        );
                        return ExitCode::SUCCESS;
                    }
                    ":help" => {
                        if matches!(state.diagnostic_format, DiagnosticFormat::Json) {
                            emit_repl_event(
                                state.diagnostic_format,
                                "status",
                                Some(
                                    "\"message\":\"REPL supports expression evaluation and session declarations.\",\"commands\":[\":help\",\":quit\",\":q\",\":reset\",\":reload\",\":module\",\":type\"]"
                                ),
                                "help",
                            );
                        } else {
                            println!(
                                "REPL supports expression evaluation and session declarations."
                            );
                            println!(":help, :quit/:q, :reset, :reload [<file.tl>], :module <file.tl>, :type <expr>");
                        }
                    }
                    command if command.starts_with(":type") => {
                        let expression = command.strip_prefix(":type").unwrap_or("").trim();
                        if expression.is_empty() {
                            emit_repl_event(
                                state.diagnostic_format,
                                "error",
                                Some("\"message\":\":type requires an expression: :type <expr>\""),
                                ":type requires an expression: :type <expr>",
                            );
                            continue;
                        }

                        match run_repl_type_query(
                            expression,
                            &declarations,
                            &module_name,
                            &temp_dir,
                            state.diagnostic_format,
                            state.native_policy,
                            state.target_profile,
                        ) {
                            Ok(value) => emit_repl_event(
                                state.diagnostic_format,
                                "type",
                                Some(&format!("\"value\":{}", json_string(&value))),
                                &format!("type: {value}"),
                            ),
                            Err(()) => {}
                        }
                    }
                    ":reset" => {
                        baseline_declarations.clear();
                        declarations.clear();
                        seed_source_path = None;
                        emit_repl_event(
                            state.diagnostic_format,
                            "status",
                            Some("\"status\":\"session reset\""),
                            "session reset",
                        );
                    }
                    command if command.starts_with(":reload") => {
                        let explicit_path = command.strip_prefix(":reload").unwrap_or("").trim();
                        let path = match (explicit_path, seed_source_path.as_deref()) {
                            ("", None) => {
                                emit_repl_event(
                                    state.diagnostic_format,
                                    "error",
                                    Some("\"message\":\":reload requires a path: :reload <file.tl>\""),
                                    ":reload requires a path: :reload <file.tl>",
                                );
                                continue;
                            }
                            ("", Some(path)) => path.to_string(),
                            (path, _) => path.to_string(),
                        };

                        match load_repl_seed_declarations(&path, state.diagnostic_format) {
                            Ok(next_declarations) => {
                                seed_source_path = Some(path.clone());
                                baseline_declarations = next_declarations.clone();
                                declarations = next_declarations;
                                let status = format!(
                                    "\"status\":\"reloaded\",\"path\":{}",
                                    json_string(&path),
                                );
                                emit_repl_event(
                                    state.diagnostic_format,
                                    "status",
                                    Some(&status),
                                    "session reloaded",
                                );
                            }
                            Err(_code) => {}
                        }
                    }
                    command if command.starts_with(":module") => {
                        let explicit_path = command.strip_prefix(":module").unwrap_or("").trim();
                        let path = match explicit_path {
                            "" => {
                                emit_repl_event(
                                    state.diagnostic_format,
                                    "error",
                                    Some(
                                        "\"message\":\":module requires a path: :module <file.tl>\""
                                    ),
                                    ":module requires a path: :module <file.tl>",
                                );
                                continue;
                            }
                            path => path.to_string(),
                        };

                        match load_repl_seed_declarations(&path, state.diagnostic_format) {
                            Ok(next_declarations) => {
                                seed_source_path = Some(path.clone());
                                baseline_declarations = next_declarations.clone();
                                declarations = next_declarations;
                                let status = format!(
                                    "\"status\":\"module_switched\",\"path\":{}",
                                    json_string(&path),
                                );
                                emit_repl_event(
                                    state.diagnostic_format,
                                    "status",
                                    Some(&status),
                                    "module switched",
                                );
                            }
                            Err(_code) => {}
                        }
                    }
                    _ => match parse_expr_as_syntax_output(trimmed) {
                        Ok(_expr) => {
                            eval_counter += 1;
                            let run_name = format!("repl_eval_{}", eval_counter);
                            match run_repl_expression(
                                trimmed,
                                &declarations,
                                &module_name,
                                &run_name,
                                &temp_dir,
                                state.diagnostic_format,
                                state.native_policy,
                                state.target_profile,
                            ) {
                                Ok(value) => emit_repl_result(state.diagnostic_format, &value),
                                Err(message) => emit_repl_event(
                                    state.diagnostic_format,
                                    "error",
                                    Some(&format!("\"message\":{}", json_string(&message))),
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
                                Some((&format!("parse serialization error: {message}"), 0, 0)),
                            );
                        }
                    },
                }
            }
        }
    }
}

fn parse_repl_declaration_and_log(
    module_name: &str,
    declaration: &str,
    diagnostic_format: DiagnosticFormat,
    declarations: &mut Vec<String>,
    expr_parse_error: Option<(&str, usize, usize)>,
) {
    match parse_repl_declaration(module_name, declaration) {
        Ok(mut next_declarations) => {
            if next_declarations.is_empty() {
                crate::support::emit_diagnostic(
                    "parse_error",
                    "no declaration parsed; expected a valid declaration",
                    "<repl>",
                    0,
                    0,
                    diagnostic_format,
                );
            } else {
                declarations.append(&mut next_declarations);
                if matches!(diagnostic_format, DiagnosticFormat::Json) {
                    emit_repl_event(
                        diagnostic_format,
                        "status",
                        Some("\"status\":\"declaration_added\""),
                        "declaration added",
                    );
                } else {
                    println!("declaration added");
                }
            }
        }
        Err((decl_message, start, end)) => {
            if let Some((expr_message, expr_start, expr_end)) = expr_parse_error {
                crate::support::emit_diagnostic(
                    "parse_error",
                    &format!("expression parse error: {expr_message}"),
                    "<repl>",
                    expr_start,
                    expr_end,
                    diagnostic_format,
                );
            }
            crate::support::emit_diagnostic(
                "parse_error",
                &format!("declaration parse error: {decl_message}"),
                "<repl>",
                start,
                end,
                diagnostic_format,
            );
        }
    }
}

fn parse_syntax_module(source: &str) -> Result<SyntaxModuleOutput, (String, usize, usize)> {
    match parse_module_as_syntax_output(source) {
        Ok(module) => Ok(module),
        Err(EbnfCompileError::Parse(message, span)) => Err((message, span.start, span.end)),
        Err(EbnfCompileError::Serialize(message)) => Err((message, 0, 0)),
    }
}

fn repl_declarations_to_source(module_name: &str, declarations: &[String]) -> String {
    let mut source = format!("module {}.\n\n", module_name);
    for declaration in declarations {
        source.push_str(declaration);
        if !declaration.ends_with('\n') {
            source.push('\n');
        }
        source.push('\n');
    }
    source
}

fn repl_declaration_sources(source: &str, declarations: &[SyntaxDeclarationOutput]) -> Vec<String> {
    declarations
        .iter()
        .filter_map(|declaration| {
            source
                .get(declaration.span.start..declaration.span.end)
                .map(|text| text.to_string())
        })
        .collect()
}

fn parse_repl_declaration(
    module_name: &str,
    declaration: &str,
) -> Result<Vec<String>, (String, usize, usize)> {
    let source = format!("module {}.\n\n{}\n", module_name, declaration);
    let module = parse_syntax_module(&source)?;
    let declarations = repl_declaration_sources(&source, &module.declarations);
    if declarations.is_empty() {
        return Err((
            "no declaration parsed; expected a valid declaration".into(),
            0,
            0,
        ));
    }
    Ok(declarations)
}

fn load_repl_seed_declarations(
    path: &str,
    diagnostic_format: DiagnosticFormat,
) -> Result<Vec<String>, ExitCode> {
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            emit_repl_event(
                diagnostic_format,
                "error",
                Some(&format!(
                    "\"message\":{},\"path\":{}",
                    json_string(&format!("failed to load repl seed module: {message}")),
                    json_string(path),
                )),
                "failed to load repl seed module",
            );
            return Err(ExitCode::from(1));
        }
    };

    match parse_syntax_module(&source) {
        Ok(syntax_module) => Ok(repl_declaration_sources(
            &source,
            &syntax_module.declarations,
        )),
        Err((message, start, end)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                path,
                start,
                end,
                diagnostic_format,
            );
            Err(ExitCode::from(1))
        }
    }
}

/// Compiles and executes one REPL expression.
///
/// Inputs:
/// - `expression`: Terlan expression source entered by the user.
/// - `declarations`: accumulated session declarations.
/// - `module_name`: generated REPL module name.
/// - `run_name`: generated function name for this expression.
/// - `temp_dir`: session temporary output directory.
/// - `diagnostic_format`: output format for diagnostics.
/// - `native_policy`: native-code policy enforced during compilation.
///
/// Output:
/// - Rendered Erlang value text or an error message.
///
/// Transformation:
/// - Builds a synthetic module, runs it through compiler phases, emits Erlang,
///   compiles the generated module, executes the generated function, and returns
///   stdout.
fn run_repl_expression(
    expression: &str,
    declarations: &[String],
    module_name: &str,
    run_name: &str,
    temp_dir: &Path,
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
) -> Result<String, String> {
    let mut source = repl_declarations_to_source(module_name, declarations);
    source.push_str(&format!(
        "pub {}(): Dynamic ->\n    {}.\n",
        run_name, expression
    ));

    let source_path = temp_dir.join(format!("{}.tl", module_name));
    if let Err(err) = fs::write(&source_path, source.as_bytes()) {
        return Err(format!("failed to write REPL module: {err}"));
    }

    let source_path_text = source_path.to_string_lossy().into_owned();
    let compiled = match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_path_text,
        &source,
        diagnostic_format,
        None,
        native_policy,
        target_profile,
    ) {
        Ok(compiled) => compiled,
        Err(_) => return Err("failed to type-check REPL expression".into()),
    };

    let interfaces = compiled.interfaces.into_iter().collect::<BTreeMap<_, _>>();
    let erlang = try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .map_err(|message| format!("failed to emit REPL Erlang output: {message}"))?;
    let erl_path = temp_dir.join(format!(
        "{}.erl",
        crate::support::erlang_output_stem(&compiled.syntax_output.module_name)
    ));
    if let Err(err) = fs::write(&erl_path, erlang) {
        return Err(format!("failed to write REPL Erlang output: {err}"));
    }

    let mut command = Command::new("erlc");
    command.arg("-o").arg(temp_dir).arg(&erl_path);
    if let Err(error) = run_command_with_no_erl_crash_dump(&mut command, "REPL compile", None) {
        return Err(format!("failed to compile REPL generated module: {error}"));
    }

    let value = {
        let mut command = Command::new("erl");
        command
            .arg("-noshell")
            .arg("-pa")
            .arg(temp_dir)
            .arg("-eval")
            .arg(format!(
                "io:format(\"~p\", [{}:{}()]), halt(0).",
                module_name, run_name
            ));
        match run_command_with_no_erl_crash_dump(
            &mut command,
            "REPL execution",
            Some(&temp_dir.join("repl_erl_crash_dump")),
        ) {
            Ok(value) => value,
            Err(error) => {
                return Err(format!("failed to execute REPL expression: {error}"));
            }
        }
    };

    let stdout = String::from_utf8_lossy(&value.stdout);
    let rendered = stdout.trim_end_matches(['\n', '\r']);
    Ok(rendered.to_string())
}

/// Infers the type of one REPL expression without executing it.
///
/// Inputs:
/// - `expression`: Terlan expression source entered after `:type`.
/// - `declarations`: accumulated session declarations.
/// - `module_name`: generated REPL module name.
/// - `temp_dir`: session temporary output directory.
/// - `diagnostic_format`: output format for diagnostics.
/// - `native_policy`: native-code policy enforced during declaration checking.
///
/// Output:
/// - Pretty-printed type text or `Err(())` after emitting diagnostics.
///
/// Transformation:
/// - Compiles the declaration context through compiler phases, parses the query
///   expression as syntax output, runs type inference, and formats the inferred
///   type.
fn run_repl_type_query(
    expression: &str,
    declarations: &[String],
    module_name: &str,
    temp_dir: &Path,
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
) -> Result<String, ()> {
    let source = repl_declarations_to_source(module_name, declarations);
    let source_path = temp_dir.join(format!("{}.tl", module_name));
    if let Err(err) = fs::write(&source_path, source.as_bytes()) {
        emit_repl_event(
            diagnostic_format,
            "error",
            Some(&format!(
                "\"message\":{}",
                json_string(&format!("failed to write REPL type query module: {err}")),
            )),
            "failed to write REPL type query module",
        );
        return Err(());
    }

    let source_path_text = source_path.to_string_lossy().into_owned();
    let compiled = match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_path_text,
        &source,
        diagnostic_format,
        None,
        native_policy,
        target_profile,
    ) {
        Ok(compiled) => compiled,
        Err(_) => {
            emit_repl_event(
                diagnostic_format,
                "error",
                Some("\"message\":\"failed to type-check REPL declarations\""),
                "failed to type-check REPL declarations",
            );
            return Err(());
        }
    };

    let expression = match parse_expr_as_syntax_output(expression) {
        Ok(expression) => expression,
        Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                "<repl>",
                span.start,
                span.end,
                diagnostic_format,
            );
            return Err(());
        }
        Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                "<repl>",
                0,
                0,
                diagnostic_format,
            );
            return Err(());
        }
    };

    let (ty, diagnostics) =
        infer_syntax_expression_type(&expression, &compiled.syntax_output, &compiled.resolved);
    let has_error = diagnostics
        .iter()
        .any(|diag| !matches!(diag.severity, terlan_typeck::DiagSeverity::Warning));
    for diag in diagnostics {
        crate::support::emit_diagnostic(
            if matches!(diag.severity, terlan_typeck::DiagSeverity::Warning) {
                "warning"
            } else {
                "type_error"
            },
            &diag.message,
            "<repl>",
            diag.span.start,
            diag.span.end,
            diagnostic_format,
        );
    }

    if has_error {
        return Err(());
    }

    Ok(pretty_type(&ty))
}

/// Emits a REPL event in text or JSON mode.
///
/// Inputs:
/// - `diagnostic_format`: output mode selected by global flags.
/// - `kind`: event kind string.
/// - `fields`: optional pre-rendered JSON fields for JSON mode.
/// - `text`: human-readable text payload.
///
/// Output:
/// - No return value; writes to stdout or stderr.
///
/// Transformation:
/// - Converts REPL events into stable JSON records or text-mode messages.
fn emit_repl_event(
    diagnostic_format: DiagnosticFormat,
    kind: &str,
    fields: Option<&str>,
    text: &str,
) {
    match diagnostic_format {
        DiagnosticFormat::Text { .. } => {
            if kind == "error" && text.is_empty() {
                eprintln!("{kind}");
            } else {
                println!("{text}");
            }
        }
        DiagnosticFormat::Json => {
            let payload = fields.filter(|value| !value.is_empty()).unwrap_or("");
            let extra = if payload.is_empty() { "" } else { "," };
            println!(
                "{{\"schema\":\"terlan-repl-event-v1\",\"kind\":\"{}\"{}{}\"text\":{}}}",
                kind,
                extra,
                payload,
                json_string(text),
            );
        }
    }
}

/// Emits a successful REPL expression result.
///
/// Inputs:
/// - `diagnostic_format`: output mode selected by global flags.
/// - `value`: rendered Erlang value from expression execution.
///
/// Output:
/// - No return value; writes a result event.
///
/// Transformation:
/// - Normalizes empty output to `ok` and otherwise includes the rendered value.
fn emit_repl_result(diagnostic_format: DiagnosticFormat, value: &str) {
    if value.trim().is_empty() {
        emit_repl_event(diagnostic_format, "result", Some("\"value\":\"ok\""), "ok");
    } else {
        emit_repl_event(
            diagnostic_format,
            "result",
            Some(&format!("\"value\":{}", json_string(value))),
            &format!("ok: {value}"),
        );
    }
}

/// Runs a process and rejects unexpected Erlang crash dumps.
///
/// Inputs:
/// - `command`: configured process command to execute.
/// - `label`: human-readable command label for errors.
/// - `erl_crash_dump`: optional Erlang crash-dump guard path.
///
/// Output:
/// - Successful process output or an error message.
///
/// Transformation:
/// - Sets `ERL_CRASH_DUMP` when supplied, checks process status, and rejects
///   unexpected crash dump file creation.
fn run_command_with_no_erl_crash_dump(
    command: &mut Command,
    label: &str,
    erl_crash_dump: Option<&Path>,
) -> Result<std::process::Output, String> {
    if let Some(path) = erl_crash_dump {
        let _ = fs::remove_file(path);
        command.env("ERL_CRASH_DUMP", path);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output
                .status
                .code()
                .map_or_else(|| "signal".to_string(), |code| code.to_string()),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if let Some(path) = erl_crash_dump {
        if path.exists() {
            return Err(format!("{label} created ERL_CRASH_DUMP at {path:?}"));
        }
    }
    Ok(output)
}
