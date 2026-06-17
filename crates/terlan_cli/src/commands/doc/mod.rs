use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
/// - Replaces the five special HTML text characters with entities.
fn escape_doc_html_text(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::UNIX_EPOCH;

    use super::run;
    use crate::{CliCommand, CliState};

    /// Verifies `doc --check` accepts matching REPL examples.
    ///
    /// Inputs:
    /// - Temporary Terlan source file with one runnable `@example` prompt.
    ///
    /// Output:
    /// - Successful command exit code.
    ///
    /// Transformation:
    /// - Runs the public doc command path so source parsing, documentation
    ///   validation, REPL prompt extraction, REPL execution, and output
    ///   comparison are all exercised together.
    #[test]
    fn doc_check_accepts_matching_repl_example() {
        let dir = make_doc_command_test_dir("matching_repl_example");
        let path = dir.join("DocExample.terl");
        fs::write(
            &path,
            r#"module doc_examples.

/**
 * Adds numbers.
 *
 * @example
 * > 1 + 2.
 * 3
 */
pub add(x: Int): Int ->
    x + 1.
"#,
        )
        .expect("write source");

        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec![path.to_string_lossy().to_string(), "--check".to_string()],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        fs::remove_dir_all(dir).expect("remove test dir");
    }

    /// Verifies `doc --check` rejects mismatched REPL examples.
    ///
    /// Inputs:
    /// - Temporary Terlan source file with one runnable `@example` prompt whose
    ///   expected output is wrong.
    ///
    /// Output:
    /// - Failing command exit code.
    ///
    /// Transformation:
    /// - Runs the public doc command path and confirms the REPL-backed doctest
    ///   gate fails before documentation output is written.
    #[test]
    fn doc_check_rejects_mismatched_repl_example() {
        let dir = make_doc_command_test_dir("mismatched_repl_example");
        let path = dir.join("DocExample.terl");
        fs::write(
            &path,
            r#"module doc_examples.

/**
 * Adds numbers.
 *
 * @example
 * > 1 + 2.
 * 4
 */
pub add(x: Int): Int ->
    x + 1.
"#,
        )
        .expect("write source");

        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec![path.to_string_lossy().to_string(), "--check".to_string()],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::from(1));
        fs::remove_dir_all(dir).expect("remove test dir");
    }

    /// Verifies `doc --format json` writes a compiler-owned JSON model.
    ///
    /// Inputs:
    /// - Temporary Terlan source file with one documented public function.
    ///
    /// Output:
    /// - Successful command exit code and parseable JSON documentation output.
    ///
    /// Transformation:
    /// - Runs the public doc command path with JSON format and parses the
    ///   generated artifact as the initial Terlan documentation model.
    #[test]
    fn doc_command_writes_json_model() {
        let dir = make_doc_command_test_dir("json_model");
        let path = dir.join("DocExample.terl");
        let out_dir = dir.join("docs");
        fs::write(
            &path,
            r#"module doc_examples.

/**
 * Adds numbers.
 */
pub add(x: Int): Int ->
    x + 1.
"#,
        )
        .expect("write source");

        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec![path.to_string_lossy().to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                doc_format: crate::DocFormat::Json,
                ..CliState::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let module_json =
            fs::read_to_string(out_dir.join("doc_examples.json")).expect("read module json docs");
        let module_value: serde_json::Value =
            serde_json::from_str(&module_json).expect("parse module docs json");
        assert_eq!(module_value["schema"], "terlan-doc-module-v1");
        assert_eq!(module_value["declarations"][0]["name"], "add");
        let project_json =
            fs::read_to_string(out_dir.join("model.json")).expect("read project docs json");
        let project_value: serde_json::Value =
            serde_json::from_str(&project_json).expect("parse project docs json");
        assert_eq!(project_value["schema"], "terlan-doc-project-v1");
        assert_eq!(project_value["modules"][0]["module"], "doc_examples");
        fs::remove_dir_all(dir).expect("remove test dir");
    }

    /// Verifies `doc` writes default HTML documentation with an aggregate index.
    ///
    /// Inputs:
    /// - Temporary Terlan source file with one documented public function.
    ///
    /// Output:
    /// - Successful command exit code, module HTML page, and `index.html`.
    ///
    /// Transformation:
    /// - Runs the public doc command path with the default HTML format and
    ///   validates the generated static documentation entry point links to the
    ///   module page.
    #[test]
    fn doc_command_defaults_to_html_index() {
        let dir = make_doc_command_test_dir("html_index");
        let path = dir.join("DocExample.terl");
        let out_dir = dir.join("docs");
        fs::write(
            &path,
            r#"module std.core.DocExample.

/**
 * Adds numbers.
 */
pub add(x: Int): Int ->
    x + 1.
"#,
        )
        .expect("write source");

        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec![path.to_string_lossy().to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..CliState::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let module_html =
            fs::read_to_string(out_dir.join("std.core.DocExample.html")).expect("read module html");
        assert!(module_html.contains("std.core.DocExample documentation"));
        assert!(module_html.contains("Functions"));
        let index_html = fs::read_to_string(out_dir.join("index.html")).expect("read index html");
        assert!(index_html.contains("Terlan documentation"));
        assert!(index_html.contains("std.core"));
        assert!(index_html.contains("std.core.DocExample.html"));
        fs::remove_dir_all(dir).expect("remove test dir");
    }

    /// Verifies `doc std --check` validates the public stdlib documentation.
    ///
    /// Inputs:
    /// - The scratch workspace `std` source tree.
    ///
    /// Output:
    /// - Successful command exit code.
    ///
    /// Transformation:
    /// - Runs the public documentation validation path over the release-owned
    ///   standard-library source modules without writing output artifacts.
    #[test]
    fn doc_check_accepts_std_reference() {
        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec!["std".to_string(), "--check".to_string()],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies `doc std` generates a navigable stdlib HTML reference.
    ///
    /// Inputs:
    /// - The scratch workspace `std` source tree and a temporary output
    ///   directory.
    ///
    /// Output:
    /// - Successful command exit code plus generated index and module pages.
    ///
    /// Transformation:
    /// - Runs the public documentation generation path over stdlib source and
    ///   confirms representative 0.0.3 public modules are linked and rendered.
    #[test]
    fn doc_command_generates_std_html_reference() {
        let dir = make_doc_command_test_dir("std_html_reference");
        let out_dir = dir.join("docs");

        let exit = run(
            CliCommand {
                verb: Some("doc".to_string()),
                args: vec!["std".to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..CliState::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let index_html = fs::read_to_string(out_dir.join("index.html")).expect("read index html");
        assert!(index_html.contains("std.core"));
        assert!(index_html.contains("std.collections"));
        assert!(index_html.contains("std.core.String.html"));
        assert!(index_html.contains("std.collections.Map.html"));
        assert!(out_dir.join("std.core.String.html").exists());
        assert!(out_dir.join("std.collections.Map.html").exists());
        fs::remove_dir_all(dir).expect("remove test dir");
    }

    /// Creates a unique temporary directory for doc command tests.
    ///
    /// Inputs:
    /// - `label`: readable test label included in the directory name.
    ///
    /// Output:
    /// - Created temporary directory path.
    ///
    /// Transformation:
    /// - Combines the label, process id, and clock time to avoid collisions,
    ///   then recreates the directory under the OS temporary directory.
    fn make_doc_command_test_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "terlan_doc_command_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create doc command test dir");
        path
    }
}
