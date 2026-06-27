use std::fs;
#[cfg(not(coverage))]
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;

#[cfg(test)]
pub(crate) mod test_fs;

use crate::commands::json::json_string;
use crate::{ColorChoice, DiagnosticFormat};

/// Selects the effective color policy for diagnostic rendering.
///
/// Inputs:
/// - `format`: current diagnostic format.
///
/// Output:
/// - Color choice used for text diagnostics.
///
/// Transformation:
/// - Preserves the text color setting and treats JSON diagnostics as non-color
///   output for later text-format transitions.
pub(crate) fn diagnostic_color(format: DiagnosticFormat) -> ColorChoice {
    match format {
        DiagnosticFormat::Text { color } => color,
        DiagnosticFormat::Json => ColorChoice::Auto,
    }
}

/// Emits one compiler diagnostic in the configured format.
///
/// Inputs:
/// - `kind`: diagnostic category, such as `parse_error` or `type_error`.
/// - `message`: diagnostic message.
/// - `path`: source path used for display and source-line lookup.
/// - `start`: byte offset where the diagnostic starts.
/// - `end`: byte offset where the diagnostic ends.
/// - `format`: text or JSON output mode.
///
/// Output:
/// - Writes diagnostic text to stderr for text mode or stdout for JSON mode.
///
/// Transformation:
/// - Renders human-readable text diagnostics with source context or compact JSON
///   diagnostics for machine-readable callers.
pub(crate) fn emit_diagnostic(
    kind: &str,
    message: &str,
    path: &str,
    start: usize,
    end: usize,
    format: DiagnosticFormat,
) {
    let kind = diagnostic_kind_for_message(kind, message);
    match format {
        DiagnosticFormat::Text { color } => {
            eprint!(
                "{}",
                render_text_diagnostic(kind, message, path, start, end, color)
            );
        }
        DiagnosticFormat::Json => {
            println!(
                "{{\"kind\":{},\"message\":{},\"path\":{},\"start\":{},\"end\":{}}}",
                json_string(kind),
                json_string(message),
                json_string(path),
                start,
                end
            );
        }
    }
}

/// Classifies a diagnostic message into its user-facing category.
///
/// Inputs:
/// - `kind`: default compiler phase diagnostic kind.
/// - `message`: diagnostic message text.
///
/// Output:
/// - User-facing diagnostic kind.
///
/// Transformation:
/// - Keeps existing phase categories except for missing selected-import
///   provider modules, which are import-resolution failures rather than type
///   mismatches.
pub(crate) fn diagnostic_kind_for_message<'a>(kind: &'a str, message: &str) -> &'a str {
    if kind == "type_error" && is_module_import_not_found(message) {
        "module_import"
    } else {
        kind
    }
}

/// Renders a text diagnostic with optional source context.
///
/// Inputs:
/// - `kind`: diagnostic category.
/// - `message`: diagnostic message.
/// - `path`: source path used to load context.
/// - `start`: byte offset where the diagnostic starts.
/// - `end`: byte offset where the diagnostic ends.
/// - `color_choice`: color policy for text output.
///
/// Output:
/// - Complete diagnostic string for stderr.
///
/// Transformation:
/// - Builds compiler-style text output, including source line, underline, and
///   expected/found detail when the message uses type-mismatch wording.
fn render_text_diagnostic(
    kind: &str,
    message: &str,
    path: &str,
    start: usize,
    end: usize,
    color_choice: ColorChoice,
) -> String {
    let mut out = String::new();
    let colors = DiagnosticColors::new(color_choice);
    let code = diagnostic_code(kind, message);
    let title = diagnostic_title(kind, message);
    out.push_str(&format!(
        "{}: {}\n",
        colors.error(&format!("error[{}]", code)),
        colors.bold(title)
    ));
    out.push('\n');

    match fs::read_to_string(Path::new(path)) {
        Ok(source) => {
            let (display_start, display_end) =
                diagnostic_display_span(kind, message, &source, start, end);
            let (line, column) = line_column(&source, display_start);
            out.push_str(&format!(
                "  --> {}\n",
                colors.location(&format!("{}:{}:{}", path, line, column))
            ));
            out.push_str(&format!("{} |\n", colors.gutter("   ")));
            if let Some(line_text) = source.lines().nth(line.saturating_sub(1)) {
                out.push_str(&format!(
                    "{} | {}\n",
                    colors.gutter(&format!("{:>3}", line)),
                    line_text
                ));
                out.push_str(&format!("{} | ", colors.gutter("   ")));
                out.push_str(&colors.error(&caret_underline(
                    line_text,
                    column,
                    display_start,
                    display_end,
                )));
                out.push('\n');
            }
            out.push_str(&format!("{} |\n", colors.gutter("   ")));
        }
        Err(_) => {
            out.push_str(&format!(
                "  --> {}\n",
                colors.location(&format!("{}:{}", path, start))
            ));
            out.push_str(&format!("{} |\n", colors.gutter("   ")));
        }
    }

    if let Some((expected, found)) = expected_found(message) {
        out.push_str(&format!("   = {}: {}\n", colors.bold("expected"), expected));
        out.push_str(&format!("   = {}:    {}\n", colors.bold("found"), found));
    } else {
        out.push_str(&format!("   = {}\n", message));
    }

    out
}

