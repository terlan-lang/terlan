mod evaluator;

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::UNIX_EPOCH;

use terlan_syntax::{
    parse_expr_as_syntax_output, parse_module_as_syntax_output, EbnfCompileError,
    SyntaxDeclarationOutput, SyntaxModuleOutput,
};

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
        args if is_repl_help_args(args) => {
            print_repl_help();
            ExitCode::SUCCESS
        }
        args => {
            if args.len() > 1 {
                eprintln!("repl accepts only --help, -h, and optional <file.terl|project-dir>");
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
                    ":quit" => {
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
                                    "\"message\":\"REPL supports expression evaluation and session declarations.\",\"commands\":[\":help\",\":quit\",\":reset\",\":load\"]"
                                ),
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
                                    Some("\"message\":\":load requires a path: :load <file.terl|project-dir>\""),
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
                            Some(&format!(
                                "\"message\":{}",
                                json_string(&format!("unknown REPL command: {command}")),
                            )),
                            &format!("unknown REPL command: {command}"),
                        );
                    }
                    _ => match repl_expression_source(trimmed) {
                        Some(expression_source) => {
                            if let Some(binding) = parse_repl_value_binding(expression_source) {
                                eval_counter += 1;
                                let run_name = format!("repl_eval_{}", eval_counter);
                                match run_repl_expression(
                                    binding.value.as_str(),
                                    &declarations,
                                    &value_bindings,
                                    &module_name,
                                    &run_name,
                                    &temp_dir,
                                    state.diagnostic_format,
                                    state.native_policy,
                                    state.target_profile,
                                ) {
                                    Ok(_value) => {
                                        value_bindings.push(binding);
                                        emit_repl_result(state.diagnostic_format, "Unit");
                                    }
                                    Err(message) => emit_repl_event(
                                        state.diagnostic_format,
                                        "error",
                                        Some(&format!("\"message\":{}", json_string(&message))),
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
                                    ) {
                                        Ok(value) => {
                                            emit_repl_result(state.diagnostic_format, &value)
                                        }
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
                            Some("\"message\":\"REPL entries must end with '.'\""),
                            "REPL entries must end with '.'",
                        ),
                    },
                }
            }
        }
    }
}

/// Returns whether REPL command-local arguments request help output.
///
/// Inputs:
/// - `args`: command-local arguments after the `repl` verb.
///
/// Output:
/// - `true` when the invocation is exactly `--help` or `-h`.
/// - `false` for seed paths, empty args, or malformed argument lists.
///
/// Transformation:
/// - Performs an exact single-argument match with no filesystem access and no
///   interactive loop side effects.
fn is_repl_help_args(args: &[String]) -> bool {
    matches!(args, [arg] if matches!(arg.as_str(), "--help" | "-h"))
}

/// Prints REPL command help.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Writes REPL usage, source-entry rules, and control commands to stdout.
///
/// Transformation:
/// - Emits user-facing help text without mutating REPL session state.
fn print_repl_help() {
    println!("terlc repl [--help|-h] [<file.terl|project-dir>]");
    println!("Interactive mode accepts normal Terlan entries terminated with '.'.");
    println!("Available commands: :help, :quit, :reset, :load <file.terl|project-dir>");
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

/// One persistent value binding entered in the REPL.
///
/// Inputs:
/// - Constructed from `let name = expr.` REPL entries.
///
/// Output:
/// - Binding name and source expression used to rebuild later REPL entries.
///
/// Transformation:
/// - Keeps user-entered source available so each later expression can go
///   through the normal parser, typechecker, and CoreIR lowering path.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReplValueBinding {
    name: String,
    value: String,
}

