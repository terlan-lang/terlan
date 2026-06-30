pub mod ebnf;
mod ebnf_lexer;
pub mod formatter;
pub mod lexer;
pub mod native;
mod parse_tree;
mod parser;
mod parser_contract;
pub mod span;
pub mod syntax_contract;
pub mod syntax_output;
pub mod token;

pub use ebnf::*;
pub use formatter::{format_interface_source_module, format_source_module};
pub use lexer::*;
pub use native::*;
#[cfg(test)]
pub(crate) use parser::{parse_interface_module, parse_module, parse_terlan_expr};
pub use parser::{ParseResult, ParserError};
pub use span::Span;
pub use syntax_contract::{
    cached_canonical_terlan_syntax_contract, cached_canonical_terlan_syntax_contract_artifact,
    cached_canonical_terlan_syntax_contract_artifact_json,
    cached_canonical_terlan_syntax_contract_identity,
    cached_canonical_terlan_syntax_contract_identity_json, canonical_terlan_syntax_contract,
    check_syntax_contract_artifact_against_current, ensure_canonical_syntax_contract_valid,
    extract_syntax_contract_artifact_fingerprint, syntax_contract_artifact_matches_current,
    syntax_contract_fingerprint, syntax_contract_identity_from_fingerprint,
    syntax_contract_identity_matches_current, validate_syntax_contract,
    validated_canonical_terlan_syntax_contract, SyntaxContractArtifact,
    SyntaxContractArtifactCheck, SyntaxContractDiagnostic, SyntaxContractError,
    SyntaxContractIdentity, CANONICAL_TERLAN_EBNF, SYNTAX_CONTRACT_ARTIFACT_SCHEMA,
    SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM,
};
pub use syntax_output::{
    parse_expr_as_syntax_output, parse_interface_module_as_syntax_output,
    parse_module_as_syntax_output, SyntaxClauseOutput, SyntaxConfigEntryOutput,
    SyntaxConfigValueOutput, SyntaxConstructorClauseOutput, SyntaxConstructorParamOutput,
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExportItem, SyntaxExprFieldOutput,
    SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput, SyntaxHtmlAttrOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNamedSlotOutput,
    SyntaxHtmlNodeOutput, SyntaxImplMethodOutput, SyntaxImportItem, SyntaxImportKind,
    SyntaxModuleOutput, SyntaxParamOutput, SyntaxPatternFieldOutput, SyntaxPatternKind,
    SyntaxPatternOutput, SyntaxSourceKind, SyntaxStructFieldOutput, SyntaxTemplatePropOutput,
    SyntaxTraitMethodOutput, SyntaxTypeOutput, SYNTAX_MODULE_OUTPUT_SCHEMA,
};
pub use token::{Token, TokenKind};

/// Removes Terlan single-quoted atom delimiters and simple escapes.
///
/// Inputs:
/// - `text`: quoted atom text including leading and trailing single quotes.
///
/// Output:
/// - Unescaped atom payload, or `None` when delimiters are missing.
///
/// Transformation:
/// - Copies escaped characters literally after dropping the escape marker.
pub(crate) fn unquote_single_quoted_atom(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Some(output)
}

/// Escapes text as a double-quoted Terlan source string literal.
///
/// Inputs:
/// - `value`: unescaped string payload.
///
/// Output:
/// - Double-quoted literal text.
///
/// Transformation:
/// - Escapes backslash, double quote, newline, carriage return, and tab
///   characters using the portable escaping accepted by Terlan source and the
///   backend literal contexts used by Rust, JavaScript, and TypeScript emitters.
pub(crate) fn quoted_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