/// ANSI color renderer for diagnostic text.
///
/// Inputs:
/// - `enabled`: whether text should be wrapped with ANSI color escapes.
///
/// Output:
/// - Small formatter used by text diagnostic rendering.
///
/// Transformation:
/// - Applies or suppresses ANSI escape sequences based on the selected color
///   policy.
struct DiagnosticColors {
    enabled: bool,
}

impl DiagnosticColors {
    /// Builds a diagnostic color formatter from a color policy.
    ///
    /// Inputs:
    /// - `choice`: CLI color choice.
    ///
    /// Output:
    /// - Formatter with resolved color enablement.
    ///
    /// Transformation:
    /// - Resolves `auto` from terminal and `NO_COLOR` state.
    fn new(choice: ColorChoice) -> Self {
        let enabled = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => auto_diagnostic_color_enabled(),
        };
        Self { enabled }
    }

    /// Formats an error-colored diagnostic segment.
    ///
    /// Inputs:
    /// - `text`: segment to format.
    ///
    /// Output:
    /// - Colored or plain segment.
    ///
    /// Transformation:
    /// - Applies the bold red ANSI style when colors are enabled.
    fn error(&self, text: &str) -> String {
        self.paint("\x1b[1;31m", text)
    }

    /// Formats a source-location diagnostic segment.
    ///
    /// Inputs:
    /// - `text`: segment to format.
    ///
    /// Output:
    /// - Colored or plain segment.
    ///
    /// Transformation:
    /// - Applies the cyan ANSI style when colors are enabled.
    fn location(&self, text: &str) -> String {
        self.paint("\x1b[36m", text)
    }

    /// Formats a source-gutter diagnostic segment.
    ///
    /// Inputs:
    /// - `text`: segment to format.
    ///
    /// Output:
    /// - Colored or plain segment.
    ///
    /// Transformation:
    /// - Applies the blue ANSI style when colors are enabled.
    fn gutter(&self, text: &str) -> String {
        self.paint("\x1b[34m", text)
    }

    /// Formats a bold diagnostic segment.
    ///
    /// Inputs:
    /// - `text`: segment to format.
    ///
    /// Output:
    /// - Colored or plain segment.
    ///
    /// Transformation:
    /// - Applies the bold ANSI style when colors are enabled.
    fn bold(&self, text: &str) -> String {
        self.paint("\x1b[1m", text)
    }

    /// Applies an ANSI code when colors are enabled.
    ///
    /// Inputs:
    /// - `code`: ANSI escape prefix.
    /// - `text`: text to wrap.
    ///
    /// Output:
    /// - Wrapped text or an unchanged string.
    ///
    /// Transformation:
    /// - Adds the reset sequence after colored output.
    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("{}{}\x1b[0m", code, text)
        } else {
            text.to_string()
        }
    }
}

/// Returns automatic color support in coverage builds.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Always `false` in coverage builds.
///
/// Transformation:
/// - Disables terminal probing to keep coverage runs deterministic.
#[cfg(coverage)]
fn auto_diagnostic_color_enabled() -> bool {
    false
}

/// Returns automatic color support for normal builds.
///
/// Inputs:
/// - Process environment and stderr terminal state.
///
/// Output:
/// - `true` when color is allowed and stderr is a terminal.
///
/// Transformation:
/// - Honors `NO_COLOR` and queries stderr terminal capability.
#[cfg(not(coverage))]
fn auto_diagnostic_color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

