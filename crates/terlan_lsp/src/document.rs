use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use terlan_hir::{
    load_interfaces_from_file_set, resolve_syntax_module_output_with_interfaces, ModuleInterface,
};
use terlan_html::{validate_artifact_template_structure, HtmlDiagnostic};
use terlan_syntax::{parse_module_as_syntax_output, EbnfCompileError, ParserError, Span};
use terlan_typeck::{type_check_syntax_module_output, Diagnostic as TypeDiagnostic};
use tower_lsp::lsp_types::{Position, Range, Url};

/// Open Terlan document tracked by the LSP server.
///
/// Inputs:
/// - Latest full text received through `didOpen` or `didChange`.
///
/// Output:
/// - Cached parse/typecheck state used for diagnostics and tests.
///
/// Transformation:
/// - Stores source text alongside parser, resolver, and typechecker results so
///   diagnostics can be republished without reparsing on close/snapshot paths.
#[derive(Debug, Clone, Default)]
pub(crate) struct OpenDocument {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) version: i32,
    pub(crate) language_id: String,
    pub(crate) kind: DocumentKind,
    pub(crate) text: String,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) parse_ok: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) resolve_diagnostics: Vec<terlan_hir::Diagnostic>,
    pub(crate) type_diagnostics: Vec<TypeDiagnostic>,
    pub(crate) template_diagnostics: Vec<HtmlDiagnostic>,
}

impl OpenDocument {
    /// Returns whether this document should be parsed as a Terlan module.
    ///
    /// Inputs:
    /// - Current cached document metadata.
    ///
    /// Output:
    /// - `true` for source/interface documents.
    /// - `false` for template documents that need target-aware validation.
    ///
    /// Transformation:
    /// - Projects the cached document kind into the parser routing decision used
    ///   by diagnostics, symbols, and definition lookup.
    pub(crate) fn is_source_like(&self) -> bool {
        self.kind == DocumentKind::Source
    }

    /// Converts an LSP UTF-16 position to a byte offset in this document.
    ///
    /// Inputs:
    /// - `position`: zero-based LSP line and UTF-16 character offset.
    ///
    /// Output:
    /// - Byte offset into `self.text`, or `None` when the position is outside
    ///   the document.
    ///
    /// Transformation:
    /// - Walks line text and accounts for UTF-16 code-unit width while
    ///   returning Rust byte offsets used by parser spans.
    pub(crate) fn byte_offset_from_position(&self, position: Position) -> Option<usize> {
        let target_line = usize::try_from(position.line).ok()?;
        let target_character = usize::try_from(position.character).ok()?;

        let mut line_start = 0usize;
        for (line_idx, raw_line) in self.text.split('\n').enumerate() {
            let line = if raw_line.ends_with('\r') {
                &raw_line[..raw_line.len().saturating_sub(1)]
            } else {
                raw_line
            };

            if line_idx == target_line {
                let mut cursor = 0usize;
                let mut utf16_offset = 0usize;
                for ch in line.chars() {
                    if utf16_offset == target_character {
                        return Some(line_start + cursor);
                    }
                    let unit_width = ch.len_utf16();
                    let byte_width = ch.len_utf8();
                    utf16_offset += unit_width;
                    cursor += byte_width;
                }
                return if utf16_offset == target_character {
                    Some(line_start + cursor)
                } else {
                    None
                };
            }

            line_start += raw_line.len() + 1;
        }

        None
    }

    /// Converts a byte offset to an LSP UTF-16 position.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `byte_offset`: byte offset into `text`.
    ///
    /// Output:
    /// - LSP position, or `None` when the offset is outside the text.
    ///
    /// Transformation:
    /// - Walks normalized source lines, preserving CRLF behavior, and converts
    ///   Rust byte offsets to UTF-16 character offsets.
    fn position_from_byte_offset(text: &str, byte_offset: usize) -> Option<Position> {
        if byte_offset > text.len() {
            return None;
        }

        let mut line_start = 0usize;
        let mut line_number = 0u32;

        for raw_line in text.split('\n') {
            let line_length = raw_line.len();
            let normalized_line = if raw_line.ends_with('\r') {
                &raw_line[..line_length.saturating_sub(1)]
            } else {
                raw_line
            };
            let normalized_length = normalized_line.len();
            let line_end = line_start + normalized_length;

            if byte_offset <= line_end {
                let mut column = 0u32;
                if byte_offset == line_start {
                    return Some(Position::new(line_number, column));
                }

                let mut cursor = 0usize;
                for ch in normalized_line.chars() {
                    let next = cursor + ch.len_utf8();
                    if byte_offset < line_start + next {
                        return Some(Position::new(line_number, column));
                    }
                    cursor = next;
                    column += ch.len_utf16() as u32;
                    if byte_offset == line_start + cursor {
                        return Some(Position::new(line_number, column));
                    }
                }

                return Some(Position::new(line_number, column));
            }

            if byte_offset <= line_start + line_length {
                let end_column = normalized_line
                    .chars()
                    .map(|ch| ch.len_utf16() as u32)
                    .sum();
                return Some(Position::new(line_number, end_column));
            }

            line_start += raw_line.len() + 1;
            line_number += 1;
        }

        None
    }

