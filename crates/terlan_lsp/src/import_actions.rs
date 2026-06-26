use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use terlan_hir::load_interfaces_from_file_set;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Position, Range, TextEdit, Url, WorkspaceEdit,
};

/// Auto-import edit candidate for one unresolved Terlan symbol.
///
/// Inputs:
/// - Compiler interface summaries and current source text.
///
/// Output:
/// - A source edit that imports or corrects the module exposing the symbol.
///
/// Transformation:
/// - Carries enough LSP data to render a quick-fix code action without
///   retaining compiler-specific interface values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportActionCandidate {
    title: String,
    edit: TextEdit,
}

impl ImportActionCandidate {
    /// Converts this candidate into an LSP code action.
    ///
    /// Inputs:
    /// - `uri`: document URI receiving the edit.
    ///
    /// Output:
    /// - One quick-fix `CodeAction`.
    ///
    /// Transformation:
    /// - Wraps the text edit in a single-file workspace edit so VS Code and
    ///   other LSP clients can apply it directly.
    pub(crate) fn into_code_action(self, uri: &Url) -> CodeAction {
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), vec![self.edit]);
        CodeAction {
            title: self.title,
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

/// Returns quick-fix actions for an unresolved-name diagnostic.
///
/// Inputs:
/// - `uri`: document URI used to discover visible interface summaries.
/// - `text`: current source text.
/// - `diagnostic_message`: compiler diagnostic message from the LSP client
///   request.
///
/// Output:
/// - Import quick-fix actions for recognized unresolved module/function names.
///
/// Transformation:
/// - Extracts the unresolved symbol from stable typechecker messages, scans
///   visible interfaces for modules/functions that expose that symbol, and
///   builds import insertion or same-leaf import replacement edits.
pub(crate) fn import_code_actions_for_diagnostic(
    uri: &Url,
    text: &str,
    diagnostic_message: &str,
) -> Vec<CodeAction> {
    let Some(symbol) = unresolved_symbol_from_diagnostic(diagnostic_message) else {
        return Vec::new();
    };
    import_candidates_for_symbol(uri, text, &symbol)
        .into_iter()
        .map(|candidate| candidate.into_code_action(uri))
        .collect()
}

/// Returns import edit candidates for one unresolved symbol.
///
/// Inputs:
/// - `uri`: document URI used for interface-summary discovery.
/// - `text`: current Terlan source.
/// - `symbol`: unresolved function, constructor, or module-like name.
///
/// Output:
/// - Stable, deduplicated import candidates.
///
/// Transformation:
/// - Converts visible module interfaces into module import candidates when the
///   module leaf or constructor matches `symbol`, and selective function
///   import candidates when a public function has the same name.
pub(crate) fn import_candidates_for_symbol(
    uri: &Url,
    text: &str,
    symbol: &str,
) -> Vec<ImportActionCandidate> {
    let file_path = source_path_for_uri(uri);
    let interfaces = file_path
        .as_ref()
        .map(|path| load_interfaces_from_file_set(&path.to_string_lossy()))
        .unwrap_or_default();

    let mut candidates = BTreeMap::new();
    for (module_name, interface) in interfaces {
        if module_exposes_symbol(&module_name, &interface, symbol) {
            candidates.insert(
                format!("module:{module_name}"),
                module_import_candidate(text, &module_name, symbol),
            );
        }

        if interface
            .functions
            .keys()
            .any(|(function_name, _)| function_name == symbol)
        {
            candidates.insert(
                format!("function:{module_name}:{symbol}"),
                selective_function_import_candidate(text, &module_name, symbol),
            );
        }
    }

    fallback_modules_for_symbol(symbol)
        .into_iter()
        .for_each(|module_name| {
            candidates
                .entry(format!("module:{module_name}"))
                .or_insert_with(|| module_import_candidate(text, module_name, symbol));
        });

    candidates.into_values().collect()
}

/// Extracts an unresolved symbol from a compiler diagnostic message.
///
/// Inputs:
/// - `message`: diagnostic text produced by parser/resolver/typechecker paths.
///
/// Output:
/// - Symbol name when the diagnostic shape is recognized.
/// - `None` for unrelated diagnostics.
///
/// Transformation:
/// - Parses stable user-facing messages rather than diagnostic codes because
///   early Terlan diagnostics are not yet code-rich across all compiler phases.
fn unresolved_symbol_from_diagnostic(message: &str) -> Option<String> {
    let constructor = message.strip_prefix("unknown constructor ")?;
    constructor
        .split_whitespace()
        .next()
        .filter(|name| is_terlan_name(name))
        .map(str::to_string)
}

/// Returns whether a module interface can satisfy a symbol.
///
/// Inputs:
/// - `module_name`: fully qualified Terlan module name.
/// - `interface`: loaded module interface.
/// - `symbol`: unresolved source name.
///
/// Output:
/// - `true` when importing the module is a useful candidate for the symbol.
///
/// Transformation:
/// - Accepts default-export style module-leaf matches and constructor exports.
fn module_exposes_symbol(
    module_name: &str,
    interface: &terlan_hir::ModuleInterface,
    symbol: &str,
) -> bool {
    module_leaf(module_name) == Some(symbol)
        || interface.constructors.contains_key(symbol)
        || interface.public_types.contains(symbol)
        || interface.opaque_types.contains(symbol)
}

/// Builds a module import candidate.
///
/// Inputs:
/// - `text`: current document text.
/// - `module_name`: fully qualified module to import.
/// - `symbol`: unresolved name used to identify same-leaf wrong imports.
///
/// Output:
/// - Import action candidate for `import module_name.`.
///
/// Transformation:
/// - Replaces an existing wrong import with the same leaf when present;
///   otherwise inserts the import after the module/import header block.
fn module_import_candidate(text: &str, module_name: &str, symbol: &str) -> ImportActionCandidate {
    let import_text = format!("import {module_name}.\n");
    if let Some(existing) = existing_import_for_leaf(text, symbol) {
        return ImportActionCandidate {
            title: format!("Replace import with {module_name}"),
            edit: TextEdit {
                range: existing.line_range,
                new_text: import_text,
            },
        };
    }

    ImportActionCandidate {
        title: format!("Import {module_name}"),
        edit: TextEdit {
            range: insertion_range(text),
            new_text: import_text,
        },
    }
}

/// Builds a selective function import candidate.
///
/// Inputs:
/// - `text`: current document text.
/// - `module_name`: module exporting the function.
/// - `function_name`: function to import selectively.
///
/// Output:
/// - Import action candidate for `import module.{function}.`.
///
/// Transformation:
/// - Inserts a selective import after the module/import header block.
fn selective_function_import_candidate(
    text: &str,
    module_name: &str,
    function_name: &str,
) -> ImportActionCandidate {
    ImportActionCandidate {
        title: format!("Import {function_name} from {module_name}"),
        edit: TextEdit {
            range: insertion_range(text),
            new_text: format!("import {module_name}.{{{function_name}}}.\n"),
        },
    }
}

/// Existing import line that can be replaced by an auto-import quick fix.
///
/// Inputs:
/// - One source import declaration.
///
/// Output:
/// - Line range and imported module path.
///
/// Transformation:
/// - Stores LSP replacement coordinates for the complete import line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExistingImport {
    line_range: Range,
}