/// Chooses the byte span displayed by a diagnostic.
///
/// Inputs:
/// - `kind`: diagnostic category.
/// - `message`: diagnostic message.
/// - `source`: source text.
/// - `start`: original byte span start.
/// - `end`: original byte span end.
///
/// Output:
/// - Display byte span.
///
/// Transformation:
/// - Narrows type-mismatch diagnostics to the returned expression when possible
///   and leaves all other diagnostics unchanged.
pub(crate) fn diagnostic_display_span(
    kind: &str,
    message: &str,
    source: &str,
    start: usize,
    end: usize,
) -> (usize, usize) {
    if kind == "type_error" {
        if let Some(constructor) = unknown_constructor_name(message) {
            if let Some(offset) = source
                .get(start..end)
                .and_then(|slice| slice.find(constructor))
            {
                let display_start = start + offset;
                return (display_start, display_start + constructor.len());
            }
        }
    }

    if kind != "type_error" || expected_found(message).is_none() {
        return (start, end);
    }
    returned_expression_span(source, start, end).unwrap_or((start, end))
}

/// Extracts the constructor name from a stable unknown-constructor diagnostic.
///
/// Inputs:
/// - `message`: typechecker diagnostic message.
///
/// Output:
/// - Constructor name when the message has `unknown constructor Name / arity`
///   shape.
///
/// Transformation:
/// - Parses only the stable diagnostic prefix used for constructor calls and
///   avoids changing the diagnostic message itself.
fn unknown_constructor_name(message: &str) -> Option<&str> {
    let rest = message.strip_prefix("unknown constructor ")?;
    let (name, _) = rest.split_once(" / ")?;
    (!name.is_empty()).then_some(name)
}

/// Finds the returned expression inside a function body span.
///
/// Inputs:
/// - `source`: source text.
/// - `start`: span start containing a function clause.
/// - `end`: span end containing a function clause.
///
/// Output:
/// - Expression byte span, or `None` when it cannot be isolated.
///
/// Transformation:
/// - Locates `->`, skips whitespace, and stops before newline, dot, or
///   semicolon terminators.
fn returned_expression_span(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let slice = source.get(start..end)?;
    let arrow = slice.find("->")?;
    let mut expr_start = start + arrow + "->".len();
    let bytes = source.as_bytes();
    while expr_start < end && bytes.get(expr_start).is_some_and(u8::is_ascii_whitespace) {
        expr_start += 1;
    }
    if expr_start >= end {
        return None;
    }

    let mut expr_end = expr_start;
    while expr_end < end {
        match bytes.get(expr_end) {
            Some(b'\n' | b'.' | b';') | None => break,
            Some(_) => expr_end += 1,
        }
    }
    while expr_end > expr_start && bytes.get(expr_end - 1).is_some_and(u8::is_ascii_whitespace) {
        expr_end -= 1;
    }

    (expr_end > expr_start).then_some((expr_start, expr_end))
}

/// Computes the displayed diagnostic code.
///
/// Inputs:
/// - `kind`: diagnostic category.
/// - `message`: diagnostic message.
///
/// Output:
/// - Display code string.
///
/// Transformation:
/// - Maps expected/found type errors to `type_mismatch`; otherwise preserves
///   the diagnostic kind.
fn diagnostic_code(kind: &str, message: &str) -> String {
    if kind == "type_error" && expected_found(message).is_some() {
        "type_mismatch".to_string()
    } else {
        kind.to_string()
    }
}

/// Computes the displayed diagnostic title.
///
/// Inputs:
/// - `kind`: diagnostic category.
/// - `message`: diagnostic message.
///
/// Output:
/// - Human-readable diagnostic title.
///
/// Transformation:
/// - Special-cases type mismatches, parse errors, and warnings.
fn diagnostic_title(kind: &str, message: &str) -> &'static str {
    if kind == "type_error" && expected_found(message).is_some() {
        "type mismatch"
    } else if kind == "module_import" && is_module_import_not_found(message) {
        "not found"
    } else if kind == "parse_error" {
        "parse error"
    } else if kind == "warning" {
        "warning"
    } else {
        "diagnostic"
    }
}

/// Detects selected-import provider lookup failures.
///
/// Inputs:
/// - `message`: diagnostic message text.
///
/// Output:
/// - `true` when the message reports a missing imported module interface.
///
/// Transformation:
/// - Recognizes the typechecker's stable selected-import wording and lets the
///   CLI present it as an import-resolution diagnostic.
fn is_module_import_not_found(message: &str) -> bool {
    message.starts_with("cannot find module `")
        && message.contains(" for imported function `")
        && message.contains("no interface for `")
}

/// Parses expected/found details from a diagnostic message.
///
/// Inputs:
/// - `message`: diagnostic message.
///
/// Output:
/// - Expected and found text, or `None` when the message does not match.
///
/// Transformation:
/// - Recognizes messages shaped as `expected ... found ...`.
fn expected_found(message: &str) -> Option<(&str, &str)> {
    let rest = message.strip_prefix("expected ")?;
    let (expected, found) = rest.split_once(" found ")?;
    Some((expected.trim(), found.trim()))
}

