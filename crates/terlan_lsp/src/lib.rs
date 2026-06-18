use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use terlan_hir::{
    load_interfaces_from_file_set, resolve_syntax_module_output_with_interfaces, ModuleInterface,
};
use terlan_syntax::{parse_module_as_syntax_output, EbnfCompileError, ParserError, Span};
use terlan_typeck::{type_check_syntax_module_output, DiagSeverity};
use tower_lsp::jsonrpc::Result;
use tower_lsp::{lsp_types::*, Client, LanguageServer, LspService, Server};

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
struct OpenDocument {
    #[cfg_attr(not(test), allow(dead_code))]
    version: i32,
    text: String,
    #[cfg_attr(not(test), allow(dead_code))]
    parse_ok: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    resolve_diagnostics: Vec<terlan_hir::Diagnostic>,
    type_diagnostics: Vec<terlan_typeck::Diagnostic>,
}

impl OpenDocument {
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
    #[cfg(test)]
    fn byte_offset_from_position(&self, position: Position) -> Option<usize> {
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
    fn range_from_span(text: &str, span: &terlan_syntax::Span) -> Range {
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
struct OpenDocuments {
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
    #[cfg(test)]
    fn is_open(&self, uri: &Url) -> bool {
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
    ///
    /// Output:
    /// - Parser error when syntax parsing fails.
    ///
    /// Transformation:
    /// - Parses, resolves, and typechecks the source, then stores the resulting
    ///   snapshot in the open-document cache.
    fn open(&self, uri: Url, text: String, version: i32) -> Option<ParserError> {
        let parse_result = parse_module_as_syntax_output(&text).map_err(Self::parser_error);
        let (parse_ok, parse_error, resolve_diagnostics, type_diagnostics) = match parse_result {
            Ok(module) => {
                let interfaces = Self::interfaces_for_uri(&uri);
                let resolved =
                    resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
                let type_diagnostics = type_check_syntax_module_output(&module, &resolved);
                (true, None, resolved.diagnostics, type_diagnostics)
            }
            Err(error) => (false, Some(error), Vec::new(), Vec::new()),
        };
        let mut lock = self.documents.lock().expect("open documents lock");
        lock.insert(
            uri,
            OpenDocument {
                version,
                text,
                parse_ok,
                resolve_diagnostics,
                type_diagnostics,
            },
        );
        parse_error
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
    fn interfaces_for_uri(uri: &Url) -> HashMap<String, ModuleInterface> {
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
    fn close(&self, uri: &Url) -> Option<OpenDocument> {
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
    fn snapshot(&self, uri: &Url) -> Option<OpenDocument> {
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
    #[cfg(test)]
    fn count(&self) -> usize {
        self.documents.lock().expect("open documents lock").len()
    }
}

/// Terlan Language Server backend.
///
/// Inputs:
/// - Tower LSP client handle supplied by `LspService`.
///
/// Output:
/// - Language server implementation for document lifecycle and diagnostics.
///
/// Transformation:
/// - Bridges LSP events into Terlan parsing, HIR resolution, type checking, and
///   diagnostics publication.
#[derive(Debug, Clone)]
pub struct Backend {
    client: Client,
    open_documents: OpenDocuments,
}

impl Backend {
    /// Creates a new LSP backend.
    ///
    /// Inputs:
    /// - `client`: Tower LSP client handle.
    ///
    /// Output:
    /// - Backend with an empty open-document cache.
    ///
    /// Transformation:
    /// - Stores the client and initializes shared document state.
    fn new(client: Client) -> Self {
        Self {
            client,
            open_documents: OpenDocuments::default(),
        }
    }

    /// Publishes parser or typechecker diagnostics for one document.
    ///
    /// Inputs:
    /// - `uri`: target document URI.
    /// - `version`: document version for diagnostic publication.
    /// - `parse_error`: optional syntax parser error.
    /// - `document`: latest document snapshot.
    ///
    /// Output:
    /// - None; diagnostics are sent to the LSP client.
    ///
    /// Transformation:
    /// - Converts Terlan spans and severities into LSP diagnostics, preferring
    ///   parse errors over type diagnostics when parsing failed.
    async fn publish_document_diagnostics(
        &self,
        uri: Url,
        version: i32,
        parse_error: Option<ParserError>,
        document: &OpenDocument,
    ) {
        let diagnostics = match parse_error {
            Some(error) => vec![Diagnostic {
                range: OpenDocument::range_from_span(&document.text, &error.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: error.message,
                source: Some("terlan-syntax".to_string()),
                ..Default::default()
            }],
            None => document
                .type_diagnostics
                .iter()
                .map(|diagnostic| Diagnostic {
                    range: OpenDocument::range_from_span(&document.text, &diagnostic.span),
                    severity: Some(match diagnostic.severity {
                        DiagSeverity::Error => DiagnosticSeverity::ERROR,
                        DiagSeverity::Warning => DiagnosticSeverity::WARNING,
                    }),
                    message: diagnostic.message.clone(),
                    source: Some("terlan-typeck".to_string()),
                    ..Default::default()
                })
                .collect(),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    /// Handles LSP initialize requests.
    ///
    /// Inputs:
    /// - `params`: client initialization payload.
    ///
    /// Output:
    /// - Server capabilities and server info.
    ///
    /// Transformation:
    /// - Currently advertises default capabilities while returning versioned
    ///   server metadata.
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let _ = params.process_id;
        let _ = self.client.clone();

        Ok(InitializeResult {
            capabilities: ServerCapabilities::default(),
            server_info: Some(ServerInfo {
                name: "terlan-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    /// Handles LSP initialized notifications.
    ///
    /// Inputs:
    /// - Initialized notification parameters.
    ///
    /// Output:
    /// - None.
    ///
    /// Transformation:
    /// - Keeps the client handle live; no registration side effects are needed
    ///   for the current minimal server.
    async fn initialized(&self, _: InitializedParams) {
        let _ = &self.client;
    }

    /// Handles LSP document-open notifications.
    ///
    /// Inputs:
    /// - `params`: opened document URI, text, and version.
    ///
    /// Output:
    /// - None; diagnostics are published asynchronously.
    ///
    /// Transformation:
    /// - Stores the full text snapshot, parses/typechecks it, and publishes
    ///   diagnostics for the opened version.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text_document = params.text_document;
        let text = text_document.text;
        let uri = text_document.uri;
        let version = text_document.version;
        let parse_error = self.open_documents.open(uri.clone(), text.clone(), version);
        if let Some(document) = self.open_documents.snapshot(&uri) {
            self.publish_document_diagnostics(uri, version, parse_error, &document)
                .await;
        }
    }

    /// Handles LSP document-change notifications.
    ///
    /// Inputs:
    /// - `params`: changed document URI, version, and text changes.
    ///
    /// Output:
    /// - None; diagnostics are published asynchronously.
    ///
    /// Transformation:
    /// - Uses full-document text changes when supplied, falls back to the last
    ///   change payload, updates the cache, and republishes diagnostics.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text_changes = params.content_changes;
        let text = text_changes
            .iter()
            .find(|change| change.range.is_none())
            .map(|change| change.text.clone())
            .or_else(|| text_changes.last().map(|change| change.text.clone()))
            .unwrap_or_else(String::new);
        let parse_error = self.open_documents.open(uri.clone(), text.clone(), version);
        if let Some(document) = self.open_documents.snapshot(&uri) {
            self.publish_document_diagnostics(uri, version, parse_error, &document)
                .await;
        }
    }

    /// Handles LSP document-close notifications.
    ///
    /// Inputs:
    /// - `params`: closed document URI.
    ///
    /// Output:
    /// - None.
    ///
    /// Transformation:
    /// - Removes the document from the open-document cache.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.open_documents.close(&params.text_document.uri);
    }

    /// Handles LSP shutdown requests.
    ///
    /// Inputs:
    /// - None beyond the request receiver.
    ///
    /// Output:
    /// - Successful JSON-RPC result.
    ///
    /// Transformation:
    /// - Leaves process shutdown to the LSP transport owner.
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Runs the Terlan LSP server over stdio.
///
/// Inputs:
/// - Process stdin/stdout.
///
/// Output:
/// - Process exit code.
///
/// Transformation:
/// - Creates a Tokio runtime and runs the async LSP service, converting startup
///   or server errors into CLI-friendly exit codes.
pub fn run_stdio_server() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("failed to start async runtime for LSP server: {err}");
            return ExitCode::from(1);
        }
    };

    if let Err(err) = runtime.block_on(run_stdio_server_async()) {
        eprintln!("terlan-lsp failed: {err}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

/// Runs the async LSP service over stdio.
///
/// Inputs:
/// - Tokio stdin/stdout handles.
///
/// Output:
/// - IO result from setting up and serving the LSP transport.
///
/// Transformation:
/// - Builds a Tower LSP service with `Backend::new` and serves it until the
///   transport exits.
async fn run_stdio_server_async() -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