/// Finds a same-leaf import that can be corrected.
///
/// Inputs:
/// - `text`: current document source.
/// - `leaf`: expected final module segment.
///
/// Output:
/// - Existing import line range when a non-selective import already ends with
///   `leaf`.
///
/// Transformation:
/// - Scans source lines for `import X.Leaf.` declarations and returns the
///   full-line LSP range so replacement removes the stale path cleanly.
fn existing_import_for_leaf(text: &str, leaf: &str) -> Option<ExistingImport> {
    text.lines().enumerate().find_map(|(line_index, line)| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("import ")?;
        if rest.contains(".{") || rest.starts_with("type ") {
            return None;
        }
        let module_name = rest.trim_end_matches('.');
        if module_leaf(module_name) != Some(leaf) {
            return None;
        }
        Some(ExistingImport {
            line_range: full_line_range(line_index, line),
        })
    })
}

/// Returns the import insertion range for a source document.
///
/// Inputs:
/// - `text`: current document source.
///
/// Output:
/// - Zero-width LSP range where new imports should be inserted.
///
/// Transformation:
/// - Places imports after the module declaration and existing import block,
///   skipping blank lines inside the header for stable editor edits.
fn insertion_range(text: &str) -> Range {
    let mut insert_line = 0usize;
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("module ") || trimmed.starts_with("import ") || trimmed.is_empty() {
            insert_line = line_index + 1;
            continue;
        }
        break;
    }
    zero_width_line_range(insert_line)
}

