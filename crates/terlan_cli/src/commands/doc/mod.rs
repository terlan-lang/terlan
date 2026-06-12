use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::{CliCommand, CliState, DocFormat};

mod render;
mod validation;
pub(crate) use render::*;
pub(crate) use validation::*;

/// Executes the `doc` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing documentation command-local
///   arguments.
/// - `state`: parsed global CLI state, including output directory, incremental
///   mode, diagnostic format, and documentation format.
///
/// Output:
/// - `ExitCode::SUCCESS` when documentation validates and, unless `--check` is
///   set, output files are written.
/// - `ExitCode::from(2)` for malformed command arguments.
/// - `ExitCode::from(1)` for source discovery, read, parse, validation, output
///   directory, or write failures.
///
/// Transformation:
/// - Parses command-local flags, discovers source files, validates doc links,
///   fences, and optional missing docs, then renders Markdown or HTML output.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let (input_path, check_only, missing_docs) = match parse_doc_args(&cmd.args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    let input = Path::new(input_path);
    let files = match doc_sources(input) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };

    if !check_only {
        if let Err(err) = fs::create_dir_all(&state.out_dir) {
            eprintln!("cannot create output directory: {}", err);
            return ExitCode::from(1);
        }
    }

    for file in files {
        let path_text = file.to_string_lossy().to_string();
        let source = match crate::support::read_file(&path_text) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let syntax_output =
            match crate::formal_pipeline::parse_source_as_syntax_output(&path_text, &source) {
                Ok(syntax_output) => syntax_output,
                Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
                    crate::support::emit_diagnostic(
                        "parse_error",
                        &message,
                        &path_text,
                        span.start,
                        span.end,
                        state.diagnostic_format,
                    );
                    return ExitCode::from(1);
                }
                Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            };
        if let Err(err) = validate_syntax_module_doc_links(&syntax_output, &source) {
            crate::support::emit_diagnostic(
                "doc_error",
                &format!("broken intra-doc link `{}`", err.target),
                &path_text,
                err.offset,
                err.offset + err.target.len(),
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
        if let Err(err) = validate_syntax_module_doc_fences(&syntax_output, &source) {
            crate::support::emit_diagnostic(
                "doc_error",
                &err.message,
                &path_text,
                err.offset,
                err.offset + err.len,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
        if missing_docs {
            if let Err(err) = validate_syntax_missing_docs(&syntax_output) {
                crate::support::emit_diagnostic(
                    "doc_error",
                    &err.message,
                    &path_text,
                    err.offset,
                    err.offset + err.len,
                    state.diagnostic_format,
                );
                return ExitCode::from(1);
            }
        }
        let (contents, extension) = match state.doc_format {
            DocFormat::Markdown => (render_syntax_module_docs_markdown(&syntax_output), "md"),
            DocFormat::Html => (render_syntax_module_docs_html(&syntax_output), "html"),
        };
        if check_only {
            continue;
        }
        let target = state
            .out_dir
            .join(format!("{}.{}", syntax_output.module_name, extension));
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            contents.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write documentation output: {}", err);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Parses command-local flags for `doc`.
///
/// Inputs:
/// - `args`: arguments after the `doc` verb.
///
/// Output:
/// - Source path, check-only flag, and missing-docs validation flag.
/// - `Err(String)` for missing, duplicate, or unexpected arguments.
///
/// Transformation:
/// - Treats the single non-flag argument as the source path and recognizes
///   `--check` plus `--missing-docs`.
pub(crate) fn parse_doc_args(args: &[String]) -> Result<(&str, bool, bool), String> {
    let mut path = None;
    let mut check_only = false;
    let mut missing_docs = false;

    for arg in args {
        if arg == "--check" {
            check_only = true;
            continue;
        }
        if arg == "--missing-docs" {
            missing_docs = true;
            continue;
        }
        if arg.starts_with("--") {
            return Err(format!("unexpected doc argument: {}", arg));
        }
        if path.replace(arg.as_str()).is_some() {
            return Err("missing or extra path argument".to_string());
        }
    }

    match path {
        Some(path) => Ok((path, check_only, missing_docs)),
        None => Err("missing or extra path argument".to_string()),
    }
}

/// Executes the `doctest` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing one source path argument.
/// - `state`: parsed global CLI state, including diagnostic format.
///
/// Output:
/// - `ExitCode::SUCCESS` when doc fences compile.
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for read, parse, doc validation, or doctest compile
///   failures.
///
/// Transformation:
/// - Reads and parses one source module, validates doc links/fences, compiles
///   Terlan doctest fences, and emits diagnostics for failures.
pub(crate) fn run_doctest(cmd: CliCommand, state: CliState) -> ExitCode {
    if cmd.args.len() != 1 {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path = &cmd.args[0];
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let syntax_output = match crate::formal_pipeline::parse_source_as_syntax_output(path, &source) {
        Ok(syntax_output) => syntax_output,
        Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                path,
                span.start,
                span.end,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
        Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(err) = validate_syntax_module_doc_links(&syntax_output, &source) {
        crate::support::emit_diagnostic(
            "doc_error",
            &format!("broken intra-doc link `{}`", err.target),
            path,
            err.offset,
            err.offset + err.target.len(),
            state.diagnostic_format,
        );
        return ExitCode::from(1);
    }
    if let Err(err) = validate_syntax_module_doc_fences(&syntax_output, &source) {
        crate::support::emit_diagnostic(
            "doc_error",
            &err.message,
            path,
            err.offset,
            err.offset + err.len,
            state.diagnostic_format,
        );
        return ExitCode::from(1);
    }
    if let Err(err) = compile_syntax_terlan_doctests(&syntax_output, &source, path) {
        crate::support::emit_diagnostic(
            "doctest_error",
            &err.message,
            path,
            err.offset,
            err.offset + err.len,
            state.diagnostic_format,
        );
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