/// Parses the REPL-only persistent value binding form.
///
/// Inputs:
/// - `entry`: terminator-stripped REPL source entry.
///
/// Output:
/// - Parsed binding when the entry has shape `let name = expr`.
/// - `None` for ordinary Terlan expressions/declarations.
///
/// Transformation:
/// - Recognizes a single simple binding without treating full source `let`
///   expressions as declarations. The right-hand expression is validated later
///   through the formal compiler path before the binding is persisted.
fn parse_repl_value_binding(entry: &str) -> Option<ReplValueBinding> {
    let rest = entry.trim().strip_prefix("let ")?;
    if rest.contains(';') {
        return None;
    }
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if !is_repl_binding_name(name) {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(ReplValueBinding {
        name: name.to_string(),
        value: value.to_string(),
    })
}

/// Checks whether a REPL value-binding name is a valid simple binding.
///
/// Inputs:
/// - `name`: candidate binding name.
///
/// Output:
/// - `true` when the name is a lower-case Terlan value binding.
///
/// Transformation:
/// - Applies the lightweight lexical rule needed before the formal parser
///   validates the generated let expression.
fn is_repl_binding_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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
                emit_repl_result(diagnostic_format, "Unit");
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
    let source_path = Path::new(path);
    let sources = match repl_load_sources(source_path) {
        Ok(sources) => sources,
        Err(message) => {
            emit_repl_event(
                diagnostic_format,
                "error",
                Some(&format!(
                    "\"message\":{},\"path\":{}",
                    json_string(&message),
                    json_string(path),
                )),
                &message,
            );
            return Err(ExitCode::from(1));
        }
    };

    let mut declarations = Vec::new();
    for (path, source) in sources {
        match parse_syntax_module(&source) {
            Ok(syntax_module) => {
                declarations.extend(repl_declaration_sources(
                    &source,
                    &syntax_module.declarations,
                ));
            }
            Err((message, start, end)) => {
                crate::support::emit_diagnostic(
                    "parse_error",
                    &message,
                    &path,
                    start,
                    end,
                    diagnostic_format,
                );
                return Err(ExitCode::from(1));
            }
        }
    }
    Ok(declarations)
}

/// Loads REPL source files from a file path or project directory.
///
/// Inputs:
/// - `path`: user supplied `terlc repl path` or `:load path` value.
///
/// Output:
/// - Ordered `(path, source)` pairs for `.terl` files to add to the session.
///
/// Transformation:
/// - Applies the 0.0.3 load contract: a file path loads exactly that file; a
///   directory path must contain `terlan.toml` and loads `.terl` files only from
///   manifest-declared source roots in deterministic path order.
fn repl_load_sources(path: &Path) -> Result<Vec<(String, String)>, String> {
    if path.is_file() {
        if path.extension().and_then(|value| value.to_str()) != Some("terl") {
            return Err("REPL source-file loads require a .terl file".to_string());
        }
        let source = fs::read_to_string(path)
            .map_err(|err| format!("failed to load REPL source file: {err}"))?;
        return Ok(vec![(path.display().to_string(), source)]);
    }

    if !path.is_dir() {
        return Err("REPL load path must be a .terl file or project directory".to_string());
    }

    let manifest = path.join("terlan.toml");
    if !manifest.is_file() {
        return Err("REPL project directory loads require terlan.toml".to_string());
    }

    let manifest = crate::commands::build::project_manifest::read_project_manifest(&manifest)
        .map_err(|err| format!("failed to read REPL project manifest: {err}"))?;
    let mut files = Vec::new();
    for root in &manifest.source_roots {
        let root_path = path.join(root);
        if !root_path.is_dir() {
            return Err(format!(
                "REPL project source root `{}` does not exist or is not a directory",
                root
            ));
        }
        collect_terl_files(&root_path, &mut files)?;
    }
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let source = fs::read_to_string(&file)
                .map_err(|err| format!("failed to load REPL project source: {err}"))?;
            Ok((file.display().to_string(), source))
        })
        .collect()
}

