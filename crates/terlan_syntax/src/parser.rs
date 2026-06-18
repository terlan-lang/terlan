use crate::{
    ebnf::EbnfCompileError,
    lexer::lex,
    parse_tree::{
        Annotation, AnnotationEntry, AnnotationKeyOption, AnnotationSchemaDecl,
        AnnotationSchemaEntry, AnnotationValue, AnnotationValueType, BinaryOp, BuiltinBlockMacro,
        CaseClause, ConstructorClause, ConstructorDecl, ConstructorParam, Decl, ExportDecl,
        ExportItem, Expr, FunctionClause, FunctionDecl, HtmlAttr, HtmlAttrValue, HtmlBlockExpr,
        HtmlElement, HtmlNamedSlot, HtmlNode, IfClause, ImportDecl, ImportItem, ImportKind,
        LetBinding, MapExprField, MapField, MethodDecl, Module, Param, Pattern, StructDecl,
        StructFieldDecl, TemplateDecl, TemplatePropDecl, TraitDecl, TraitImplDecl, TraitMethodDecl,
        TryAfterClause, TypeDecl, TypeExpr, UnaryOp, UnsupportedDecl,
    },
    span::Span,
    syntax_contract::{
        ensure_canonical_syntax_contract_valid as ensure_syntax_contract_valid, SyntaxContractError,
    },
    token::{Token, TokenKind},
};

mod annotations;
mod callables;
mod expressions;
mod html;
mod imports;
mod modules;
mod patterns;
mod type_decls;
mod types;

use html::parse_html_nodes;
pub(crate) use html::parse_terlan_expr;

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

/// Parses a full Terlan source module.
///
/// Inputs:
/// - `input`: complete Terlan source text.
///
/// Output:
/// - Parsed module tree, or the first lexer/parser/contract diagnostic.
///
/// Transformation:
/// - Validates the canonical syntax contract, lexes the source, and consumes a
///   normal source module with declaration bodies.
pub(crate) fn parse_module(input: &str) -> ParseResult<Module> {
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

    let mut parser = Parser::new(tokens);
    parser.parse_module()
}

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

    let mut parser = Parser::new(tokens);
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
}

impl Parser {
    /// Creates a parser cursor over a token stream.
    ///
    /// Inputs:
    /// - `tokens`: lexer output terminated by EOF.
    ///
    /// Output:
    /// - Parser positioned at the first token.
    ///
    /// Transformation:
    /// - Stores tokens without modification and initializes the cursor.
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parses a public source declaration after consuming no tokens yet.
    ///
    /// Inputs:
    /// - Parser cursor at `pub`.
    ///
    /// Output:
    /// - Parsed declaration with public visibility.
    ///
    /// Transformation:
    /// - Consumes `pub` and dispatches to the declaration parser for the next
    ///   keyword or function head.
    fn parse_pub_decl(&mut self) -> ParseResult<Decl> {
        self.expect_keyword(TokenKind::Pub)?;
        match self.current().kind {
            TokenKind::Type => self.parse_type_decl(false, true),
            TokenKind::Opaque => self.parse_type_decl(true, true),
            TokenKind::Struct => self.parse_struct_decl(true),
            TokenKind::Constructor => self.parse_constructor_decl(true),
            TokenKind::Trait => self.parse_trait_decl(true),
            TokenKind::Impl => self.parse_trait_impl_decl(true),
            TokenKind::LParen => self.parse_method_decl(true),
            TokenKind::Atom if self.current().text == "annotation" => {
                self.parse_annotation_schema_decl(true)
            }
            TokenKind::Macro => {
                self.bump();
                self.parse_function_decl(true, true)
            }
            TokenKind::Atom | TokenKind::Var => self.parse_function_decl(true, false),
            _ => Err(ParseError {
                message: "expected declaration after `pub`".to_string(),
                span: self.current().span(),
            }),
        }
    }