/// Converts a byte offset to one-based line and column.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: byte offset.
///
/// Output:
/// - One-based `(line, column)` pair.
///
/// Transformation:
/// - Walks character indices so line starts account for UTF-8 boundaries.
fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut line_start = 0usize;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + ch.len_utf8();
        }
    }
    (line, offset.saturating_sub(line_start) + 1)
}

/// Builds the caret underline for a source diagnostic.
///
/// Inputs:
/// - `line_text`: source line containing the diagnostic.
/// - `column`: one-based diagnostic column.
/// - `start`: byte span start.
/// - `end`: byte span end.
///
/// Output:
/// - Underline text with leading spaces and one or more carets.
///
/// Transformation:
/// - Converts byte-span width into a bounded underline for the displayed line.
fn caret_underline(line_text: &str, column: usize, start: usize, end: usize) -> String {
    let prefix_len = column.saturating_sub(1);
    let mut underline = String::new();
    underline.push_str(&" ".repeat(prefix_len));
    let width = if end > start { end - start } else { 1 };
    let remaining = line_text.len().saturating_sub(prefix_len);
    underline.push_str(&"^".repeat(width.min(remaining).max(1)));
    underline
}

/// Reads a UTF-8 source file for a command.
///
/// Inputs:
/// - `path`: filesystem path as command text.
///
/// Output:
/// - File contents, or a user-facing error string.
///
/// Transformation:
/// - Reads the file as text and normalizes IO errors into CLI diagnostics.
pub(crate) fn read_file(path: &str) -> Result<String, String> {
    match fs::read_to_string(Path::new(path)) {
        Ok(contents) => Ok(contents),
        Err(err) => Err(format!("failed to read {}: {}", path, err)),
    }
}

/// Writes bytes while preserving incremental no-op behavior.
///
/// Inputs:
/// - `path`: output path.
/// - `bytes`: desired output bytes.
/// - `incremental`: whether unchanged existing output should be left untouched.
///
/// Output:
/// - Filesystem write result.
///
/// Transformation:
/// - Skips the write when incremental mode is enabled and existing bytes match;
///   otherwise writes the requested bytes.
pub(crate) fn write_if_changed_or_forced(
    path: &Path,
    bytes: &[u8],
    incremental: bool,
) -> std::io::Result<()> {
    if incremental && path.exists() {
        if let Ok(existing) = fs::read(path) {
            if existing == bytes {
                return Ok(());
            }
        }
    }

    fs::write(path, bytes)
}

/// Checks whether a string is lowercase SHA-256 hex.
///
/// Inputs:
/// - `value`: candidate hash text.
///
/// Output:
/// - `true` when `value` is exactly 64 lowercase hexadecimal characters.
///
/// Transformation:
/// - Performs a byte-level check so malformed hashes can be rejected before
///   comparing generated or external checksum values.
pub(crate) fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Computes a file SHA-256 with the local system hash tool.
///
/// Inputs:
/// - `path`: existing file path to hash.
///
/// Output:
/// - `Ok(String)` with lowercase hex SHA-256.
/// - `Err(String)` when `sha256sum` is unavailable or returns malformed
///   output.
///
/// Transformation:
/// - Invokes `sha256sum`, reads the first whitespace-delimited field, and
///   validates it as lowercase SHA-256 hex before returning it.
pub(crate) fn sha256sum_file(path: &Path) -> Result<String, String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|error| format!("cannot run sha256sum for `{}`: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!("sha256sum failed for `{}`", path.display()));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("sha256sum output was not UTF-8: {error}"))?;
    let Some(hash) = stdout.split_whitespace().next() else {
        return Err("sha256sum output was empty".to_string());
    };
    if !is_valid_sha256_hex(hash) {
        return Err(format!("sha256sum output was not SHA-256 hex: `{hash}`"));
    }
    Ok(hash.to_string())
}

/// Converts a Terlan module name into an Erlang output file stem.
///
/// Inputs:
/// - `module_name`: resolved Terlan module name.
///
/// Output:
/// - Erlang-compatible output stem.
///
/// Transformation:
/// - Replaces module namespace dots with underscores and lowercases the result
///   for Erlang module/file compatibility.
pub(crate) fn erlang_output_stem(module_name: &str) -> String {
    module_name.replace('.', "_").to_ascii_lowercase()
}
