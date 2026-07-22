use crate::terlan_syntax::{
    ebnf::EbnfCompileError,
    lexer::lex,
    parse_tree::{
        Annotation, AnnotationEntry, AnnotationKeyOption, AnnotationSchemaDecl,
        AnnotationSchemaEntry, AnnotationValue, AnnotationValueType, BinaryLayoutField, BinaryOp,
        BuiltinBlockMacro, CaseClause, ConstFunctionDecl, ConstantDecl, ConstructorClause,
        ConstructorDecl, ConstructorParam, Decl, ExportDecl, ExportItem, Expr, FunctionClause,
        FunctionDecl, HtmlAttr, HtmlAttrValue, HtmlBlockExpr, HtmlElement, HtmlNamedSlot, HtmlNode,
        IfClause, ImplConstDecl, ImportDecl, ImportItem, ImportKind, LetBinding, MapExprField,
        MapField, MethodDecl, Module, Param, Pattern, ShapeDecl, StructDecl, StructFieldDecl,
        TemplateDecl, TemplatePropDecl, TraitConstDecl, TraitDecl, TraitImplDecl, TraitMethodDecl,
        TryAfterClause, TypeDecl, TypeExpr, UnaryOp, UnsupportedDecl, ValuedUnionArmDecl,
    },
    span::Span,
    syntax_contract::{
        ensure_canonical_syntax_contract_valid as ensure_syntax_contract_valid, SyntaxContractError,
    },
    token::{Token, TokenKind},
    unquote_single_quoted_atom,
};

use helpers::*;
use html::parse_html_nodes;
pub(crate) use html::parse_terlan_expr;
use nesting::ensure_token_nesting_within_limit;
pub(crate) use repeated_lets::{
    parse_module, parse_module_for_repeated_let_migration, repeated_let_migration_offsets,
};

/// Parses one standalone Terlan pattern for compiler-owned expansion passes.
///
/// Inputs:
/// - `input`: pattern source without a declaration terminator.
///
/// Output:
/// - Parsed pattern when the entire input is consumed.
///
/// Transformation:
/// - Reuses the canonical lexer, nesting guard, and pattern parser so
///   compile-time syntax features cannot acquire a second pattern grammar.
pub(crate) fn parse_terlan_pattern(input: &str) -> ParseResult<Pattern> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;
    let tokens = match lex(input) {
        Ok(tokens) => tokens,
        Err(errors) => {
            let first = errors.into_iter().next().ok_or_else(|| ParseError {
                message: "lexical failure".to_string(),
                span: Span::new(0, 0),
            })?;
            return Err(ParseError {
                message: first.message,
                span: first.span,
            });
        }
    };
    ensure_token_nesting_within_limit(&tokens)?;

    let mut parser = Parser::new(tokens, LetBindingMode::Canonical);
    let pattern = parser.parse_pattern()?;
    if !parser.check(TokenKind::EOF) {
        return Err(ParseError {
            message: "unexpected tokens after pattern".to_string(),
            span: parser.current().span(),
        });
    }
    Ok(pattern)
}

/// Parser diagnostic with message and source span.
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// Result type returned by parser operations.
pub type ParseResult<T> = Result<T, ParseError>;
/// Backwards-compatible parser error alias.
pub type ParserError = ParseError;

/// Parses a generated interface module.
///
/// Inputs:
/// - `input`: complete `.terli`-style interface source text.
///
/// Output:
/// - Parsed module tree, or the first lexer/parser/contract diagnostic.
///
/// Transformation:
/// - Validates the canonical syntax contract, lexes the source, and consumes an
///   interface module where declarations may be signatures.
pub(crate) fn parse_interface_module(input: &str) -> ParseResult<Module> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;

    let tokens = match lex(input) {
        Ok(tokens) => tokens,
        Err(errors) => {
            let first = errors.into_iter().next().ok_or_else(|| ParseError {
                message: "lexical failure".to_string(),
                span: Span::new(0, 0),
            })?;
            return Err(ParseError {
                message: first.message,
                span: first.span,
            });
        }
    };

    ensure_token_nesting_within_limit(&tokens)?;

    let mut parser = Parser::new(tokens, LetBindingMode::Canonical);
    parser.parse_interface_module()
}

/// Converts syntax-contract failures into parser diagnostics.
///
/// Inputs:
/// - `error`: syntax-contract compile or validation failure.
///
/// Output:
/// - Parser error with a source span suitable for existing diagnostics.
///
/// Transformation:
/// - Preserves the first contract diagnostic span when available and otherwise
///   anchors the failure at the start of the input.
fn syntax_contract_parse_error(error: SyntaxContractError) -> ParseError {
    let (message, span) = match error {
        SyntaxContractError::Compile(error) => match error {
            EbnfCompileError::Parse(message, span) => (
                format!("canonical syntax contract failed to compile: {message}"),
                span,
            ),
            EbnfCompileError::Serialize(message) => (
                format!("canonical syntax contract failed to serialize: {message}"),
                Span::new(0, 0),
            ),
        },
        SyntaxContractError::Validation(diagnostics) => {
            if let Some(first) = diagnostics.into_iter().next() {
                (
                    format!(
                        "canonical syntax contract failed validation: {}",
                        first.message
                    ),
                    first.span,
                )
            } else {
                (
                    "canonical syntax contract failed validation".to_string(),
                    Span::new(0, 0),
                )
            }
        }
    };

    ParseError { message, span }
}

/// Stateful recursive-descent parser over lexer tokens.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    let_binding_mode: LetBindingMode,
    implicit_let_binding_offsets: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LetBindingMode {
    Canonical,
    MigrateImplicit,
}
