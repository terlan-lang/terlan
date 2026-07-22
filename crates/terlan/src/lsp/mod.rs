use std::collections::HashSet;
use std::path::{Path, PathBuf};
mod document;
mod hover;
mod import_actions;
mod server;
mod template_completion;

pub use server::run_stdio_server;

use crate::terlan_hir::{ConstructorSignature, FunctionSignature};
use crate::terlan_syntax::{
    lex, parse_module_as_syntax_output, token::TokenKind, ParserError, Span,
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxImplMethodOutput, SyntaxModuleOutput,
    SyntaxParamOutput, SyntaxStructFieldOutput, SyntaxTraitMethodOutput,
};
use crate::terlan_typeck::DiagSeverity;
use document::{OpenDocument, OpenDocuments};
use hover::hover_for_position;
use import_actions::import_code_actions_for_diagnostic;
use template_completion::template_completion_items;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::request::{
    GotoDeclarationParams, GotoDeclarationResponse, GotoImplementationParams,
    GotoImplementationResponse, GotoTypeDefinitionParams, GotoTypeDefinitionResponse,
};
use tower_lsp::{lsp_types::*, Client, LanguageServer};

fn is_semantic_constant_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && name.bytes().any(|byte| byte.is_ascii_uppercase())
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

include!("backend_impl_part_001.rs");
include!("backend_impl_part_002.rs");
include!("backend_impl_part_003.rs");
include!("backend_impl_part_004.rs");
include!("backend_impl_part_005.rs");

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
                declaration_provider: Some(DeclarationCapability::Simple(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::KEYWORD,
                                    SemanticTokenType::VARIABLE,
                                ],
                                token_modifiers: vec![SemanticTokenModifier::READONLY],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
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

    /// Handles LSP go-to-declaration requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Same locations returned by go-to-definition for Terlan's current
    ///   declaration/definition surface.
    ///
    /// Transformation:
    /// - Reuses the compiler-backed definition resolver until declaration and
    ///   definition semantics diverge in the language model.
    async fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let locations = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::definition_locations_for_position(&uri, &document, position))
            .unwrap_or_default();
        Ok(Some(GotoDeclarationResponse::Array(locations)))
    }

    /// Handles LSP go-to-type-definition requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Same locations returned by go-to-definition for Terlan's current type
    ///   declaration surface.
    ///
    /// Transformation:
    /// - Reuses the compiler-backed definition resolver until type-definition
    ///   semantics diverge from ordinary definition navigation in the language
    ///   model.
    async fn goto_type_definition(
        &self,
        params: GotoTypeDefinitionParams,
    ) -> Result<Option<GotoTypeDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let locations = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::definition_locations_for_position(&uri, &document, position))
            .unwrap_or_default();
        Ok(Some(GotoTypeDefinitionResponse::Array(locations)))
    }

    /// Handles LSP go-to-implementation requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Same locations returned by go-to-definition for Terlan's current
    ///   implementation-aware receiver-method navigation surface.
    ///
    /// Transformation:
    /// - Reuses the compiler-backed definition resolver, which already prefers
    ///   explicit impl methods for receiver-call member references.
    async fn goto_implementation(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let locations = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::definition_locations_for_position(&uri, &document, position))
            .unwrap_or_default();
        Ok(Some(GotoImplementationResponse::Array(locations)))
    }

    /// Handles LSP find-references requests.
    ///
    /// Inputs:
    /// - `params`: document URI, cursor position, and reference context.
    ///
    /// Output:
    /// - Same-document locations for the identifier under the cursor.
    ///
    /// Transformation:
    /// - Keeps the first reference provider conservative by scanning exact
    ///   identifier-token occurrences in the latest open source document.
    /// - Honors `includeDeclaration` by removing locations that overlap the
    ///   current definition resolver's target ranges.
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let Some(document) = self.open_documents.snapshot(&uri) else {
            return Ok(Some(Vec::new()));
        };
        if !document.is_source_like() {
            return Ok(Some(Vec::new()));
        }

        let mut locations = Self::reference_locations_for_position(&uri, &document, position);
        if !params.context.include_declaration {
            let definitions = Self::definition_locations_for_position(&uri, &document, position);
            locations.retain(|location| {
                !Self::is_reference_declaration_range(&document, location.range)
                    && !definitions.iter().any(|definition| {
                        definition.uri == location.uri && definition.range == location.range
                    })
            });
        }
        Ok(Some(locations))
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

    /// Handles LSP completion requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Completion items for currently supported language surfaces.
    ///
    /// Transformation:
    /// - Reuses compiler syntax output and generated summaries so editor
    ///   completion tracks the same shape declarations as hover/docs.
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let items = self
            .open_documents
            .snapshot(&uri)
            .map_or_else(Vec::new, |document| {
                if document.is_source_like() {
                    Self::completion_items_for_position(&uri, &document, position)
                } else {
                    template_completion_items(&uri, &document, position)
                }
            });
        Ok(Some(CompletionResponse::Array(items)))
    }

    /// Handles LSP signature-help requests.
    ///
    /// Inputs:
    /// - `params`: document URI and cursor position.
    ///
    /// Output:
    /// - Signature help for supported local function calls.
    ///
    /// Transformation:
    /// - Reads the latest open document and projects compiler syntax-output
    ///   callable metadata into standard LSP signature-help payloads.
    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let signature_help = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .and_then(|document| Self::signature_help_for_position(&document, &uri, position));
        Ok(signature_help)
    }

    /// Handles LSP inlay-hint requests.
    ///
    /// Inputs:
    /// - `params`: document URI and requested visible range.
    ///
    /// Output:
    /// - Deterministic inlay hints for the supported source range.
    ///
    /// Transformation:
    /// - Reads the latest open document and emits conservative type hints for
    ///   simple inferred literal bindings.
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let hints = self
            .open_documents
            .snapshot(&uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::inlay_hints_for_range(&document, &uri, params.range))
            .unwrap_or_default();
        Ok(Some(hints))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let tokens = self
            .open_documents
            .snapshot(&params.text_document.uri)
            .filter(OpenDocument::is_source_like)
            .map(|document| Self::value_lifecycle_semantic_tokens(&document));
        Ok(tokens.map(SemanticTokensResult::Tokens))
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

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;

#[cfg(test)]
#[path = "trait_negative_test.rs"]
mod trait_negative_test;