    /// Converts a Terlan source span to an LSP range.
    ///
    /// Inputs:
    /// - `text`: source text containing the span.
    /// - `span`: byte-span from syntax or type checking.
    ///
    /// Output:
    /// - LSP range with UTF-16 positions.
    ///
    /// Transformation:
    /// - Normalizes reversed spans and uses byte-to-position conversion with
    ///   safe fallback positions for malformed spans.
    pub(crate) fn range_from_span(text: &str, span: &Span) -> Range {
        let normalized_start = if span.start <= span.end {
            span.start
        } else {
            span.end
        };
        let normalized_end = if span.end >= span.start {
            span.end
        } else {
            span.start
        };

        let start = Self::position_from_byte_offset(text, normalized_start).unwrap_or_default();
        let end = Self::position_from_byte_offset(text, normalized_end).unwrap_or(start);
        Range::new(start, end)
    }
}

/// LSP document category used to choose the validation path.
///
/// Inputs:
/// - LSP `languageId` supplied by editor integrations.
///
/// Output:
/// - Coarse document category for diagnostics and symbol handling.
///
/// Transformation:
/// - Keeps source/interface parsing on the formal compiler path while avoiding
///   bogus module diagnostics for `.terl.*` template documents while letting
///   the shared template validators own target-aware structure diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentKind {
    Source,
    Template,
}

impl Default for DocumentKind {
    /// Returns the default document kind for tests and unclassified inputs.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - `DocumentKind::Source`.
    ///
    /// Transformation:
    /// - Preserves the pre-existing behavior for callers that do not supply an
    ///   explicit LSP language id.
    fn default() -> Self {
        Self::Source
    }
}

impl DocumentKind {
    /// Classifies an LSP language id.
    ///
    /// Inputs:
    /// - `language_id`: LSP language id from an opened document.
    ///
    /// Output:
    /// - `DocumentKind::Template` for the editor-registered Terlan template
    ///   language family.
    /// - `DocumentKind::Source` for source/interface/unknown language ids.
    ///
    /// Transformation:
    /// - Uses the shared editor language-id prefix convention instead of file
    ///   extension parsing so untitled template buffers behave consistently.
    fn from_language_id(language_id: &str) -> Self {
        if language_id.starts_with("terlan-template-") {
            Self::Template
        } else {
            Self::Source
        }
    }
}

/// Thread-safe collection of currently open LSP documents.
///
/// Inputs:
/// - LSP document lifecycle events.
///
/// Output:
/// - Mutable URI-to-document cache behind a mutex.
///
/// Transformation:
/// - Serializes document updates and snapshots so async LSP handlers can share
///   document state safely.
#[derive(Debug, Clone, Default)]
pub(crate) struct OpenDocuments {
    documents: Arc<Mutex<HashMap<Url, OpenDocument>>>,
}