    /// Parses a public interface declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `pub` inside an interface module.
    ///
    /// Output:
    /// - Parsed public interface declaration or signature.
    ///
    /// Transformation:
    /// - Consumes `pub` and dispatches to interface-aware declaration parsers.
    fn parse_pub_interface_decl(&mut self) -> ParseResult<Decl> {
        self.expect_keyword(TokenKind::Pub)?;
        match self.current().kind {
            TokenKind::Type => self.parse_type_interface_decl(false, true),
            TokenKind::Opaque => self.parse_type_interface_decl(true, true),
            TokenKind::Struct => self.parse_struct_decl(true),
            TokenKind::Constructor => self.parse_constructor_decl(true),
            TokenKind::Trait => self.parse_trait_decl(true),
            TokenKind::Impl => self.parse_trait_impl_interface_decl(true),
            TokenKind::LParen => self.parse_method_signature_decl(true),
            TokenKind::Atom if self.current().text == "annotation" => {
                self.parse_annotation_schema_decl(true)
            }
            TokenKind::Macro => {
                self.bump();
                self.parse_function_signature_decl(true, true)
            }
            TokenKind::Atom | TokenKind::Var => self.parse_function_signature_decl(true, false),
            _ => Err(ParseError {
                message: "expected declaration after `pub`".to_string(),
                span: self.current().span(),
            }),
        }
    }

    /// Parses a template declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `template`.
    ///
    /// Output:
    /// - Parsed template declaration.
    ///
    /// Transformation:
    /// - Consumes the template header, source path, typed props, and terminating
    ///   dot into a `TemplateDecl`.
    fn parse_template_decl(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Template)?;
        let name = self.expect_type_name()?;
        if !self.consume_keyword("from") {
            return Err(ParseError {
                message: "expected `from` in template declaration".to_string(),
                span: self.current().span(),
            });
        }
        let raw_path = self.expect(TokenKind::String)?.text.clone();
        let source_path = raw_path
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(&raw_path)
            .to_string();
        self.expect(TokenKind::LBrace)?;

