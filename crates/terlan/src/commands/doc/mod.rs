use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_html::escape_html_text;
use crate::{CliCommand, CliState, DocFormat};

mod render;
mod validation;
pub(crate) use render::*;
pub(crate) use validation::*;

/// Rendered documentation module metadata used for aggregate outputs.
///
/// Inputs:
/// - Produced while rendering one source module.
///
/// Output:
/// - Module name and generated artifact file name.
///
/// Transformation:
/// - Carries enough stable metadata for index/model generation after per-file
///   rendering succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedDocModule {
    module_name: String,
    file_name: String,
}

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
///   fences, optional missing docs, and REPL example extraction, then renders
///   Markdown or HTML output.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let (input_path, check_only, missing_docs) = match parse_doc_args(&cmd.args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    let input = resolve_doc_input_path(input_path);
    let files = match doc_sources(&input) {
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

    let mut json_modules = Vec::new();
    let mut html_modules = Vec::new();
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
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
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
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
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
        if check_only {
            if let Err(err) = validate_repl_doc_examples(
                &syntax_output,
                &source,
                state.diagnostic_format,
                state.native_policy,
                state.target_profile,
            ) {
                crate::support::emit_diagnostic(
                    "doctest_error",
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
            DocFormat::Json => (render_syntax_module_docs_json(&syntax_output), "json"),
        };
        if matches!(state.doc_format, DocFormat::Json) {
            json_modules.push(contents.trim().to_string());
        }
        if matches!(state.doc_format, DocFormat::Html) {
            html_modules.push(RenderedDocModule {
                module_name: syntax_output.module_name.clone(),
                file_name: format!("{}.{}", syntax_output.module_name, extension),
            });
        }
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

    if !check_only && matches!(state.doc_format, DocFormat::Html) {
        let target = state.out_dir.join("index.html");
        let contents = render_project_docs_html_index(&html_modules);
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            contents.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write documentation index: {}", err);
            return ExitCode::from(1);
        }
    }

    if !check_only && matches!(state.doc_format, DocFormat::Json) {
        let target = state.out_dir.join("model.json");
        let contents = render_project_docs_json_model(&json_modules);
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            contents.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write documentation model: {}", err);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Resolves command-local documentation input aliases.
///
/// Inputs:
/// - `input_path`: path-like argument passed after `terlc doc`.
///
/// Output:
/// - Borrowed filesystem path for normal inputs.
/// - Workspace stdlib source path for the well-known `std` alias when the
///   current directory does not contain `std`.
///
/// Transformation:
/// - Keeps user paths unchanged while making `terlc doc std` stable from
///   command tests and developer workspaces where the current directory is the
///   CLI crate rather than the repository root.
fn resolve_doc_input_path(input_path: &str) -> PathBuf {
    if input_path == "std" {
        let local_std = Path::new("std");
        if local_std.exists() {
            return local_std.to_path_buf();
        }
        let workspace_std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        if workspace_std.exists() {
            return workspace_std;
        }
    }
    PathBuf::from(input_path)
}

/// Renders the aggregate static HTML documentation index.
///
/// Inputs:
/// - `modules`: rendered module metadata collected during documentation output.
///
/// Output:
/// - Static HTML index linking to generated module pages.
///
/// Transformation:
/// - Builds a deterministic, target-neutral index page without relying on an
///   external documentation generator.
fn render_project_docs_html_index(modules: &[RenderedDocModule]) -> String {
    let mut package_modules: BTreeMap<String, Vec<&RenderedDocModule>> = BTreeMap::new();
    for module in modules {
        package_modules
            .entry(doc_package_name(&module.module_name))
            .or_default()
            .push(module);
    }
    let sections = package_modules
        .iter()
        .map(|(package, modules)| render_project_docs_html_package(package, modules))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Terlan documentation</title>\n<style>\n{}\n</style>\n</head>\n<body>\n<main class=\"doc-index\"><p class=\"doc-kicker\">Terlan reference</p><h1>Terlan documentation</h1><p class=\"doc-intro\">Generated public API reference for documented Terlan modules.</p>{}</main>\n</body>\n</html>\n",
        project_doc_html_styles(),
        sections
    )
}

/// Renders one package group in the aggregate documentation index.
///
/// Inputs:
/// - `package`: package name derived from module names.
/// - `modules`: module metadata entries belonging to that package.
///
/// Output:
/// - HTML package section containing links to module pages.
///
/// Transformation:
/// - Sorts modules by generation order within the deterministic package map
///   and emits a readable package card.
fn render_project_docs_html_package(package: &str, modules: &[&RenderedDocModule]) -> String {
    let links = modules
        .iter()
        .map(|module| {
            format!(
                "<li><a href=\"{}\"><code>{}</code></a></li>",
                escape_doc_html_text(&module.file_name),
                escape_doc_html_text(&module.module_name),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<section class=\"doc-package\"><h2>{}</h2><ul>{}</ul></section>",
        escape_doc_html_text(package),
        links
    )
}

/// Derives a package name from a Terlan module name.
///
/// Inputs:
/// - `module_name`: dot-separated module name.
///
/// Output:
/// - Parent package for nested modules.
/// - The module name itself when no parent segment exists.
///
/// Transformation:
/// - Removes the final dot segment so `std.core.String` groups under
///   `std.core`.
fn doc_package_name(module_name: &str) -> String {
    module_name.rsplit_once('.').map_or_else(
        || module_name.to_string(),
        |(package, _)| package.to_string(),
    )
}

/// Returns shared CSS for the generated documentation index.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static CSS stylesheet text.
///
/// Transformation:
/// - Encodes a dependency-free index layout directly in the CLI renderer.
fn project_doc_html_styles() -> &'static str {
    "body{margin:0;background:#f8fafc;color:#172033;font:16px/1.55 system-ui,-apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif}.doc-index{max-width:980px;margin:0 auto;padding:2.5rem 1.25rem}.doc-kicker{margin:0;text-transform:uppercase;font-size:.75rem;letter-spacing:.08em;color:#2563eb}.doc-index h1{margin:.2rem 0;font-size:2.25rem}.doc-intro{color:#475569;max-width:680px}.doc-package{background:white;border:1px solid #d7dee9;border-radius:8px;margin:1rem 0;padding:1rem 1.25rem}.doc-package h2{font-size:1rem;text-transform:uppercase;color:#475569;letter-spacing:.06em}.doc-package ul{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:.5rem 1rem;list-style:none;margin:0;padding:0}.doc-package a{color:#1d4ed8;text-decoration:none}.doc-package a:hover{text-decoration:underline}code{font-family:\"SFMono-Regular\",Consolas,\"Liberation Mono\",monospace}"
}

/// Escapes text for generated documentation HTML.
///
/// Inputs:
/// - `input`: raw link or label text.
///
/// Output:
/// - HTML-escaped text.
///
/// Transformation:
/// - Delegates text escaping to `terlan_html` so the project documentation
///   index and module documentation pages share one escaping boundary.
fn escape_doc_html_text(input: &str) -> String {
    escape_html_text(input)
}

/// Renders the aggregate project documentation JSON model.
///
/// Inputs:
/// - `modules`: per-module JSON object strings already rendered from syntax
///   output.
///
/// Output:
/// - Project-level JSON model containing all module models.
///
/// Transformation:
/// - Wraps deterministic module JSON objects in the `terlan-doc-project-v1`
///   schema used by `terlc doc --format json`.
fn render_project_docs_json_model(modules: &[String]) -> String {
    format!(
        "{{\"schema\":\"terlan-doc-project-v1\",\"modules\":[{}]}}\n",
        modules.join(",")
    )
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
        Err(crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
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
        Err(crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
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

#[cfg(test)]
#[path = "doc_test.rs"]
mod doc_test;
