pub(super) use super::super::document::{DocumentKind, OpenDocument, OpenDocuments};
pub(super) use super::super::Backend;
pub(super) use super::semantic_tokens_and_diagnostic_support::{
    assert_clear_diagnostic_message, assert_parse_diagnostic_message,
    assert_resolve_diagnostic_message, assert_type_diagnostic_message, assert_type_warning_message,
    read_lsp_message, write_lsp_message,
};
pub(super) use crate::terlan_syntax::ebnf::EbnfSourceSpan;
pub(super) use crate::terlan_syntax::{
    format_source_module, Span, SyntaxParamOutput, SyntaxTypeOutput,
};
pub(super) use std::fs;
pub(super) use std::io::{self as std_io, ErrorKind};
pub(super) use std::time::{SystemTime, UNIX_EPOCH};
pub(super) use tokio::io::duplex;
pub(super) use tokio::time::{timeout, Duration};
pub(super) use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, Position, SymbolKind, Url,
};
pub(super) use tower_lsp::{LspService, Server};

/// Returns Markdown/text documentation from a completion item.
///
/// Inputs:
/// - `item`: LSP completion item.
///
/// Output:
/// - Documentation text, or an empty string for undocumented completions.
///
/// Transformation:
/// - Normalizes both LSP documentation variants for compact assertions.
pub(super) fn completion_doc_text(item: &CompletionItem) -> &str {
    match item.documentation.as_ref() {
        Some(Documentation::MarkupContent(markup)) => markup.value.as_str(),
        Some(Documentation::String(text)) => text.as_str(),
        None => "",
    }
}