impl OpenDocuments {
    /// Returns whether a URI is currently open.
    ///
    /// Inputs:
    /// - `uri`: document URI.
    ///
    /// Output:
    /// - `true` when the URI has an open document entry.
    ///
    /// Transformation:
    /// - Performs a read-only lookup in the open-document cache.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_open(&self, uri: &Url) -> bool {
        self.documents
            .lock()
            .expect("open documents lock")
            .contains_key(uri)
    }

    /// Opens or replaces one document snapshot.
    ///
    /// Inputs:
    /// - `uri`: document URI.
    /// - `text`: full document text.
    /// - `version`: LSP document version.
    /// - `language_id`: LSP language id supplied by the editor.
    ///
    /// Output:
    /// - Parser error when syntax parsing fails.
    ///
    /// Transformation:
    /// - Classifies the document. Source/interface documents parse, resolve,
    ///   and typecheck through the compiler path. Template documents skip
    ///   source-module parsing and validate through `terlan_html`.
    pub(crate) fn open(
        &self,
        uri: Url,
        text: String,
        version: i32,
        language_id: String,
    ) -> Option<ParserError> {
        let kind = DocumentKind::from_language_id(&language_id);
        let (parse_ok, parse_error, resolve_diagnostics, type_diagnostics, template_diagnostics) =
            match kind {
                DocumentKind::Source => {
                    let parse_result =
                        parse_module_as_syntax_output(&text).map_err(Self::parser_error);
                    match parse_result {
                        Ok(module) => {
                            let interfaces = Self::interfaces_for_uri(&uri);
                            let resolved =
                                resolve_syntax_module_output_with_interfaces(&module, &interfaces)
                                    .module;
                            let type_diagnostics =
                                type_check_syntax_module_output(&module, &resolved);
                            (
                                true,
                                None,
                                resolved.diagnostics,
                                type_diagnostics,
                                Vec::new(),
                            )
                        }
                        Err(error) => (false, Some(error), Vec::new(), Vec::new(), Vec::new()),
                    }
                }
                DocumentKind::Template => (
                    true,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Self::template_diagnostics_for_uri(&uri, &text),
                ),
            };
        let mut lock = self.documents.lock().expect("open documents lock");
        lock.insert(
            uri,
            OpenDocument {
                version,
                language_id,
                kind,
                text,
                parse_ok,
                resolve_diagnostics,
                type_diagnostics,
                template_diagnostics,
            },
        );
        parse_error
    }

    /// Validates a template document through the shared template validators.
    ///
    /// Inputs:
    /// - `uri`: document URI opened by the editor.
    /// - `text`: current template source.
    ///
    /// Output:
    /// - Template diagnostics from `terlan_html`, or an empty vector when the
    ///   source validates.
    ///
    /// Transformation:
    /// - Converts file URIs to paths so suffix-based target validation can
    ///   select HTML, Markdown, JSON, TOML, YAML, or text rules. Non-file URIs
    ///   use their URI path as a best-effort suffix source for untitled/editor
    ///   virtual documents.
    fn template_diagnostics_for_uri(uri: &Url, text: &str) -> Vec<HtmlDiagnostic> {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.path()));
        match validate_artifact_template_structure(text, &path) {
            Ok(()) => Vec::new(),
            Err(diagnostics) => diagnostics,
        }
    }

    /// Converts syntax-output parse errors into LSP parser diagnostics.
    ///
    /// Inputs:
    /// - `error`: EBNF/syntax-output compile error returned by the formal
    ///   syntax parser.
    ///
    /// Output:
    /// - Existing parser diagnostic shape consumed by the LSP publisher.
    ///
    /// Transformation:
    /// - Preserves parser messages and spans. Serialization failures are
    ///   projected to a zero-width source span because they are compiler
    ///   artifact failures rather than user source spans.
    fn parser_error(error: EbnfCompileError) -> ParserError {
        match error {
            EbnfCompileError::Parse(message, span) => ParserError { message, span },
            EbnfCompileError::Serialize(message) => ParserError {
                message,
                span: Span::new(0, 0),
            },
        }
    }

    /// Loads visible interface summaries for a document URI.
    ///
    /// Inputs:
    /// - `uri`: document URI being parsed.
    ///
    /// Output:
    /// - Interface map loaded from the surrounding file set.
    ///
    /// Transformation:
    /// - Converts file URIs to paths and delegates summary discovery to HIR;
    ///   non-file URIs use an empty interface map.
    pub(crate) fn interfaces_for_uri(uri: &Url) -> HashMap<String, ModuleInterface> {
        uri.to_file_path()
            .ok()
            .map(|path| load_interfaces_from_file_set(&path.to_string_lossy()))
            .unwrap_or_default()
    }

    /// Removes a document from the open-document cache.
    ///
    /// Inputs:
    /// - `uri`: document URI to close.
    ///
    /// Output:
    /// - Removed document snapshot when present.
    ///
    /// Transformation:
    /// - Mutates only the cache entry for the supplied URI.
    pub(crate) fn close(&self, uri: &Url) -> Option<OpenDocument> {
        self.documents
            .lock()
            .expect("open documents lock")
            .remove(uri)
    }

    /// Clones the current document snapshot for a URI.
    ///
    /// Inputs:
    /// - `uri`: document URI.
    ///
    /// Output:
    /// - Cloned `OpenDocument` when the URI is open.
    ///
    /// Transformation:
    /// - Keeps diagnostics publishing outside the mutex critical section by
    ///   returning an owned clone.
    pub(crate) fn snapshot(&self, uri: &Url) -> Option<OpenDocument> {
        self.documents
            .lock()
            .expect("open documents lock")
            .get(uri)
            .cloned()
    }

    /// Counts currently open documents.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Number of cached document snapshots.
    ///
    /// Transformation:
    /// - Performs a read-only cache length lookup for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn count(&self) -> usize {
        self.documents.lock().expect("open documents lock").len()
    }
}
