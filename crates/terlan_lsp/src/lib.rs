use std::process::ExitCode;

mod document;
mod hover;
mod import_actions;

use document::{OpenDocument, OpenDocuments};
use hover::hover_for_position;
use import_actions::import_code_actions_for_diagnostic;
use terlan_syntax::{
    parse_module_as_syntax_output, ParserError, Span, SyntaxDeclarationPayload, SyntaxModuleOutput,
};
use terlan_typeck::DiagSeverity;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{lsp_types::*, Client, LanguageServer, LspService, Server};

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
    ///   parse errors when parsing failed, then publishing resolver diagnostics
    ///   before typechecker diagnostics for parseable documents.
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
            None => Self::resolver_diagnostics_for_document(document)
                .into_iter()
                .chain(Self::type_diagnostics_for_document(document))
                .chain(Self::template_diagnostics_for_document(document))
                .collect(),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    /// Converts cached resolver diagnostics into LSP diagnostics.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    ///
    /// Output:
    /// - LSP diagnostics sourced from `terlan-hir`.
    ///
    /// Transformation:
    /// - Treats HIR resolver diagnostics as errors and converts byte spans to
    ///   UTF-16 LSP ranges.
    fn resolver_diagnostics_for_document(document: &OpenDocument) -> Vec<Diagnostic> {
        document
            .resolve_diagnostics
            .iter()
            .map(|diagnostic| Diagnostic {
                range: OpenDocument::range_from_span(&document.text, &diagnostic.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: diagnostic.message.clone(),
                source: Some("terlan-hir".to_string()),
                ..Default::default()
            })
            .collect()
    }

    /// Converts cached typechecker diagnostics into LSP diagnostics.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    ///
    /// Output:
    /// - LSP diagnostics sourced from `terlan-typeck`.
    ///
    /// Transformation:
    /// - Preserves typechecker severity and converts byte spans to UTF-16 LSP
    ///   ranges.
    fn type_diagnostics_for_document(document: &OpenDocument) -> Vec<Diagnostic> {
        document
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
            .collect()
    }

    /// Converts cached template diagnostics into LSP diagnostics.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    ///
    /// Output:
    /// - LSP diagnostics sourced from `terlan-template`.
    ///
    /// Transformation:
    /// - Projects path-aware template structure diagnostics into a conservative
    ///   zero-width document-start range until `terlan_html` exposes precise
    ///   source spans for every target validator.
    fn template_diagnostics_for_document(document: &OpenDocument) -> Vec<Diagnostic> {
        document
            .template_diagnostics
            .iter()
            .map(|diagnostic| Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: diagnostic.message.clone(),
                source: Some("terlan-template".to_string()),
                ..Default::default()
            })
            .collect()
    }

    /// Builds document symbols for Terlan source text.
    ///
    /// Inputs:
    /// - `text`: current document text.
    ///
    /// Output:
    /// - Nested LSP document symbols, or an empty list when parsing fails.
    ///
    /// Transformation:
    /// - Parses through the compiler syntax-output path and projects module and
    ///   declaration payloads into LSP symbol names, kinds, and ranges.
    fn document_symbols_for_text(text: &str) -> Vec<DocumentSymbol> {
        let Ok(module) = parse_module_as_syntax_output(text) else {
            return Vec::new();
        };
        vec![Self::module_document_symbol(text, &module)]
    }

    /// Finds same-document definition locations for a source position.
    ///
    /// Inputs:
    /// - `uri`: document URI used in returned LSP locations.
    /// - `document`: current open-document snapshot.
    /// - `position`: cursor position from the editor.
    ///
    /// Output:
    /// - One location for a matching declaration symbol.
    /// - Empty vector when the cursor is not on an identifier, parsing fails,
    ///   or the identifier has no same-document declaration match.
    ///
    /// Transformation:
    /// - Extracts the identifier under the cursor, reuses compiler-backed
    ///   document symbols, and maps the first matching declaration selection
    ///   range into an LSP location. Cross-file imports are intentionally
    ///   deferred until compiler resolver data can expose safe definition
    ///   targets.
    fn definition_locations_for_position(
        uri: &Url,
        document: &OpenDocument,
        position: Position,
    ) -> Vec<Location> {
        let Some(byte_offset) = document.byte_offset_from_position(position) else {
            return Vec::new();
        };
        let Some(identifier) = Self::identifier_at_byte_offset(&document.text, byte_offset) else {
            return Vec::new();
        };
        let symbols = Self::document_symbols_for_text(&document.text);
        let Some(range) = Self::find_symbol_selection_range(&symbols, &identifier) else {
            return Vec::new();
        };
        vec![Location::new(uri.clone(), range)]
    }

    /// Returns the source identifier under a byte offset.
    ///
    /// Inputs:
    /// - `text`: source document text.
    /// - `byte_offset`: byte offset produced from an LSP position.
    ///
    /// Output:
    /// - Identifier text when the offset touches a Terlan identifier.
    /// - `None` when the offset is outside text or on punctuation/whitespace.
    ///
    /// Transformation:
    /// - Expands left and right over ASCII identifier characters. This matches
    ///   the current Terlan identifier subset used by the parser and keeps
    ///   definition lookup conservative for dotted module-member references.
    pub(crate) fn identifier_at_byte_offset(text: &str, byte_offset: usize) -> Option<String> {
        if byte_offset > text.len() || !text.is_char_boundary(byte_offset) {
            return None;
        }
        let bytes = text.as_bytes();
        let mut start = byte_offset;
        if start == text.len() && start > 0 {
            start -= 1;
        }
        if !Self::is_identifier_byte(*bytes.get(start)?) {
            if start == 0 || !Self::is_identifier_byte(bytes[start - 1]) {
                return None;
            }
            start -= 1;
        }
        while start > 0 && Self::is_identifier_byte(bytes[start - 1]) {
            start -= 1;
        }

        let mut end = start;
        while end < bytes.len() && Self::is_identifier_byte(bytes[end]) {
            end += 1;
        }
        (end > start).then(|| text[start..end].to_string())
    }

    /// Checks whether a byte is part of a Terlan identifier.
    ///
    /// Inputs:
    /// - `byte`: candidate source byte.
    ///
    /// Output:
    /// - `true` for ASCII letters, digits, and underscore.
    ///
    /// Transformation:
    /// - Mirrors the initial LSP identifier lookup subset without depending on
    ///   parser internals or allocating.
    pub(crate) fn is_identifier_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    /// Finds a symbol selection range by name.
    ///
    /// Inputs:
    /// - `symbols`: nested document symbols.
    /// - `name`: identifier text under the cursor.
    ///
    /// Output:
    /// - Selection range for the first matching symbol.
    /// - `None` when no symbol name matches.
    ///
    /// Transformation:
    /// - Walks module and child symbols in document order, preserving the same
    ///   ordering editors already receive from `textDocument/documentSymbol`.
    fn find_symbol_selection_range(symbols: &[DocumentSymbol], name: &str) -> Option<Range> {
        for symbol in symbols {
            if symbol.name == name {
                return Some(symbol.selection_range);
            }
            if let Some(children) = &symbol.children {
                if let Some(range) = Self::find_symbol_selection_range(children, name) {
                    return Some(range);
                }
            }
        }
        None
    }

    /// Builds the top-level module document symbol.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `module`: parsed syntax-output module.
    ///
    /// Output:
    /// - One module symbol with declaration children.
    ///
    /// Transformation:
    /// - Converts the module span and declaration payloads into the nested LSP
    ///   symbol shape expected by editors.
    #[allow(deprecated)]
    fn module_document_symbol(text: &str, module: &SyntaxModuleOutput) -> DocumentSymbol {
        let module_span = Span::new(module.span.start, module.span.end);
        let module_range = OpenDocument::range_from_span(text, &module_span);
        let selection_range = Self::symbol_selection_range(text, &module_span, &module.module_name)
            .unwrap_or(module_range);
        let children = module
            .declarations
            .iter()
            .filter_map(|declaration| {
                Self::declaration_document_symbol(text, &declaration.payload, &declaration.span)
            })
            .collect::<Vec<_>>();

        DocumentSymbol {
            name: module.module_name.clone(),
            detail: Some("module".to_string()),
            kind: SymbolKind::MODULE,
            tags: None,
            deprecated: None,
            range: module_range,
            selection_range,
            children: Some(children),
        }
    }

    /// Builds one declaration document symbol.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `payload`: syntax-output declaration payload.
    /// - `span`: declaration source span.
    ///
    /// Output:
    /// - LSP document symbol when the declaration has a user-facing name.
    ///
    /// Transformation:
    /// - Maps compiler declaration variants to stable editor symbol names and
    ///   broad LSP symbol kinds.
    #[allow(deprecated)]
    fn declaration_document_symbol(
        text: &str,
        payload: &SyntaxDeclarationPayload,
        span: &terlan_syntax::ebnf::EbnfSourceSpan,
    ) -> Option<DocumentSymbol> {
        let (name, detail, kind) = Self::declaration_symbol_parts(payload)?;
        let source_span = Span::new(span.start, span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, &name).unwrap_or(range);
        Some(DocumentSymbol {
            name,
            detail: Some(detail),
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: None,
        })
    }

    /// Builds a name-only selection range inside a broader symbol span.
    ///
    /// Inputs:
    /// - `text`: full document text.
    /// - `span`: byte range for the enclosing module or declaration.
    /// - `name`: symbol name to locate inside that range.
    ///
    /// Output:
    /// - LSP range for the first matching symbol name, or `None` if the name
    ///   cannot be found inside the span.
    ///
    /// Transformation:
    /// - Searches only within the compiler-provided span and converts the
    ///   matched byte range back to UTF-16 LSP coordinates.
    fn symbol_selection_range(text: &str, span: &Span, name: &str) -> Option<Range> {
        let start = span.start.min(text.len());
        let end = span.end.min(text.len());
        if start >= end || name.is_empty() {
            return None;
        }
        let haystack = &text[start..end];
        let relative_start = haystack.find(name)?;
        let name_start = start + relative_start;
        let name_end = name_start + name.len();
        Some(OpenDocument::range_from_span(
            text,
            &Span::new(name_start, name_end),
        ))
    }

    /// Returns declaration symbol metadata.
    ///
    /// Inputs:
    /// - `payload`: syntax-output declaration payload.
    ///
    /// Output:
    /// - Symbol name, detail label, and LSP symbol kind for named declarations.
    ///
    /// Transformation:
    /// - Keeps editor symbol naming centralized so future declarations can be
    ///   added without changing the LSP request handler.
    fn declaration_symbol_parts(
        payload: &SyntaxDeclarationPayload,
    ) -> Option<(String, String, SymbolKind)> {
        match payload {
            SyntaxDeclarationPayload::Type { name, .. } => {
                Some((name.clone(), "type".to_string(), SymbolKind::TYPE_PARAMETER))
            }
            SyntaxDeclarationPayload::Struct { name, .. } => {
                Some((name.clone(), "struct".to_string(), SymbolKind::STRUCT))
            }
            SyntaxDeclarationPayload::Constructor { name, .. } => Some((
                name.clone(),
                "constructor".to_string(),
                SymbolKind::CONSTRUCTOR,
            )),
            SyntaxDeclarationPayload::Function { name, .. } => {
                Some((name.clone(), "function".to_string(), SymbolKind::FUNCTION))
            }
            SyntaxDeclarationPayload::Method { name, .. } => {
                Some((name.clone(), "method".to_string(), SymbolKind::METHOD))
            }
            SyntaxDeclarationPayload::Trait { name, .. } => {
                Some((name.clone(), "trait".to_string(), SymbolKind::INTERFACE))
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                ..
            } => Some((
                format!("{} for {}", trait_ref.text, for_type.text),
                "impl".to_string(),
                SymbolKind::INTERFACE,
            )),
            SyntaxDeclarationPayload::AnnotationSchema { path, .. } => {
                Some((path.join("."), "annotation".to_string(), SymbolKind::KEY))
            }
            SyntaxDeclarationPayload::Template { name, .. } => {
                Some((name.clone(), "template".to_string(), SymbolKind::FUNCTION))
            }
            SyntaxDeclarationPayload::Config { name, .. } => {
                Some((name.clone(), "config".to_string(), SymbolKind::OBJECT))
            }
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Raw { .. } => None,
        }
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
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
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
    /// - None; diagnostics are cleared asynchronously when the document was
    ///   tracked.
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
        let language_id = text_document.language_id;
        let parse_error = self
            .open_documents
            .open(uri.clone(), text.clone(), version, language_id);
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
        let language_id = self
            .open_documents
            .snapshot(&uri)
            .map(|document| document.language_id)
            .unwrap_or_else(|| "terlan".to_string());
        let text_changes = params.content_changes;
        let text = text_changes
            .iter()
            .find(|change| change.range.is_none())
            .map(|change| change.text.clone())
            .or_else(|| text_changes.last().map(|change| change.text.clone()))
            .unwrap_or_else(String::new);
        let parse_error = self
            .open_documents
            .open(uri.clone(), text.clone(), version, language_id);
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
    /// - Removes the document from the open-document cache and publishes an
    ///   empty diagnostic set so editors do not keep stale closed-file errors.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(document) = self.open_documents.close(&uri) {
            self.client
                .publish_diagnostics(uri, Vec::new(), Some(document.version))
                .await;
        }
    }

    /// Handles LSP document-symbol requests.
    ///
    /// Inputs:
    /// - `params`: document URI for the requested symbols.
    ///
    /// Output:
    /// - Nested module/declaration symbols for open documents, or an empty
    ///   symbol list when the document is not open or does not parse.
    ///
    /// Transformation:
    /// - Reads the latest open-document snapshot and reuses compiler
    ///   syntax-output parsing to construct editor symbols.
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let symbols = self
            .open_documents
            .snapshot(&params.text_document.uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::document_symbols_for_text(&document.text))
            .unwrap_or_default();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    /// Handles LSP go-to-definition requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Same-document declaration location when one can be resolved.
    /// - Empty definition list otherwise.
    ///
    /// Transformation:
    /// - Uses the latest open-document snapshot and compiler-backed document
    ///   symbols. This first slice intentionally avoids cross-file resolver
    ///   targets until the compiler exposes source locations for imports.
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let locations = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::definition_locations_for_position(&uri, &document, position))
            .unwrap_or_default();
        Ok(Some(GotoDefinitionResponse::Array(locations)))
    }

    /// Handles LSP hover requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Markdown hover content for source symbols when documentation exists.
    /// - `None` for template documents, parse errors, or undocumented spans.
    ///
    /// Transformation:
    /// - Reuses compiler syntax output and packaged interface summaries so
    ///   VS Code and other clients receive the same docs shipped with std and
    ///   project interfaces.
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let hover = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .and_then(|document| hover_for_position(&uri, &document, position));
        Ok(hover)
    }

    /// Handles LSP code-action requests.
    ///
    /// Inputs:
    /// - `params`: document URI, requested range, and diagnostics supplied by
    ///   the editor client.
    ///
    /// Output:
    /// - Quick-fix actions for supported diagnostics.
    ///
    /// Transformation:
    /// - Reads the current open document snapshot, recognizes unresolved-name
    ///   diagnostics, and delegates import-edit construction to the compiler
    ///   summary-backed import action module.
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(document) = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
        else {
            return Ok(Some(Vec::new()));
        };

        let actions = params
            .context
            .diagnostics
            .iter()
            .flat_map(|diagnostic| {
                import_code_actions_for_diagnostic(&uri, &document.text, &diagnostic.message)
            })
            .map(CodeActionOrCommand::CodeAction)
            .collect::<Vec<_>>();
        Ok(Some(actions))
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
