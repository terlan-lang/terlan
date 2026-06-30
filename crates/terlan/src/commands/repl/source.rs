use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::terlan_syntax::{
    parse_module_as_syntax_output, EbnfCompileError, SyntaxDeclarationOutput, SyntaxModuleOutput,
};
use crate::DiagnosticFormat;

use super::event::{emit_repl_event, emit_repl_result, repl_json_field};

/// Parses a REPL declaration entry and emits diagnostics or success output.
///
/// Inputs:
/// - `module_name`: synthetic REPL module used to wrap the declaration.
/// - `declaration`: user-entered declaration source.
/// - `diagnostic_format`: text or JSON diagnostic output mode.
/// - `declarations`: persistent REPL declaration accumulator.
/// - `expr_parse_error`: optional expression-parser failure to report before
///   declaration parse diagnostics.
///
/// Output:
/// - None; parsed declarations are appended and diagnostics/results are printed.
///
/// Transformation:
/// - Rewraps the entry as a temporary module, stores source-spanned
///   declarations, and reports both expression and declaration parse failures
///   so ambiguous REPL entries remain debuggable.
pub(super) fn parse_repl_declaration_and_log(
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

/// Parses a complete Terlan module into syntax output for REPL loading.
///
/// Inputs:
/// - `source`: complete module source text.
///
/// Output:
/// - Parsed `SyntaxModuleOutput` on success.
/// - Diagnostic message and byte span on parse or serialization failure.
///
/// Transformation:
/// - Adapts the syntax crate error shape into the compact REPL diagnostic tuple.
fn parse_syntax_module(source: &str) -> Result<SyntaxModuleOutput, (String, usize, usize)> {
    match parse_module_as_syntax_output(source) {
        Ok(module) => Ok(module),
        Err(EbnfCompileError::Parse(message, span)) => Err((message, span.start, span.end)),
        Err(EbnfCompileError::Serialize(message)) => Err((message, 0, 0)),
    }
}

/// Rebuilds a synthetic module from persistent REPL declarations.
///
/// Inputs:
/// - `module_name`: generated REPL module name.
/// - `declarations`: source snippets already accepted into the session.
///
/// Output:
/// - Complete module source text.
///
/// Transformation:
/// - Prepends the module declaration and joins persisted declarations with
///   stable spacing so later expression evaluation can use the normal compiler.
pub(super) fn repl_declarations_to_source(module_name: &str, declarations: &[String]) -> String {
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

/// Extracts declaration source slices from parsed syntax declarations.
///
/// Inputs:
/// - `source`: original module source containing the declarations.
/// - `declarations`: parsed declarations with source spans.
///
/// Output:
/// - Source snippets for declarations whose spans are valid.
///
/// Transformation:
/// - Uses parser-provided spans to preserve user declaration text without
///   pretty-printing or normalizing the source.
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

/// Parses one declaration-style REPL entry into persistent source snippets.
///
/// Inputs:
/// - `module_name`: generated module wrapper name.
/// - `declaration`: user-entered declaration source.
///
/// Output:
/// - Parsed declaration source snippets on success.
/// - Parse diagnostic tuple when the wrapped module is invalid or empty.
///
/// Transformation:
/// - Wraps the entry in a temporary module because the syntax parser accepts
///   complete modules, then extracts the declaration spans back out.
pub(super) fn parse_repl_declaration(
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

/// Loads declarations from a REPL seed file or project directory.
///
/// Inputs:
/// - `path`: `.terl` file or project directory path supplied to `terlc repl`
///   or `:load`.
/// - `diagnostic_format`: text or JSON output mode for load errors.
///
/// Output:
/// - Declaration source snippets to seed the REPL session.
/// - `ExitCode` when filesystem or parse diagnostics should abort loading.
///
/// Transformation:
/// - Expands the path into ordered Terlan sources, parses each complete module,
///   and stores only declaration source slices for later REPL evaluation.
pub(super) fn load_repl_seed_declarations(
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
                &[
                    repl_json_field("message", message.as_str()),
                    repl_json_field("path", path),
                ],
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
/// - Applies the REPL load contract: a file path loads exactly that file; a
///   directory path must contain `terlan.toml` and loads `.terl` files only from
///   manifest-declared source roots in deterministic path order.
pub(super) fn repl_load_sources(path: &Path) -> Result<Vec<(String, String)>, String> {
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