        let mut props = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                self.skip_comments();
                let _docs = self.take_item_docs();
                self.skip_comments();
                let prop_start = self.current().start;
                let prop_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let annotation = self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBrace])?;
                props.push(TemplatePropDecl {
                    name: prop_name,
                    annotation,
                    span: Span::new(prop_start, self.previous().end),
                });

                if self.consume_if(TokenKind::Comma) {
                    continue;
                }
                break;
            }

            self.expect(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Template(TemplateDecl {
            name,
            source_path,
            props,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Reports whether the cursor starts a template declaration.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - `true` when the next tokens match `template Name from`.
    ///
    /// Transformation:
    /// - Peeks ahead without advancing.
    fn is_template_decl_start(&self) -> bool {
        self.current().text == "template"
            && matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if matches!(token.kind, TokenKind::Atom | TokenKind::Var)
            )
            && matches!(
                self.tokens.get(self.pos + 2),
                Some(token) if token.text == "from"
            )
    }

    /// Parses a raw unsupported declaration block.
    ///
    /// Inputs:
    /// - `kind`: raw declaration kind selected by the caller.
    ///
    /// Output:
    /// - Unsupported declaration preserving raw text for downstream diagnostics.
    ///
    /// Transformation:
    /// - Consumes nested braces until the declaration terminator and joins the
    ///   token text into a stable raw declaration payload.
    fn parse_raw_decl(&mut self, kind: String) -> ParseResult<Decl> {
        let start = self.current().start;
        let mut parts = vec![kind.clone()];
        if self.current().text == kind {
            self.bump();
        }

        let mut depth = if self.consume_if(TokenKind::LBrace) {
            parts.push("{".to_string());
            1
        } else {
            0
        };

        let mut found_dot = false;

        while !self.check(TokenKind::EOF) {
            if self.consume_if(TokenKind::Dot) {
                if depth == 0 {
                    found_dot = true;
                    break;
                }
                parts.push(".".to_string());
                continue;
            }

            if self.consume_if(TokenKind::LBrace) {
                depth += 1;
                parts.push("{".to_string());
                continue;
            }

            if self.consume_if(TokenKind::RBrace) {
                if depth == 0 {
                    return Err(ParseError {
                        message: format!("unterminated {} declaration", kind),
                        span: Span::new(start, self.current().end),
                    });
                }
                depth -= 1;
                parts.push("}".to_string());
                if depth == 0 {
                    if self.check(TokenKind::Dot) {
                        self.bump();
                    }
                    found_dot = true;
                    break;
                }
                continue;
            }

            parts.push(self.bump().text);
        }

        if parts.is_empty() {
            return Err(ParseError {
                message: format!("malformed {} declaration", kind),
                span: Span::new(start, self.current().end),
            });
        }

        if !found_dot {
            return Err(ParseError {
                message: format!("unterminated {} declaration", kind),
                span: Span::new(start, self.current().end),
            });
        }

        Ok(Decl::Raw(UnsupportedDecl {
            kind,
            text: parts.join(" "),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses an interface export declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `export` inside an interface module.
    ///
    /// Output:
    /// - Parsed export declaration.
    ///
    /// Transformation:
    /// - Accepts type export lists or function arity exports and consumes the
    ///   terminating dot.
    fn parse_interface_export(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Export)?;
        if self.consume_keyword("type") {
            if self.consume_if(TokenKind::LParen) {
                loop {
                    self.expect_ident()?;
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    break;
                }
                self.expect(TokenKind::RParen)?;
            } else {
                loop {
                    self.expect_ident()?;
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    break;
                }
            }

            self.expect(TokenKind::Dot)?;
            return Ok(Decl::Export(ExportDecl {
                items: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }

        let mut items = Vec::new();
        loop {
            let name = self.expect_ident()?;
            if !self.consume_if(TokenKind::Slash) {
                return Err(ParseError {
                    message: "expected function arity in interface export".to_string(),
                    span: self.current().span(),
                });
            }

            let arity = {
                self.expect(TokenKind::Int)?;
                self.previous()
                    .text
                    .parse::<usize>()
                    .map_err(|_| ParseError {
                        message: "expected numeric arity".to_string(),
                        span: self.previous().span(),
                    })?
            };

            items.push(ExportItem {
                name,
                arity,
                span: Span::new(self.previous().start, self.previous().end),
            });

            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            break;
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Export(ExportDecl {
            items,
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a canonical type-like declaration name.
    ///
    /// Inputs: the parser cursor must point at a `TypeName` position in a
    /// declaration head.
    /// Outputs: the type name text or a syntax diagnostic at the offending
    /// token.
    /// Transformation: consumes only lexer `Var` tokens, which represent
    /// upper-case identifiers in Terlan source mode.
    fn expect_type_name(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Atom => Err(ParseError {
                message: "expected upper-case type name".to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected type name".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a lower-case source identifier for a grammar position that
    /// explicitly requires `LowerIdent`.
    ///
    /// Inputs: the parser cursor must point at the expected lower-case
    /// identifier, and `message` describes the grammar position for diagnostics.
    /// Outputs: the identifier text or a syntax diagnostic at the offending
    /// token.
    /// Transformation: consumes only lexer `Atom` tokens, preserving the source
    /// spelling of the lower-case identifier.
    fn expect_lower_ident(&mut self, message: &str) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Var => Err(ParseError {
                message: message.to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected lower-case identifier".to_string(),
                span: token.span(),
            }),
        }
    }

    fn parse_raw_block(&mut self) -> ParseResult<String> {
        let start = self.current().start;
        self.expect(TokenKind::LBrace)?;

        let mut depth = 1usize;
        let mut raw = String::new();
        let mut previous_end = start + 1;
        while !self.check(TokenKind::EOF) {
            let token = self.bump();
            if token.start > previous_end {
                raw.push(' ');
            }
            match token.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    raw.push_str(&token.text);
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(raw);
                    }
                    raw.push_str(&token.text);
                }
                _ => raw.push_str(&token.text),
            }
            previous_end = token.end;
        }

        Err(ParseError {
            message: "unterminated html block".to_string(),
            span: Span::new(start, self.previous().end),
        })
    }
}

impl Parser {
    /// Consumes a fixed lexer keyword token.
    ///
    /// Inputs:
    /// - `expected`: exact token kind expected at the cursor.
    ///
    /// Output:
    /// - `Ok(())` after consuming the token, or a parser diagnostic.
    ///
    /// Transformation:
    /// - Delegates to `expect` and discards the consumed token payload.
    fn expect_keyword(&mut self, expected: TokenKind) -> ParseResult<()> {
        self.expect(expected).map(|_| ())
    }

    /// Consumes an expected contextual keyword.
    ///
    /// Inputs:
    /// - `expected`: lower-case keyword text expected in the current grammar
    ///   position.
    ///
    /// Output:
    /// - `Ok(())` when the current token is the expected contextual keyword.
    ///
    /// Transformation:
    /// - Advances over an identifier token with matching text without making
    ///   the word globally reserved in the lexer.
    fn expect_contextual_keyword(&mut self, expected: &str) -> ParseResult<()> {
        if self.check_keyword(expected) {
            self.pos += 1;
            return Ok(());
        }
        Err(ParseError {
            message: format!("expected `{expected}`"),
            span: self.current().span(),
        })
    }

    /// Consumes a contextual keyword if present.
    ///
    /// Inputs:
    /// - `expected`: lower-case contextual keyword text.
    ///
    /// Output:
    /// - `true` when the token was consumed.
    ///
    /// Transformation:
    /// - Advances over matching identifier-like tokens without reserving the
    ///   word globally.
    fn consume_keyword(&mut self, expected: &str) -> bool {
        if self.check_keyword(expected) {
            self.pos += 1;
            return true;
        }
        false
    }

    /// Parses a source identifier in a permissive identifier position.
    ///
    /// Inputs:
    /// - Parser cursor at an identifier-like token.
    ///
    /// Output:
    /// - Identifier text or a parser diagnostic.
    ///
    /// Transformation:
    /// - Accepts lower and upper identifier tokens and consumes the token.
    fn expect_ident(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            _ => Err(ParseError {
                message: "expected identifier".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a canonical `Binding` name.
    ///
    /// Inputs:
    /// - Parser cursor at the token that should name a local binding.
    ///
    /// Output:
    /// - Accepted binding name text.
    ///
    /// Transformation:
    /// - Accepts lower identifiers and ignored lower identifiers while
    ///   rejecting `_` and uppercase names in value-binding position.
    fn expect_binding_name(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        if self.is_binding_token(&token) {
            self.bump();
            Ok(token.text)
        } else {
            Err(ParseError {
                message: "expected lower-case binding name".to_string(),
                span: token.span(),
            })
        }
    }

    /// Reports whether a token is a canonical `Binding` token.
    ///
    /// Inputs:
    /// - `token`: token candidate from the parser stream.
    ///
    /// Output:
    /// - `true` for `LowerIdent` and `_LowerIdent` spellings.
    ///
    /// Transformation:
    /// - Applies the EBNF `Binding ::= LowerIdent | "_" LowerIdent` rule to
    ///   already-lexed identifier tokens.
    fn is_binding_token(&self, token: &Token) -> bool {
        matches!(token.kind, TokenKind::Atom)
            && token.text != "_"
            && token
                .text
                .strip_prefix('_')
                .map(|tail| {
                    tail.chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_lowercase())
                })
                .unwrap_or(true)
    }

    /// Parses an atom literal payload after `:`.
    ///
    /// Inputs:
    /// - Parser cursor at a lower identifier or quoted atom payload.
    ///
    /// Output:
    /// - Atom payload without quote delimiters where possible.
    ///
    /// Transformation:
    /// - Consumes the atom name token and unquotes quoted interop atoms.
    fn expect_atom_literal_name(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::String => {
                self.bump();
                Ok(unquote_single_quoted_atom(&token.text).unwrap_or(token.text))
            }
            _ => Err(ParseError {
                message: "expected atom literal name after ':'".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Consumes an exact token kind.
    ///
    /// Inputs:
    /// - `expected`: token kind required at the parser cursor.
    ///
    /// Output:
    /// - Consumed token on success, otherwise a parser diagnostic at the cursor.
    ///
    /// Transformation:
    /// - Advances one token only when the kind matches.
    fn expect(&mut self, expected: TokenKind) -> ParseResult<Token> {
        let token = self.current().clone();
        if token.kind == expected {
            Ok(self.bump())
        } else {
            Err(ParseError {
                message: format!("expected {:?}", expected),
                span: token.span(),
            })
        }
    }

    /// Consumes a token when its kind matches.
    ///
    /// Inputs:
    /// - `kind`: token kind to consume opportunistically.
    ///
    /// Output:
    /// - `true` when a token was consumed.
    ///
    /// Transformation:
    /// - Checks the current token and advances the cursor on match.
    fn consume_if(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Checks the current token kind.
    ///
    /// Inputs:
    /// - `kind`: token kind to compare against the current token.
    ///
    /// Output:
    /// - `true` when the current token kind matches.
    ///
    /// Transformation:
    /// - Reads the cursor without advancing.
    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    /// Checks whether the current token matches any candidate kind.
    ///
    /// Inputs:
    /// - `kinds`: accepted token kinds.
    ///
    /// Output:
    /// - `true` when any candidate matches the current token.
    ///
    /// Transformation:
    /// - Runs `check` over the candidate list without advancing.
    fn check_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|kind| self.check(kind.clone()))
    }

    /// Skips ordinary comments.
    ///
    /// Inputs:
    /// - Parser cursor at any token.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Advances over non-doc comments and stops at the first non-comment.
    fn skip_comments(&mut self) {
        while self.check(TokenKind::Comment) {
            self.pos += 1;
        }
    }

    /// Rejects module documentation after the module declaration.
    ///
    /// Inputs:
    /// - Parser cursor at a possible documentation token.
    ///
    /// Output:
    /// - `Ok(())` when no misplaced module docs are present.
    ///
    /// Transformation:
    /// - Converts misplaced `//!` or `@module` block docs into parser errors.
    fn reject_misplaced_module_docs(&self) -> ParseResult<()> {
        if self.check(TokenKind::ModuleDocComment) {
            return Err(ParseError {
                message: "module doc comments (`//!`) must appear before the module declaration"
                    .to_string(),
                span: self.current().span(),
            });
        }
        if self.check(TokenKind::DocBlockComment) && is_module_doc_block(&self.current().text) {
            return Err(ParseError {
                message: "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
                    .to_string(),
                span: self.current().span(),
            });
        }

        Ok(())
    }

    /// Consumes item documentation comments.
    ///
    /// Inputs:
    /// - Parser cursor at zero or more item doc tokens.
    ///
    /// Output:
    /// - Raw documentation token text in source order.
    ///
    /// Transformation:
    /// - Advances over `///` and non-module `/** ... */` doc tokens.
    fn take_item_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::DocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    /// Consumes module documentation comments.
    ///
    /// Inputs:
    /// - Parser cursor at zero or more module doc tokens.
    ///
    /// Output:
    /// - Raw module documentation token text in source order.
    ///
    /// Transformation:
    /// - Advances over `//!` and module doc block tokens.
    fn take_module_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::ModuleDocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    /// Advances the parser by one token.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - Token that was current before advancing.
    ///
    /// Transformation:
    /// - Clones the token and increments the cursor position.
    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        self.pos += 1;
        token
    }

    /// Returns the previously consumed token.
    ///
    /// Inputs:
    /// - Parser state after at least one token has been consumed.
    ///
    /// Output:
    /// - Reference to the previous token.
    ///
    /// Transformation:
    /// - Indexes the token stream at `pos - 1`.
    fn previous(&self) -> &Token {
        &self.tokens[self.pos - 1]
    }

    /// Checks for a contextual keyword at the cursor.
    ///
    /// Inputs:
    /// - `expected`: contextual keyword text.
    ///
    /// Output:
    /// - `true` when the current identifier-like token has matching text.
    ///
    /// Transformation:
    /// - Treats atom and upper-identifier tokens as contextual keyword carriers.
    fn check_keyword(&self, expected: &str) -> bool {
        matches!(self.current().kind, TokenKind::Atom | TokenKind::Var)
            && self.current().text == expected
    }

    /// Returns the current parser token.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - Reference to the current token.
    ///
    /// Transformation:
    /// - Indexes the token stream without advancing.
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }
}

/// Removes quote delimiters from a single-quoted atom payload.
///
/// Inputs:
/// - `text`: raw token text including single quotes.
///
/// Output:
/// - Unescaped atom payload, or `None` if delimiters are missing.
///
/// Transformation:
/// - Drops escape markers and copies escaped characters literally.
fn unquote_single_quoted_atom(text: &str) -> Option<String> {
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

/// Reports whether a token kind can carry an identifier-like spelling.
///
/// Inputs:
/// - `kind`: token kind to classify.
///
/// Output:
/// - `true` for lower identifiers, generic identifiers, and upper identifiers.
///
/// Transformation:
/// - Centralizes the parser's permissive identifier token set.
fn is_identifier_like_token(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Atom | TokenKind::Ident | TokenKind::Var)
}

/// Combines ordered list-comprehension filters into one boolean guard.
///
/// Inputs:
/// - `guard`: optional accumulated guard expression from earlier filters.
/// - `filter`: next filter expression in source order.
///
/// Output:
/// - Guard expression equivalent to all filters seen so far.
///
/// Transformation:
/// - Folds comma-separated comprehension filters with `and`, preserving
///   left-to-right source order while reusing the current single-guard parse tree
///   representation.
fn combine_comprehension_filter_guard(guard: Option<Box<Expr>>, filter: Expr) -> Box<Expr> {
    match guard {
        Some(previous) => Box::new(Expr::BinaryOp {
            op: BinaryOp::And,
            left: previous,
            right: Box::new(filter),
        }),
        None => Box::new(filter),
    }
}

/// Parses a lexer integer token into its signed integer value.
///
/// Inputs:
/// - `token`: an integer token emitted by the lexer, including decimal or
///   prefixed `0b`, `0x`, and `0o` forms.
///
/// Output:
/// - Parsed `i64` value, or a parse diagnostic anchored to the token span.
///
/// Transformation:
/// - Selects the radix from the token prefix and delegates to Rust integer
///   parsing, preserving one stable diagnostic message for invalid literals.
fn parse_int_literal_token(token: &Token) -> ParseResult<i64> {
    parse_int_literal_text(&token.text).ok_or_else(|| ParseError {
        message: "invalid integer literal".to_string(),
        span: token.span(),
    })
}

/// Parses the string token payload used by `Atom["name"]`.
///
/// Inputs:
/// - `token`: lexer token for the quoted string inside the atom literal.
///
/// Output:
/// - The unescaped non-empty atom payload.
///
/// Transformation:
/// - Removes the surrounding quotes and recognizes the small string escape set
///   needed by symbolic atom payloads.
fn parse_atom_string_literal_token(token: &Token) -> Option<String> {
    let payload = parse_string_token_payload(&token.text)?;
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}

/// Unquotes a normal Terlan string token.
///
/// Inputs:
/// - `text`: raw lexer token text including double quotes.
///
/// Output:
/// - The unescaped payload when `text` is a valid quoted string token.
///
/// Transformation:
/// - Performs deterministic escape decoding without interpreting source text
///   outside the existing lexer token boundary.
fn parse_string_token_payload(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => output.push(other),
        }
    }
    Some(output)
}

/// Parses integer literal text using Terlan's supported radix prefixes.
///
/// Inputs:
/// - `text`: raw token text for a decimal, binary, hexadecimal, or octal
///   integer literal.
///
/// Output:
/// - Parsed `i64` value when the text is valid for its radix.
///
/// Transformation:
/// - Strips recognized radix prefixes and applies the matching base; plain
///   text remains decimal.
fn parse_int_literal_text(text: &str) -> Option<i64> {
    if let Some(digits) = text.strip_prefix("0b") {
        return i64::from_str_radix(digits, 2).ok();
    }
    if let Some(digits) = text.strip_prefix("0x") {
        return i64::from_str_radix(digits, 16).ok();
    }
    if let Some(digits) = text.strip_prefix("0o") {
        return i64::from_str_radix(digits, 8).ok();
    }
    text.parse::<i64>().ok()
}

/// Reports whether a name starts a raw declaration family.
///
/// Inputs:
/// - `name`: lower-case declaration name candidate.
///
/// Output:
/// - `true` for raw declarations preserved by the parser.
///
/// Transformation:
/// - Keeps non-core declaration families explicit at the parser boundary.
fn is_raw_declaration_name(name: &str) -> bool {
    matches!(name, "target" | "native" | "machine" | "static")
}

/// Attaches parsed documentation tokens to a declaration.
///
/// Inputs:
/// - `decl`: declaration parsed after documentation comments.
/// - `docs`: raw documentation tokens in source order.
///
/// Output:
/// - Declaration with its documentation field populated where supported.
///
/// Transformation:
/// - Mutates only declaration variants that carry docs and leaves imports and
///   exports unchanged.
fn attach_docs(mut decl: Decl, docs: Vec<String>) -> Decl {
    if docs.is_empty() {
        return decl;
    }

    match &mut decl {
        Decl::Type(type_decl) => type_decl.docs = docs,
        Decl::Struct(struct_decl) => struct_decl.docs = docs,
        Decl::Constructor(constructor_decl) => constructor_decl.docs = docs,
        Decl::Function(function_decl) => function_decl.docs = docs,
        Decl::Method(method_decl) => method_decl.docs = docs,
        Decl::Raw(raw_decl) => raw_decl.docs = docs,
        Decl::Trait(trait_decl) => trait_decl.docs = docs,
        Decl::TraitImpl(trait_impl_decl) => trait_impl_decl.docs = docs,
        Decl::AnnotationSchema(annotation_schema_decl) => annotation_schema_decl.docs = docs,
        Decl::Template(template_decl) => template_decl.docs = docs,
        Decl::Import(_) | Decl::Export(_) => {}
    }

    decl
}

/// Checks whether normalized block documentation declares module docs.
///
/// Inputs:
/// - `text`: normalized public documentation block text.
///
/// Output:
/// - `true` when any trimmed line starts with `@module`.
///
/// Transformation:
/// - Treats the TypeDoc-style `@module` tag as the marker that a block belongs
///   to the module declaration rather than to a following item declaration.
fn is_module_doc_block(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim_start().starts_with("@module"))
}

/// Joins raw declaration token parts into readable source text.
///
/// Inputs:
/// - `parts`: token text collected from a raw declaration.
///
/// Output:
/// - Stable raw declaration text.
///
/// Transformation:
/// - Inserts spaces where needed while preserving punctuation adjacency for
///   dots, brackets, commas, and operators.
fn join_parts(parts: &[String]) -> String {
    let mut output = String::new();
    let mut first = true;
    for part in parts {
        if first {
            output.push_str(part);
            first = false;
        } else if part == "." {
            output.push('.');
        } else if output.ends_with('.') {
            output.push_str(part);
        } else if part == "," || part == "|" || part == ":" || part == "->" || part == "|>" {
            output.push(' ');
            output.push_str(part);
        } else if output.ends_with('(')
            || output.ends_with('[')
            || output.ends_with('{')
            || part == "["
            || part == "]"
            || part == ","
        {
            output.push_str(part);
        } else {
            output.push(' ');
            output.push_str(part);
        }
    }
    output
}

#[cfg(test)]
#[path = "parser_decl_test.rs"]
mod parser_decl_test;

#[cfg(test)]
#[path = "parser_expr_test.rs"]
mod parser_expr_test;

#[cfg(test)]
#[path = "parser_html_test.rs"]
mod parser_html_test;

#[cfg(test)]
#[path = "parser_pattern_test.rs"]
mod parser_pattern_test;