/// Collects Terlan source files below a directory.
///
/// Inputs:
/// - `dir`: directory to traverse.
/// - `files`: output accumulator for discovered `.terl` files.
///
/// Output:
/// - `Ok(())` when traversal succeeds; otherwise filesystem error text.
///
/// Transformation:
/// - Recursively walks the directory tree and records only `.terl` paths without
///   reading their contents.
fn collect_terl_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|err| format!("failed to read REPL project: {err}"))? {
        let entry = entry.map_err(|err| format!("failed to read REPL project entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_terl_files(&path, files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("terl") {
            files.push(path);
        }
    }
    Ok(())
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
                run_repl_expression_with_output(
                    binding.value.as_str(),
                    &declarations,
                    &value_bindings,
                    &module_name,
                    &run_name,
                    &temp_dir,
                    diagnostic_format,
                    native_policy,
                    target_profile,
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
) -> Result<String, String> {
    let mut output = |value: &str| match diagnostic_format {
        DiagnosticFormat::Text { .. } => println!("{value}"),
        DiagnosticFormat::Json => emit_repl_event(
            DiagnosticFormat::Json,
            "stdout",
            Some(&format!(
                "\"stream\":\"stdout\",\"value\":{}",
                json_string(value)
            )),
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
/// - Builds a synthetic module, compiles it through formal phases, and executes
///   selected CoreIR directly while routing console effects through `output`.
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

    evaluator::evaluate_repl_function_with_output(&compiled.core, run_name, output)
        .map(|value| value.render())
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
        compile.derive_expansion_diagnostics.as_slice(),
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

/// Builds the generated expression body for one REPL evaluation.
///
/// Inputs:
/// - `expression`: current expression source.
/// - `value_bindings`: persisted REPL value bindings.
///
/// Output:
/// - Source expression that evaluates previous bindings before the current
///   expression.
///
/// Transformation:
/// - Converts REPL state into an ordinary Terlan `let` expression so parsing,
///   typechecking, CoreIR lowering, and evaluation stay on the normal compiler
///   path.
fn repl_expression_with_bindings(expression: &str, value_bindings: &[ReplValueBinding]) -> String {
    if value_bindings.is_empty() {
        return expression.to_string();
    }

    let bindings = value_bindings
        .iter()
        .map(|binding| format!("{} = {}", binding.name, binding.value))
        .collect::<Vec<_>>()
        .join("; ");
    format!("let {bindings}; {expression}")
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
            println!("{}", render_repl_json_event(kind, fields, text));
        }
    }
}

/// Renders one structured REPL event as a JSON object.
///
/// Inputs:
/// - `kind`: stable REPL event kind.
/// - `fields`: optional pre-rendered JSON object fields without surrounding
///   braces.
/// - `text`: human-readable event text.
///
/// Output:
/// - A single-line JSON object with schema, kind, optional fields, and text.
///
/// Transformation:
/// - Places optional fields between `kind` and `text` while preserving valid
///   comma separation for empty and non-empty field payloads.
fn render_repl_json_event(kind: &str, fields: Option<&str>, text: &str) -> String {
    let payload = fields.filter(|value| !value.is_empty()).unwrap_or("");
    if payload.is_empty() {
        format!(
            "{{\"schema\":\"terlan-repl-event-v1\",\"kind\":\"{}\",\"text\":{}}}",
            kind,
            json_string(text),
        )
    } else {
        format!(
            "{{\"schema\":\"terlan-repl-event-v1\",\"kind\":\"{}\",{},\"text\":{}}}",
            kind,
            payload,
            json_string(text),
        )
    }
}

/// Emits a successful REPL expression result.
///
/// Inputs:
/// - `diagnostic_format`: output mode selected by global flags.
/// - `value`: rendered Terlan value from expression execution.
///
/// Output:
/// - No return value; writes a result event.
///
/// Transformation:
/// - Normalizes empty output to `Unit` and otherwise includes the rendered value.
fn emit_repl_result(diagnostic_format: DiagnosticFormat, value: &str) {
    if value.trim().is_empty() {
        emit_repl_event(
            diagnostic_format,
            "result",
            Some("\"value\":\"Unit\""),
            "Unit",
        );
    } else {
        emit_repl_event(
            diagnostic_format,
            "result",
            Some(&format!("\"value\":{}", json_string(value))),
            value,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use super::{
        is_repl_help_args, parse_repl_value_binding, render_repl_json_event,
        repl_expression_with_bindings, repl_load_sources, ReplValueBinding,
    };

    /// Verifies REPL command-local help aliases are recognized.
    ///
    /// Inputs:
    /// - Synthetic command-local argument vectors for `--help` and `-h`.
    ///
    /// Output:
    /// - Test assertions only; no files are read or written.
    ///
    /// Transformation:
    /// - Exercises the REPL help detector without starting the interactive
    ///   command loop.
    #[test]
    fn repl_help_args_accept_long_and_short_help() {
        assert!(is_repl_help_args(&["--help".to_string()]));
        assert!(is_repl_help_args(&["-h".to_string()]));
    }

    /// Verifies REPL help detection does not consume seed paths.
    ///
    /// Inputs:
    /// - Synthetic command-local argument vectors for empty args, one seed
    ///   path, and malformed extra arguments.
    ///
    /// Output:
    /// - Test assertions only; no files are read or written.
    ///
    /// Transformation:
    /// - Keeps REPL help routing exact so normal seed loading and malformed
    ///   argument diagnostics remain owned by the main command path.
    #[test]
    fn repl_help_args_reject_non_help_invocations() {
        assert!(!is_repl_help_args(&[]));
        assert!(!is_repl_help_args(&["src/main.terl".to_string()]));
        assert!(!is_repl_help_args(&[
            "--help".to_string(),
            "src/main.terl".to_string()
        ]));
    }

    /// Verifies that REPL binding syntax captures a simple persistent name.
    ///
    /// Inputs:
    /// - A terminator-stripped REPL entry with shape `let name = expr`.
    ///
    /// Output:
    /// - Parsed name and value expression.
    ///
    /// Transformation:
    /// - Exercises the REPL-only binding parser without invoking the full
    ///   interactive command loop.
    #[test]
    fn repl_value_binding_parser_accepts_simple_binding() {
        let binding = parse_repl_value_binding("let total = 1 + 2").unwrap();

        assert_eq!(binding.name, "total");
        assert_eq!(binding.value, "1 + 2");
    }

    /// Verifies that full Terlan `let` expressions are left to source parsing.
    ///
    /// Inputs:
    /// - A terminator-stripped source `let` expression containing `;`.
    ///
    /// Output:
    /// - `None`, indicating the REPL did not treat the entry as persistent
    ///   session state.
    ///
    /// Transformation:
    /// - Keeps the REPL-only `let name = expr.` entry form separate from normal
    ///   Terlan let expressions such as `let x = 1; x + 1`.
    #[test]
    fn repl_value_binding_parser_rejects_source_let_expression() {
        assert!(parse_repl_value_binding("let x = 1; x + 1").is_none());
    }

    /// Verifies that persisted bindings lower through ordinary Terlan `let`.
    ///
    /// Inputs:
    /// - Current expression source and two persisted REPL value bindings.
    ///
    /// Output:
    /// - Generated source expression with persisted bindings evaluated first.
    ///
    /// Transformation:
    /// - Converts REPL state into the compiler-owned source form used before
    ///   parsing, typechecking, CoreIR lowering, and evaluator execution.
    #[test]
    fn repl_expression_with_bindings_builds_source_let_expression() {
        let expression = repl_expression_with_bindings(
            "x + y",
            &[
                ReplValueBinding {
                    name: "x".to_string(),
                    value: "1".to_string(),
                },
                ReplValueBinding {
                    name: "y".to_string(),
                    value: "2".to_string(),
                },
            ],
        );

        assert_eq!(expression, "let x = 1; y = 2; x + y");
    }

    /// Verifies JSON REPL events are valid without optional fields.
    ///
    /// Inputs:
    /// - Event kind and text without extra field payload.
    ///
    /// Output:
    /// - Parsed JSON with schema, kind, and text fields.
    ///
    /// Transformation:
    /// - Renders the event through the same helper used by the REPL command and
    ///   parses it back through `serde_json`.
    #[test]
    fn repl_json_event_without_extra_fields_is_valid_json() {
        let event = render_repl_json_event("ready", None, "REPL ready");
        let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

        assert_eq!(value["schema"], "terlan-repl-event-v1");
        assert_eq!(value["kind"], "ready");
        assert_eq!(value["text"], "REPL ready");
    }

    /// Verifies JSON REPL events are valid with optional fields.
    ///
    /// Inputs:
    /// - Event kind, field payload, and human-readable text.
    ///
    /// Output:
    /// - Parsed JSON containing both the payload field and text field.
    ///
    /// Transformation:
    /// - Confirms optional field insertion preserves comma separation before
    ///   the final `text` property.
    #[test]
    fn repl_json_event_with_extra_fields_is_valid_json() {
        let event = render_repl_json_event(
            "result",
            Some("\"value\":\"Unit\",\"message\":\"ok\""),
            "Unit",
        );
        let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

        assert_eq!(value["schema"], "terlan-repl-event-v1");
        assert_eq!(value["kind"], "result");
        assert_eq!(value["value"], "Unit");
        assert_eq!(value["message"], "ok");
        assert_eq!(value["text"], "Unit");
    }

    /// Verifies project loads follow manifest source roots.
    ///
    /// Inputs:
    /// - A temporary project with `src` and `lib` source roots plus unrelated
    ///   `.terl` files outside those roots.
    ///
    /// Output:
    /// - Loaded source paths from `src` and `lib` only.
    ///
    /// Transformation:
    /// - Reads `terlan.toml`, resolves `[build] source_roots`, recursively
    ///   collects Terlan files under those roots, and ignores unrelated project
    ///   directories such as `_build`.
    #[test]
    fn repl_load_sources_uses_project_manifest_source_roots() {
        let root = make_repl_test_dir("manifest_source_roots");
        fs::create_dir_all(root.join("src/app")).expect("create src");
        fs::create_dir_all(root.join("lib/app")).expect("create lib");
        fs::create_dir_all(root.join("_build/src")).expect("create ignored build dir");
        fs::create_dir_all(root.join("misc")).expect("create ignored misc dir");
        fs::write(
            root.join("terlan.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"lib\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("write manifest");
        fs::write(root.join("src/app/Main.terl"), "module app.Main.\n").expect("write src");
        fs::write(root.join("lib/app/Util.terl"), "module app.Util.\n").expect("write lib");
        fs::write(
            root.join("_build/src/generated.terl"),
            "module ignored.Generated.\n",
        )
        .expect("write ignored build source");
        fs::write(root.join("misc/Other.terl"), "module ignored.Other.\n")
            .expect("write ignored misc source");

        let sources = repl_load_sources(&root).expect("load project sources");
        let paths = sources
            .iter()
            .map(|(path, _)| path.replace('\\', "/"))
            .collect::<Vec<_>>();

        assert_eq!(sources.len(), 2);
        assert!(paths.iter().any(|path| path.ends_with("lib/app/Util.terl")));
        assert!(paths.iter().any(|path| path.ends_with("src/app/Main.terl")));
        assert!(!paths.iter().any(|path| path.contains("_build")));
        assert!(!paths.iter().any(|path| path.contains("/misc/")));

        fs::remove_dir_all(root).expect("remove test project");
    }

    /// Creates a unique temporary directory for REPL unit tests.
    ///
    /// Inputs:
    /// - `label`: stable readable prefix for the test directory name.
    ///
    /// Output:
    /// - Path to a newly created directory under the OS temporary directory.
    ///
    /// Transformation:
    /// - Combines the label, process id, and current time to avoid collisions,
    ///   removes any stale directory with that exact name, then creates it.
    fn make_repl_test_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "terlan_repl_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create repl test dir");
        path
    }
}