/// Builds a full-line LSP range.
///
/// Inputs:
/// - `line_index`: zero-based line number.
/// - `line`: source line text without a trailing line terminator.
///
/// Output:
/// - Range from line start through the next line start.
///
/// Transformation:
/// - Uses whole-line replacement so import quick fixes preserve a single
///   newline regardless of the old import length.
fn full_line_range(line_index: usize, line: &str) -> Range {
    let lsp_line = line_index as u32;
    Range::new(
        Position::new(lsp_line, 0),
        Position::new(lsp_line + 1, line_utf16_width(line) + 1),
    )
}

/// Builds a zero-width range at the start of a source line.
///
/// Inputs:
/// - `line_index`: zero-based source line.
///
/// Output:
/// - Empty LSP range at line start.
///
/// Transformation:
/// - Converts a Rust line index into LSP coordinates for insertion edits.
fn zero_width_line_range(line_index: usize) -> Range {
    let line = line_index as u32;
    Range::new(Position::new(line, 0), Position::new(line, 0))
}

/// Computes a line's UTF-16 character width.
///
/// Inputs:
/// - `line`: source line.
///
/// Output:
/// - UTF-16 code-unit width.
///
/// Transformation:
/// - Sums each character's UTF-16 width to match LSP character offsets.
fn line_utf16_width(line: &str) -> u32 {
    line.chars().map(|ch| ch.len_utf16() as u32).sum()
}

/// Returns the final module segment.
///
/// Inputs:
/// - `module_name`: dotted Terlan module path.
///
/// Output:
/// - Final segment when present.
///
/// Transformation:
/// - Splits on `.` and ignores empty paths.
fn module_leaf(module_name: &str) -> Option<&str> {
    module_name
        .rsplit('.')
        .next()
        .filter(|leaf| !leaf.is_empty())
}

/// Converts a document URI into a local path when possible.
///
/// Inputs:
/// - `uri`: LSP document URI.
///
/// Output:
/// - Local filesystem path for file URIs.
///
/// Transformation:
/// - Uses Tower LSP URL conversion and drops non-file URIs.
fn source_path_for_uri(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// Returns compiler-owned fallback module suggestions for shipped std modules.
///
/// Inputs:
/// - `symbol`: unresolved source name.
///
/// Output:
/// - Built-in module names that may not be discoverable from a user workspace.
///
/// Transformation:
/// - Keeps the first editor slice useful for installed compilers whose release
///   summaries are embedded rather than present under the project root.
fn fallback_modules_for_symbol(symbol: &str) -> Vec<&'static str> {
    match symbol {
        "List" => vec!["std.collections.List"],
        "Map" => vec!["std.collections.Map"],
        "Object" => vec!["std.core.Object"],
        "Set" => vec!["std.collections.Set"],
        "Vector" => vec!["std.native.collections.Vector"],
        "Console" => vec!["std.io.Console"],
        "Bool" => vec!["std.core.Bool"],
        "Int" => vec!["std.core.Int"],
        "String" => vec!["std.core.String"],
        "Option" => vec!["std.core.Option"],
        "Result" => vec!["std.core.Result"],
        _ => Vec::new(),
    }
}

/// Returns whether a token is a Terlan name candidate.
///
/// Inputs:
/// - `name`: candidate symbol from a diagnostic.
///
/// Output:
/// - `true` when the name contains only Terlan identifier bytes.
///
/// Transformation:
/// - Applies the same ASCII identifier subset currently used by LSP
///   definition lookup.
fn is_terlan_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
#[path = "import_actions_test.rs"]
mod import_actions_test;
