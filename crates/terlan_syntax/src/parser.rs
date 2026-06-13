use crate::{
    ast::{
        Annotation, BinaryOp, BuiltinBlockMacro, CaseClause, ConstructorClause, ConstructorDecl,
        ConstructorParam, Decl, ExportDecl, ExportItem, Expr, FunctionClause, FunctionDecl,
        HtmlAttr, HtmlAttrValue, HtmlBlockExpr, HtmlElement, HtmlNamedSlot, HtmlNode, IfClause,
        ImportDecl, ImportItem, ImportKind, LetBinding, MapExprField, MapField, MethodDecl, Module,
        Param, Pattern, ReceiveAfterClause, StructDecl, StructFieldDecl, TemplateDecl,
        TemplatePropDecl, TraitDecl, TraitImplDecl, TraitMethodDecl, TryAfterClause, TypeDecl,
        TypeExpr, UnaryOp, UnsupportedDecl,
    },
    ebnf::EbnfCompileError,
    lexer::lex,
    span::Span,
    syntax_contract::{
        ensure_canonical_syntax_contract_valid as ensure_syntax_contract_valid, SyntaxContractError,
    },
    token::{Token, TokenKind},
};

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

pub type ParseResult<T> = Result<T, ParseError>;
pub type ParserError = ParseError;

pub fn parse_module(input: &str) -> ParseResult<Module> {
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

pub fn parse_interface_module(input: &str) -> ParseResult<Module> {
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

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_module(&mut self) -> ParseResult<Module> {
        self.skip_comments();
        let docs = self.take_module_docs();
        self.skip_comments();
        let start = self.current().start;
        self.expect_keyword(TokenKind::Module)?;
        let name = self.parse_module_path()?;
        self.expect(TokenKind::Dot)?;

        let mut declarations = Vec::new();
        let mut declaration_annotations = Vec::new();
        let mut pending_pub_function: Option<FunctionDecl> = None;
        let mut pending_pub_function_annotations: Vec<Annotation> = Vec::new();
        while !self.check(TokenKind::EOF) {
            self.skip_comments();
            self.reject_misplaced_module_docs()?;
            let docs = self.take_item_docs();
            self.skip_comments();
            self.reject_misplaced_module_docs()?;
            let annotations = self.parse_leading_annotations()?;
            self.skip_comments();
            if self.check(TokenKind::EOF) {
                break;
            }

            if let Some(pending) = pending_pub_function.as_ref() {
                if !matches!(self.current().kind, TokenKind::Atom | TokenKind::Var)
                    || self.current().text != pending.name
                {
                    declarations.push(Decl::Function(pending_pub_function.take().unwrap()));
                    declaration_annotations
                        .push(std::mem::take(&mut pending_pub_function_annotations));
                }
            }

            match self.current().kind {
                TokenKind::Import => {
                    declarations.push(attach_docs(self.parse_import()?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Pub => {
                    let declaration = attach_docs(self.parse_pub_decl()?, docs);
                    match declaration {
                        Decl::Function(function_decl)
                            if function_decl.is_public && function_decl.clauses.is_empty() =>
                        {
                            pending_pub_function = Some(function_decl);
                            pending_pub_function_annotations = annotations;
                        }
                        _ => {
                            declarations.push(declaration);
                            declaration_annotations.push(annotations);
                        }
                    }
                }
                TokenKind::Export => {
                    return Err(ParseError {
                        message:
                            "source export declarations are not part of canonical Terlan; use `pub` on declarations"
                                .to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::Type => {
                    declarations.push(attach_docs(self.parse_type_decl(false, false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Opaque => {
                    declarations.push(attach_docs(self.parse_type_decl(true, false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Trait => {
                    declarations.push(attach_docs(self.parse_trait_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Impl => {
                    declarations.push(attach_docs(self.parse_trait_impl_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Struct => {
                    declarations.push(attach_docs(self.parse_struct_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Constructor => {
                    declarations.push(attach_docs(self.parse_constructor_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Template => {
                    declarations.push(attach_docs(self.parse_template_decl()?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::LParen => {
                    declarations.push(attach_docs(self.parse_method_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Macro => {
                    self.bump();
                    declarations.push(attach_docs(self.parse_function_decl(false, true)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Atom | TokenKind::Var => {
                    if self.is_template_decl_start() {
                        declarations.push(attach_docs(self.parse_template_decl()?, docs));
                        declaration_annotations.push(annotations);
                    } else if is_raw_declaration_name(&self.current().text) {
                        let decl = self.parse_raw_decl(self.current().text.clone())?;
                        declarations.push(attach_docs(decl, docs));
                        declaration_annotations.push(annotations);
                    } else if let Some(function_decl) = pending_pub_function
                        .take()
                        .filter(|f| f.name == self.current().text)
                    {
                        let clauses = self.parse_function_clause_group(
                            &function_decl.name,
                            function_decl.params.len(),
                        )?;
                        declarations.push(Decl::Function(FunctionDecl {
                            clauses,
                            ..function_decl
                        }));
                        declaration_annotations
                            .push(std::mem::take(&mut pending_pub_function_annotations));
                    } else {
                        declarations
                            .push(attach_docs(self.parse_function_decl(false, false)?, docs));
                        declaration_annotations.push(annotations);
                    }
                }
                TokenKind::EOF => break,
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "unexpected token {:?} in module body",
                            self.current().kind
                        ),
                        span: self.current().span(),
                    })
                }
            }
        }

        if let Some(function_decl) = pending_pub_function {
            declarations.push(Decl::Function(function_decl));
            declaration_annotations.push(pending_pub_function_annotations);
        }

        Ok(Module {
            name,
            docs,
            declarations,
            declaration_annotations,
            span: Span::new(start, self.previous().end),
        })
    }

    fn parse_interface_module(&mut self) -> ParseResult<Module> {
        self.skip_comments();
        let docs = self.take_module_docs();
        self.skip_comments();
        let start = self.current().start;
        self.expect_keyword(TokenKind::Module)?;
        let name = self.parse_module_path()?;
        self.expect(TokenKind::Dot)?;

        let mut declarations = Vec::new();
        let mut declaration_annotations = Vec::new();
        while !self.check(TokenKind::EOF) {
            self.skip_comments();
            self.reject_misplaced_module_docs()?;
            let docs = self.take_item_docs();
            self.skip_comments();
            self.reject_misplaced_module_docs()?;
            let annotations = self.parse_leading_annotations()?;
            self.skip_comments();
            match self.current().kind {
                TokenKind::Import => {
                    declarations.push(attach_docs(self.parse_import()?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Export => {
                    declarations.push(attach_docs(self.parse_interface_export()?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Pub => {
                    declarations.push(attach_docs(self.parse_pub_interface_decl()?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Trait => {
                    declarations.push(attach_docs(self.parse_trait_decl(true)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Type => {
                    declarations.push(attach_docs(self.parse_type_decl(false, false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Opaque => {
                    declarations.push(attach_docs(self.parse_type_decl(true, false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Struct => {
                    declarations.push(attach_docs(self.parse_struct_decl(true)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Constructor => {
                    declarations.push(attach_docs(self.parse_constructor_decl(true)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::LParen => {
                    declarations.push(attach_docs(self.parse_method_signature_decl(false)?, docs));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Macro => {
                    self.bump();
                    declarations.push(attach_docs(
                        self.parse_function_signature_decl(false, true)?,
                        docs,
                    ));
                    declaration_annotations.push(annotations);
                }
                TokenKind::Atom | TokenKind::Var => {
                    if is_raw_declaration_name(&self.current().text) {
                        let decl = self.parse_raw_decl(self.current().text.clone())?;
                        declarations.push(attach_docs(decl, docs));
                        declaration_annotations.push(annotations);
                    } else {
                        declarations.push(attach_docs(
                            self.parse_function_signature_decl(false, false)?,
                            docs,
                        ));
                        declaration_annotations.push(annotations);
                    }
                }
                TokenKind::EOF => break,
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "unexpected token {:?} in interface module body",
                            self.current().kind
                        ),
                        span: self.current().span(),
                    });
                }
            }
        }

        Ok(Module {
            name,
            docs,
            declarations,
            declaration_annotations,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses declaration-leading annotations that precede the next item.
    ///
    /// Inputs:
    /// - Parser cursor positioned before a possible annotation sequence.
    ///
    /// Output:
    /// - Ordered annotation metadata consumed before the declaration.
    ///
    /// Transformation:
    /// - Recognizes the current formal annotation surface and returns metadata
    ///   for syntax output and later semantic phases.
    fn parse_leading_annotations(&mut self) -> ParseResult<Vec<Annotation>> {
        let mut annotations = Vec::new();
        while self.check(TokenKind::At) {
            annotations.push(self.parse_annotation()?);
            self.skip_comments();
        }
        Ok(annotations)
    }

    /// Parses one declaration annotation prefix.
    ///
    /// Inputs:
    /// - Parser cursor at `@`.
    ///
    /// Output:
    /// - Parsed annotation path, optional raw metadata block, and source span.
    ///
    /// Transformation:
    /// - Validates `@lower.path` annotation paths and skips an immediately
    ///   following `{ ... }` metadata block with balanced delimiters. Subject
    ///   values remain a documented follow-up because the token stream does not
    ///   yet carry line boundaries needed to disambiguate them from lower-case
    ///   function declarations.
    fn parse_annotation(&mut self) -> ParseResult<Annotation> {
        let start = self.expect(TokenKind::At)?.start;
        let mut path = vec![self.expect_lower_ident("expected annotation path segment")?];
        while self.consume_if(TokenKind::Dot) {
            path.push(self.expect_lower_ident("expected annotation path segment after '.'")?);
        }

        self.reject_unsupported_annotation_subject()?;

        let args = if self.check(TokenKind::LBrace) {
            Some(self.skip_balanced_annotation_block()?)
        } else {
            None
        };

        Ok(Annotation {
            path,
            args,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Rejects annotation subject forms that the current parser can identify.
    ///
    /// Inputs:
    /// - Parser cursor immediately after an annotation path.
    ///
    /// Output:
    /// - `Ok(())` when the next token can be interpreted as declaration syntax
    ///   or annotation metadata.
    /// - A parse error for unambiguous annotation subject syntax.
    ///
    /// Transformation:
    /// - Keeps declaration-leading annotations valid while rejecting subject
    ///   forms from the canonical EBNF until the lexer exposes line-boundary
    ///   information needed to distinguish lower-case subjects from annotated
    ///   function declarations.
    fn reject_unsupported_annotation_subject(&self) -> ParseResult<()> {
        let token = self.current();
        let next = self.tokens.get(self.pos + 1);
        let is_unambiguous_subject = match token.kind {
            TokenKind::Int | TokenKind::Float | TokenKind::String | TokenKind::Var => true,
            TokenKind::Atom => next
                .map(|next| matches!(next.kind, TokenKind::Dot | TokenKind::LBrace))
                .unwrap_or(false),
            _ => false,
        };

        if is_unambiguous_subject {
            return Err(ParseError {
                message: "annotation subjects are not supported in Terlan 0.0.1; place annotations immediately before the declaration".to_string(),
                span: token.span(),
            });
        }

        Ok(())
    }

    /// Skips an annotation metadata block while preserving parser alignment.
    ///
    /// Inputs:
    /// - Parser cursor at the opening `{` of an annotation metadata block.
    ///
    /// Output:
    /// - Raw block text after consuming the matching `}`.
    ///
    /// Transformation:
    /// - Tracks nested `()`, `[]`, and `{}` delimiters and reports an
    ///   unterminated annotation block before the declaration parser runs.
    fn skip_balanced_annotation_block(&mut self) -> ParseResult<String> {
        let start = self.current().start;
        let mut depth = 0usize;
        let mut parts = Vec::new();
        loop {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated annotation block".to_string(),
                    span: Span::new(start, self.current().end),
                });
            }

            let token = self.bump();
            match token.kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    if depth == 0 {
                        return Err(ParseError {
                            message: "unbalanced annotation block".to_string(),
                            span: token.span(),
                        });
                    }
                    depth -= 1;
                    if depth == 0 {
                        parts.push(token.text);
                        break;
                    }
                }
                _ => {}
            }
            parts.push(token.text);
        }

        Ok(join_parts(&parts))
    }

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

    fn parse_struct_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Struct)?;
        let name = self.expect_type_name()?;
        let mut derives = Vec::new();
        if self.consume_if(TokenKind::Derives) {
            loop {
                derives.push(
                    self.parse_type_expr(&[
                        TokenKind::Comma,
                        TokenKind::Implements,
                        TokenKind::LBrace,
                    ])?
                    .text,
                );
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        let implements = self.parse_implements_clause(&[TokenKind::LBrace])?;
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                self.skip_comments();
                let docs = self.take_item_docs();
                self.skip_comments();
                let field_start = self.current().start;
                let field_name =
                    self.expect_lower_ident("expected lower-case struct field name")?;
                self.expect(TokenKind::Colon)?;
                let annotation = self.parse_type_expr(&[
                    TokenKind::Comma,
                    TokenKind::RBrace,
                    TokenKind::Equals,
                ])?;
                let default = if self.consume_if(TokenKind::Equals) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                fields.push(StructFieldDecl {
                    name: field_name,
                    annotation,
                    default,
                    docs,
                    span: Span::new(field_start, self.previous().end),
                });

                if self.consume_if(TokenKind::Comma) {
                    continue;
                }
                break;
            }

            self.expect(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Struct(StructDecl {
            name,
            derives,
            implements,
            fields,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

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
                let docs = self.take_item_docs();
                self.skip_comments();
                let prop_start = self.current().start;
                let prop_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let annotation = self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBrace])?;
                props.push(TemplatePropDecl {
                    name: prop_name,
                    annotation,
                    docs,
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

    /// Parses a receiver method declaration as a validated preserved form.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the receiver.
    /// - Parser cursor at the receiver opening parenthesis.
    ///
    /// Output:
    /// - A formal `MethodDecl` containing receiver, optional receiver
    ///   mutability, parameters, return type, body clause, visibility, and
    ///   source span.
    ///
    /// Transformation:
    /// - Validates and consumes the receiver-method declaration, then stores it
    ///   as structured AST so syntax output, type checking, and backend
    ///   lowering do not have to recover method data from raw source text.
    fn parse_method_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;

        self.expect(TokenKind::LParen)?;
        let receiver_start = self.current().start;
        let receiver_is_mutable = self.consume_keyword("mut");
        let receiver_name = self.expect_lower_ident("expected lower-case method receiver name")?;
        self.expect(TokenKind::Colon)?;
        let receiver_type = self.parse_receiver_type_expr()?;
        let receiver_end = self.previous().end;
        self.expect(TokenKind::RParen)?;

        let name = self.expect_lower_ident("expected lower-case method name")?;
        self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);
        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_expr(&[TokenKind::Arrow])?;
        self.expect(TokenKind::Arrow)?;
        let body = self.parse_body_expr()?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Method(MethodDecl {
            receiver: Param {
                name: receiver_name,
                annotation: receiver_type,
                is_mutable: receiver_is_mutable,
                span: Span::new(receiver_start, receiver_end),
            },
            name,
            params,
            return_type,
            is_public,
            generic_bounds,
            clauses: vec![FunctionClause {
                patterns: Vec::new(),
                body,
                span: Span::new(start, self.previous().end),
                guard: None,
            }],
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a bodyless receiver method signature for interface summaries.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the receiver.
    /// - Parser cursor at the receiver opening parenthesis in interface mode.
    ///
    /// Output:
    /// - A `MethodDecl` with receiver, optional receiver mutability,
    ///   non-receiver params, return type, visibility, and no body clauses.
    ///
    /// Transformation:
    /// - Consumes the same receiver-method header as source methods, but
    ///   terminates at `.` so `.typi` summaries can preserve receiver metadata
    ///   without inventing receiver-first function signatures.
    fn parse_method_signature_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;

        self.expect(TokenKind::LParen)?;
        let receiver_start = self.current().start;
        let receiver_is_mutable = self.consume_keyword("mut");
        let receiver_name = self.expect_lower_ident("expected lower-case method receiver name")?;
        self.expect(TokenKind::Colon)?;
        let receiver_type = self.parse_receiver_type_expr()?;
        let receiver_end = self.previous().end;
        self.expect(TokenKind::RParen)?;

        let name = self.expect_lower_ident("expected lower-case method name")?;
        self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);
        self.expect(TokenKind::Colon)?;
        if self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }
        let return_type = self.parse_type_expr(&[TokenKind::Dot])?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Method(MethodDecl {
            receiver: Param {
                name: receiver_name,
                annotation: receiver_type,
                is_mutable: receiver_is_mutable,
                span: Span::new(receiver_start, receiver_end),
            },
            name,
            params,
            return_type,
            is_public,
            generic_bounds,
            clauses: Vec::new(),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses the type expression used by a receiver declaration.
    ///
    /// Inputs:
    /// - Parser cursor positioned after `(receiver:`.
    ///
    /// Output:
    /// - A `TypeExpr` whose text preserves the receiver type constructor and
    ///   optional type arguments.
    ///
    /// Transformation:
    /// - Requires the receiver type head to be an upper-case Terlan type name,
    ///   then consumes optional bracketed type arguments before the receiver
    ///   closing parenthesis.
    fn parse_receiver_type_expr(&mut self) -> ParseResult<TypeExpr> {
        let start = self.current().start;
        let name = self.expect_type_name()?;
        let args = self.parse_optional_type_arg_text()?;
        Ok(TypeExpr {
            text: format!("{name}{args}"),
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses optional type arguments while preserving their source text.
    ///
    /// Inputs:
    /// - Parser cursor at `[` or the next token after a type constructor.
    ///
    /// Output:
    /// - Bracketed type-argument text such as `[T, U]`, or an empty string when
    ///   no type-argument list is present.
    ///
    /// Transformation:
    /// - Consumes a balanced bracketed type-expression list and joins each
    ///   argument through the parser's canonical type-expression formatter.
    fn parse_optional_type_arg_text(&mut self) -> ParseResult<String> {
        if !self.consume_if(TokenKind::LBracket) {
            return Ok(String::new());
        }

        let mut args = Vec::new();
        if !self.check(TokenKind::RBracket) {
            loop {
                args.push(
                    self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                        .text,
                );
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBracket)?;

        Ok(format!("[{}]", args.join(", ")))
    }

    fn parse_trait_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Trait)?;
        let name = self.expect_type_name()?;
        let params = self.parse_optional_type_params()?;
        let mut super_traits = Vec::new();
        if self.consume_if(TokenKind::Extends) {
            loop {
                super_traits.push(
                    self.parse_type_expr(&[TokenKind::Comma, TokenKind::LBrace])?
                        .text,
                );
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while !self.check(TokenKind::RBrace) {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated trait declaration".to_string(),
                    span: Span::new(start, self.current().end),
                });
            }

            self.skip_comments();
            if self.consume_if(TokenKind::Semicolon) {
                continue;
            }

            let docs = self.take_item_docs();
            self.skip_comments();
            if self.check(TokenKind::RBrace) {
                break;
            }

            let method = self.parse_trait_method(docs)?;
            methods.push(method);
            self.consume_if(TokenKind::Semicolon);
        }

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Trait(TraitDecl {
            name,
            params,
            super_traits,
            methods,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses an explicit trait conformance block.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `impl`.
    /// - Parser cursor positioned at the `impl` keyword.
    ///
    /// Output:
    /// - A structured `TraitImplDecl` preserving the implemented trait, target
    ///   type, method bodies, visibility, and span.
    ///
    /// Transformation:
    /// - Consumes `impl TraitRef for TypeExpr { FunctionDecl* }.` and stores
    ///   method bodies as ordinary function declarations for later semantic
    ///   conformance checking and CoreIR lowering.
    fn parse_trait_impl_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        self.parse_trait_impl_decl_with_body_mode(is_public, true)
    }

    /// Parses an interface-form trait conformance block.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `impl`.
    /// - Parser cursor positioned at the `impl` keyword.
    ///
    /// Output:
    /// - A structured `TraitImplDecl` with signature-only method declarations.
    ///
    /// Transformation:
    /// - Consumes the same conformance header as source `impl`, but parses
    ///   method entries as signatures so `.tli` files can summarize
    ///   conformances without bodies.
    fn parse_trait_impl_interface_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        self.parse_trait_impl_decl_with_body_mode(is_public, false)
    }

    /// Parses a trait implementation block in source or interface mode.
    ///
    /// Inputs:
    /// - `is_public`: declaration-site visibility.
    /// - `with_bodies`: when `true`, method declarations require bodies;
    ///   otherwise signatures are accepted.
    ///
    /// Output:
    /// - A `TraitImplDecl` containing header type references and implementation
    ///   method declarations.
    ///
    /// Transformation:
    /// - Shares the conformance header parser while switching the body parser
    ///   between source function declarations and interface signatures.
    fn parse_trait_impl_decl_with_body_mode(
        &mut self,
        is_public: bool,
        with_bodies: bool,
    ) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Impl)?;
        let trait_ref = self.parse_type_expr(&[TokenKind::For])?;
        self.expect_keyword(TokenKind::For)?;
        let for_type = self.parse_type_expr(&[TokenKind::LBrace])?;
        self.expect(TokenKind::LBrace)?;

        let mut methods = Vec::new();
        while !self.check(TokenKind::RBrace) {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated impl declaration".to_string(),
                    span: Span::new(start, self.current().end),
                });
            }

            self.skip_comments();
            if self.consume_if(TokenKind::Semicolon) {
                continue;
            }
            if self.check(TokenKind::RBrace) {
                break;
            }

            let method_decl = if with_bodies {
                self.parse_function_decl(false, false)?
            } else {
                self.parse_function_signature_decl(false, false)?
            };
            let Decl::Function(function_decl) = method_decl else {
                return Err(ParseError {
                    message: "expected impl method declaration".to_string(),
                    span: self.current().span(),
                });
            };
            methods.push(function_decl);
            self.consume_if(TokenKind::Semicolon);
        }

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::TraitImpl(TraitImplDecl {
            trait_ref,
            for_type,
            methods,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_trait_method(&mut self, docs: Vec<String>) -> ParseResult<TraitMethodDecl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case trait method name")?;
        self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);
        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_expr(&[TokenKind::Arrow, TokenKind::Dot])?;
        let default_body = if self.consume_if(TokenKind::Arrow) {
            Some(self.parse_body_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Dot)?;

        Ok(TraitMethodDecl {
            name,
            params,
            return_type,
            generic_bounds,
            default_body,
            docs,
            is_public: false,
            span: Span::new(start, self.previous().end),
        })
    }

    fn parse_constructor_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Constructor)?;
        let name = self.expect_type_name()?;
        let params = self.parse_optional_type_params()?;
        self.expect(TokenKind::LBrace)?;

        let mut clauses = Vec::new();
        if !self.check(TokenKind::RBrace) {
            loop {
                clauses.push(self.parse_constructor_clause()?);
                if self.consume_if(TokenKind::Semicolon) {
                    if self.check(TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        validate_constructor_clause_shapes(&clauses)?;

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Constructor(ConstructorDecl {
            name,
            params,
            clauses,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_constructor_clause(&mut self) -> ParseResult<ConstructorClause> {
        let start = self.current().start;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                let param = self.parse_constructor_param()?;
                let param_is_varargs = param.is_varargs;
                params.push(param);

                if param_is_varargs && !self.check(TokenKind::RParen) {
                    return Err(ParseError {
                        message: "constructor varargs parameter must be last".to_string(),
                        span: self.current().span(),
                    });
                }

                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;

        let has_varargs = params.iter().any(|param| param.is_varargs);
        let has_defaults = params.iter().any(|param| param.default.is_some());
        if has_varargs && has_defaults {
            return Err(ParseError {
                message: "constructor clauses cannot combine defaults and varargs yet".to_string(),
                span: Span::new(start, self.previous().end),
            });
        }

        let mut seen_default = false;
        for param in &params {
            if param.default.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(ParseError {
                    message: "constructor default parameters must be trailing".to_string(),
                    span: param.span,
                });
            }
        }

        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_expr(&[TokenKind::Arrow])?;
        self.expect(TokenKind::Arrow)?;
        let body = self.parse_body_expr_with_clause_sep(None, true)?;

        Ok(ConstructorClause {
            params,
            return_type,
            body,
            span: Span::new(start, self.previous().end),
        })
    }

    fn parse_constructor_param(&mut self) -> ParseResult<ConstructorParam> {
        let start = self.current().start;
        let is_varargs = self.consume_if(TokenKind::Ellipsis);
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let annotation =
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen, TokenKind::Equals])?;
        let default = if self.consume_if(TokenKind::Equals) {
            if is_varargs {
                return Err(ParseError {
                    message: "constructor varargs parameters cannot have defaults".to_string(),
                    span: Span::new(start, self.previous().end),
                });
            }
            Some(self.parse_single_expr()?)
        } else {
            None
        };

        Ok(ConstructorParam {
            name,
            annotation,
            default,
            is_varargs,
            span: Span::new(start, self.previous().end),
        })
    }

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

    /// Parses an import declaration after the current token has been identified
    /// as `import`.
    ///
    /// Inputs: the parser cursor must point at the `import` keyword.
    /// Outputs: a module import declaration, asset import declaration, or a
    /// syntax diagnostic.
    /// Transformation: consumes the import token stream and normalizes the
    /// grammar's single `ImportItem` shape into one `ImportDecl` with module
    /// path, imported symbols, type-import flag, or asset source metadata.
    fn parse_import(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Import)?;

        let asset_import = if self.consume_keyword("file") {
            Some((ImportKind::File, "file"))
        } else if self.consume_keyword("css") {
            Some((ImportKind::Css, "css"))
        } else if self.consume_keyword("markdown") {
            Some((ImportKind::Markdown, "markdown"))
        } else {
            None
        };

        if let Some((kind, keyword)) = asset_import {
            let raw_path = self.expect(TokenKind::String)?.text.clone();
            let path = raw_path
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(&raw_path)
                .to_string();
            if !self.consume_keyword("as") {
                return Err(ParseError {
                    message: format!("expected `as` in {keyword} import"),
                    span: self.current().span(),
                });
            }
            let alias = self.expect_ident()?;
            let alias_span = Span::new(self.previous().start, self.previous().end);
            self.expect(TokenKind::Dot)?;
            return Ok(Decl::Import(ImportDecl {
                kind,
                module_name: String::new(),
                items: vec![ImportItem {
                    name: alias,
                    as_alias: None,
                    span: alias_span,
                }],
                is_type: false,
                source_path: Some(path),
                span: Span::new(start, self.previous().end),
            }));
        }

        let mut is_type = false;
        if self.consume_if(TokenKind::Type) || self.consume_keyword("type") {
            is_type = true;
        }

        let first_segment = self.expect_ident()?;
        let first_segment_span = self.previous().span();
        let mut path_segments = vec![(first_segment, first_segment_span)];
        let mut brace_import = false;
        while self.check(TokenKind::Dot) {
            if matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if token.kind == TokenKind::LBrace
            ) {
                self.bump();
                brace_import = true;
                break;
            }
            if matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if matches!(token.kind, TokenKind::Atom | TokenKind::Var)
            ) {
                self.bump();
                let segment = self.expect_ident()?;
                let segment_span = self.previous().span();
                path_segments.push((segment, segment_span));
                continue;
            }
            break;
        }

        let mut items = Vec::new();
        let module_name;
        if brace_import && self.consume_if(TokenKind::LBrace) {
            validate_module_path_segments(&path_segments)?;
            module_name = path_segments
                .iter()
                .map(|(segment, _)| segment.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if self.check(TokenKind::RBrace) {
                return Err(ParseError {
                    message: "expected at least one import symbol".to_string(),
                    span: self.current().span(),
                });
            } else {
                loop {
                    let name = self.expect_ident()?;
                    let as_alias = if self.consume_keyword("as") {
                        let alias = self.expect_ident()?;
                        validate_import_alias_class(&name, &alias, self.previous().span())?;
                        Some(alias)
                    } else {
                        None
                    };
                    items.push(ImportItem {
                        name,
                        as_alias,
                        span: Span::new(self.previous().start, self.previous().end),
                    });
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    self.expect(TokenKind::RBrace)?;
                    break;
                }
            }
        } else {
            let (name, _) = path_segments.pop().ok_or_else(|| ParseError {
                message: "expected import item".to_string(),
                span: self.current().span(),
            })?;
            validate_module_path_segments(&path_segments)?;
            module_name = path_segments
                .iter()
                .map(|(segment, _)| segment.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if module_name.is_empty() {
                return Err(ParseError {
                    message: "expected import module".to_string(),
                    span: self.current().span(),
                });
            }
            let as_alias = if self.consume_keyword("as") {
                let alias = self.expect_ident()?;
                validate_import_alias_class(&name, &alias, self.previous().span())?;
                Some(alias)
            } else {
                None
            };
            items.push(ImportItem {
                name,
                as_alias,
                span: Span::new(self.previous().start, self.previous().end),
            });
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Import(ImportDecl {
            kind: ImportKind::Module,
            module_name,
            items,
            is_type,
            source_path: None,
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_type_decl(&mut self, is_opaque: bool, is_public: bool) -> ParseResult<Decl> {
        self.parse_type_decl_with_body_requirement(is_opaque, is_public, !is_opaque)
    }

    /// Parses a type declaration in interface mode.
    ///
    /// Inputs:
    /// - `is_opaque`: whether the declaration starts with `opaque type`.
    /// - `is_public`: whether `pub` was consumed before the declaration.
    ///
    /// Output:
    /// - A `TypeDecl` whose `variants` may be empty for type-header
    ///   summaries such as `pub type ExternalUser.`.
    ///
    /// Transformation:
    /// - Reuses source type parsing while allowing bodyless public interface
    ///   headers so generated `.typi` files can summarize exported nominal
    ///   types without inventing structural bodies.
    fn parse_type_interface_decl(&mut self, is_opaque: bool, is_public: bool) -> ParseResult<Decl> {
        self.parse_type_decl_with_body_requirement(is_opaque, is_public, false)
    }

    /// Parses a type declaration with caller-selected body strictness.
    ///
    /// Inputs:
    /// - `is_opaque`: whether the declaration starts with `opaque type`.
    /// - `is_public`: declaration-site visibility.
    /// - `body_required`: whether missing `=` is an error.
    ///
    /// Output:
    /// - A structured `TypeDecl`.
    ///
    /// Transformation:
    /// - Consumes the type header, optional implements clause, optional union
    ///   body, and terminating `.`, while keeping source-mode and
    ///   interface-mode body requirements explicit at the call site.
    fn parse_type_decl_with_body_requirement(
        &mut self,
        is_opaque: bool,
        is_public: bool,
        body_required: bool,
    ) -> ParseResult<Decl> {
        let start = self.current().start;
        if is_opaque {
            self.expect_keyword(TokenKind::Opaque)?;
            self.expect_keyword(TokenKind::Type)?;
        } else {
            self.expect_keyword(TokenKind::Type)?;
        }
        let name = self.expect_type_name()?;

        let params = self.parse_optional_type_params()?;
        let implements = self.parse_implements_clause(&[TokenKind::Equals, TokenKind::Dot])?;

        let mut variants = Vec::new();
        if self.consume_if(TokenKind::Equals) {
            loop {
                variants.push(self.parse_type_expr(&[TokenKind::Pipe, TokenKind::Dot])?);
                if self.consume_if(TokenKind::Pipe) {
                    continue;
                }
                break;
            }
        } else if body_required {
            return Err(ParseError {
                message: "expected `=` in type declaration".to_string(),
                span: self.current().span(),
            });
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Type(TypeDecl {
            name,
            params,
            variants,
            implements,
            is_public,
            is_opaque,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses an optional declaration-site trait conformance list.
    ///
    /// Inputs:
    /// - Parser cursor positioned after the type head or struct derivation
    ///   list.
    /// - `stop`: tokens that end the surrounding declaration header.
    ///
    /// Output:
    /// - A list of trait references named by `implements`, or an empty list
    ///   when no conformance clause is present.
    ///
    /// Transformation:
    /// - Consumes `implements TraitRef { "," TraitRef }` and preserves each
    ///   trait reference as a `TypeExpr` for later semantic conformance
    ///   resolution.
    fn parse_implements_clause(&mut self, stop: &[TokenKind]) -> ParseResult<Vec<TypeExpr>> {
        let mut implements = Vec::new();
        if !self.consume_if(TokenKind::Implements) {
            return Ok(implements);
        }

        loop {
            let mut trait_stop = Vec::with_capacity(stop.len() + 1);
            trait_stop.push(TokenKind::Comma);
            trait_stop.extend_from_slice(stop);
            implements.push(self.parse_type_expr(&trait_stop)?);
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }

        Ok(implements)
    }

    fn parse_type_param_text(&mut self) -> ParseResult<String> {
        let ty = self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?;
        Ok(ty.text)
    }

    fn parse_optional_type_params(&mut self) -> ParseResult<Vec<String>> {
        let mut params = Vec::new();
        if self.consume_if(TokenKind::LBracket) {
            if !self.check(TokenKind::RBracket) {
                loop {
                    params.push(self.parse_type_param_text()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RBracket)?;
        }
        Ok(params)
    }

    fn parse_function_signature_decl(
        &mut self,
        is_public: bool,
        is_macro: bool,
    ) -> ParseResult<Decl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case function name")?;
        self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);

        self.expect(TokenKind::Colon)?;
        if self.check(TokenKind::Arrow) || self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }

        let return_type = self.parse_type_expr(&[TokenKind::Dot])?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Function(FunctionDecl {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            generic_bounds,
            clauses: Vec::new(),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

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

    fn parse_function_decl(&mut self, is_public: bool, is_macro: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case function name")?;
        self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;

        self.expect(TokenKind::LParen)?;
        if !self.check(TokenKind::RParen) && !self.is_typed_param_start() {
            return self.parse_untyped_function_decl_after_name(
                start,
                name,
                is_public,
                is_macro,
                generic_bounds,
            );
        }
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);

        self.expect(TokenKind::Colon)?;
        if self.check(TokenKind::Arrow) || self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }

        let return_type = self.parse_type_expr(&[TokenKind::Arrow, TokenKind::Dot])?;

        let mut clauses = Vec::new();

        let consumed_arrow = self.consume_if(TokenKind::Arrow);
        if consumed_arrow {
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            self.expect(TokenKind::Dot)?;
            clauses.push(FunctionClause {
                patterns: params
                    .iter()
                    .map(|param| Pattern::Var(param.name.clone()))
                    .collect(),
                body,
                guard: None,
                span: Span::new(start, self.previous().end),
            });

            return Ok(Decl::Function(FunctionDecl {
                name,
                params,
                return_type,
                is_public,
                is_macro,
                generic_bounds: generic_bounds.clone(),
                clauses,
                docs: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }

        self.expect(TokenKind::Dot)?;

        loop {
            let clause_name = self.expect_lower_ident("expected lower-case function name")?;
            if clause_name != name {
                return Err(ParseError {
                    message: "expected function clause for declared function name".to_string(),
                    span: self.previous().span(),
                });
            }
            self.expect(TokenKind::LParen)?;

            let mut clause_patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    clause_patterns.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            clauses.push(FunctionClause {
                patterns: clause_patterns,
                body,
                guard: None,
                span: Span::new(start, self.previous().end),
            });

            if self.consume_if(TokenKind::Semicolon) {
                if self.check(TokenKind::EOF) || self.check(TokenKind::Dot) {
                    break;
                }
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        Ok(Decl::Function(FunctionDecl {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            generic_bounds,
            clauses,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_untyped_function_decl_after_name(
        &mut self,
        start: usize,
        name: String,
        is_public: bool,
        is_macro: bool,
        generic_bounds: Vec<String>,
    ) -> ParseResult<Decl> {
        let mut clauses = Vec::new();
        let mut arity = None;

        loop {
            let clause_start = if clauses.is_empty() {
                start
            } else {
                self.current().start
            };
            let mut patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    patterns.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;

            if let Some(expected) = arity {
                if patterns.len() != expected {
                    return Err(ParseError {
                        message: format!(
                            "clause for {name} has arity {}, expected {expected}",
                            patterns.len()
                        ),
                        span: self.current().span(),
                    });
                }
            } else {
                arity = Some(patterns.len());
            }

            if self.consume_if(TokenKind::Colon) {
                self.parse_type_expr(&[TokenKind::Arrow])?;
            }

            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            clauses.push(FunctionClause {
                patterns,
                body,
                guard,
                span: Span::new(clause_start, self.previous().end),
            });

            if self.consume_if(TokenKind::Semicolon) {
                let clause_name = self.expect_lower_ident("expected lower-case function name")?;
                if clause_name != name {
                    return Err(ParseError {
                        message: "expected function clause for declared function name".to_string(),
                        span: self.previous().span(),
                    });
                }
                self.expect(TokenKind::LParen)?;
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        let arity = arity.unwrap_or(0);
        Ok(Decl::Function(FunctionDecl {
            name,
            params: (0..arity)
                .map(|index| Param {
                    name: format!("_Arg{}", index + 1),
                    annotation: TypeExpr {
                        text: "Dynamic".to_string(),
                        span: Span::new(start, start),
                    },
                    is_mutable: false,
                    span: Span::new(start, start),
                })
                .collect(),
            return_type: TypeExpr {
                text: "Dynamic".to_string(),
                span: Span::new(start, start),
            },
            is_public,
            is_macro,
            generic_bounds,
            clauses,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_function_clause_group(
        &mut self,
        name: &str,
        arity: usize,
    ) -> ParseResult<Vec<FunctionClause>> {
        let mut clauses = Vec::new();

        loop {
            let start = self.current().start;
            let clause_name = self.expect_lower_ident("expected lower-case function name")?;
            if clause_name != name {
                return Err(ParseError {
                    message: "expected function clause for declared function name".to_string(),
                    span: self.previous().span(),
                });
            }

            self.expect(TokenKind::LParen)?;
            let mut patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    patterns.push(self.parse_pattern_with_type_annotation()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;

            if patterns.len() != arity {
                return Err(ParseError {
                    message: format!(
                        "clause for {name} has arity {}, expected {arity}",
                        patterns.len()
                    ),
                    span: self.current().span(),
                });
            }

            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name), false)?;
            clauses.push(FunctionClause {
                patterns,
                body,
                span: Span::new(start, self.previous().end),
                guard,
            });

            if self.consume_if(TokenKind::Semicolon) {
                if self.check(TokenKind::EOF) {
                    break;
                }
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        Ok(clauses)
    }

    fn parse_pattern_with_type_annotation(&mut self) -> ParseResult<Pattern> {
        let pattern = self.parse_pattern()?;
        if self.consume_if(TokenKind::Colon) {
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen])?;
        }
        Ok(pattern)
    }

    fn parse_param(&mut self) -> ParseResult<Param> {
        let start = self.current().start;
        if self.consume_if(TokenKind::Ellipsis) {
            return Err(ParseError {
                message: "function varargs parameters are not supported in Terlan 0.0.1"
                    .to_string(),
                span: Span::new(start, self.previous().end),
            });
        }
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let annotation = if self.check(TokenKind::RParen) {
            TypeExpr {
                text: "Dynamic".to_string(),
                span: Span::new(start, start),
            }
        } else {
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen, TokenKind::Equals])?
        };
        if self.consume_if(TokenKind::Equals) {
            return Err(ParseError {
                message: "function default parameters are not supported in Terlan 0.0.1"
                    .to_string(),
                span: Span::new(start, self.previous().end),
            });
        }

        Ok(Param {
            name,
            annotation,
            is_mutable: false,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses a canonical Terlan module path.
    ///
    /// Inputs: the parser cursor must point at the first module path segment.
    /// Outputs: the dotted module path string or a syntax diagnostic.
    /// Transformation: consumes a package-rooted module path. The first segment
    /// must be lower-case, while later segments may be lower-case package
    /// segments or upper-case public module namespace segments.
    fn parse_module_path(&mut self) -> ParseResult<String> {
        let mut segments = vec![self.expect_package_root_segment()?];
        while self.check(TokenKind::Dot) {
            let dot = self.current().clone();
            let Some(next) = self.tokens.get(self.pos + 1) else {
                break;
            };
            if dot.end != next.start {
                break;
            }
            match next.kind {
                TokenKind::Atom | TokenKind::Var => {
                    self.bump();
                    segments.push(self.expect_module_path_segment()?);
                }
                _ => break,
            }
        }
        Ok(segments.join("."))
    }

    /// Parses the lower-case package root segment of a module path.
    ///
    /// Inputs: the parser cursor must point at the next expected segment.
    /// Outputs: the segment text or a syntax diagnostic at the offending token.
    /// Transformation: consumes only lexer `Atom` tokens for the package root.
    fn expect_package_root_segment(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Var => Err(ParseError {
                message: "expected lower-case package root segment".to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected module path segment".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a non-root module path segment.
    ///
    /// Inputs: the parser cursor must point at a dotted module path segment.
    /// Outputs: the segment text or a syntax diagnostic.
    /// Transformation: consumes lower-case package segments or upper-case
    /// public module namespace segments.
    fn expect_module_path_segment(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            _ => Err(ParseError {
                message: "expected module path segment".to_string(),
                span: token.span(),
            }),
        }
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

    fn is_typed_param_start(&self) -> bool {
        if self.check(TokenKind::Ellipsis) {
            return true;
        }
        if !matches!(self.current().kind, TokenKind::Atom | TokenKind::Var) {
            return false;
        }
        matches!(
            self.tokens.get(self.pos + 1),
            Some(token) if token.kind == TokenKind::Colon
        )
    }

    fn consume_generic_params_if_present(&mut self) -> ParseResult<()> {
        if !self.consume_if(TokenKind::LBracket) {
            return Ok(());
        }

        let start = self.previous().start;
        let mut depth = 1usize;
        while !self.check(TokenKind::EOF) {
            if self.consume_if(TokenKind::LBracket) {
                depth += 1;
                continue;
            }
            if self.consume_if(TokenKind::RBracket) {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
                continue;
            }
            self.bump();
        }

        Err(ParseError {
            message: "unterminated generic parameter list".to_string(),
            span: Span::new(start, self.current().end),
        })
    }

    fn consume_angle_generic_params_if_present(&mut self) -> ParseResult<Vec<String>> {
        if !self.consume_if(TokenKind::Lt) {
            return Ok(Vec::new());
        }

        let start = self.previous().start;
        let mut depth = 1usize;
        let mut depth_p = 0usize;
        let mut depth_b = 0usize;
        let mut depth_br = 0usize;
        let mut current = Vec::new();
        let mut bounds = Vec::new();

        let flush = |current: &mut Vec<String>, bounds: &mut Vec<String>| {
            let bound = join_parts(current);
            if !bound.trim().is_empty() {
                bounds.push(bound);
            }
            current.clear();
        };

        while !self.check(TokenKind::EOF) {
            if self.consume_if(TokenKind::Lt) {
                depth += 1;
                current.push("<".to_string());
                continue;
            }
            if self.consume_if(TokenKind::Gt) {
                depth -= 1;
                if depth == 0 {
                    flush(&mut current, &mut bounds);
                    return Ok(bounds);
                }
                current.push(">".to_string());
                continue;
            }

            if self.check(TokenKind::Comma)
                && depth == 1
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0
            {
                flush(&mut current, &mut bounds);
                self.bump();
                continue;
            }

            let token = self.bump();
            if matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::DocBlockComment
            ) {
                continue;
            }
            if matches!(
                token.kind,
                TokenKind::Case
                    | TokenKind::Of
                    | TokenKind::End
                    | TokenKind::Receive
                    | TokenKind::Fun
                    | TokenKind::When
                    | TokenKind::And
                    | TokenKind::Or
                    | TokenKind::PipeForward
                    | TokenKind::EqEq
                    | TokenKind::EqEqEq
                    | TokenKind::NotEq
                    | TokenKind::NotEqEq
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::DivRem
                    | TokenKind::Rem
                    | TokenKind::Bang
            ) {
                return Err(ParseError {
                    message: format!(
                        "runtime expression token '{}' is not valid in type position",
                        token.text
                    ),
                    span: token.span(),
                });
            }

            match token.kind {
                TokenKind::LParen => depth_p += 1,
                TokenKind::RParen => depth_p = depth_p.saturating_sub(1),
                TokenKind::LBracket => depth_b += 1,
                TokenKind::RBracket => depth_b = depth_b.saturating_sub(1),
                TokenKind::LBrace => depth_br += 1,
                TokenKind::RBrace => depth_br = depth_br.saturating_sub(1),
                _ => {}
            }
            current.push(token.text);
        }

        Err(ParseError {
            message: "unterminated generic parameter list".to_string(),
            span: Span::new(start, self.current().end),
        })
    }

    /// Parses a canonical post-parameter trait constraint list.
    ///
    /// Inputs:
    /// - Parser cursor immediately after a callable parameter list.
    ///
    /// Output:
    /// - Constraint type-expression texts such as `Eq[A]` or `Show[A]`, or an
    ///   empty list when no constraint list is present.
    ///
    /// Transformation:
    /// - Consumes `[Constraint, ...]` only in callable constraint position and
    ///   preserves each constraint as type-expression text for later semantic
    ///   conversion into typechecker `FunctionBound` values.
    fn consume_constraint_list_if_present(&mut self) -> ParseResult<Vec<String>> {
        if !self.consume_if(TokenKind::LBracket) {
            return Ok(Vec::new());
        }

        let mut constraints = Vec::new();
        if self.consume_if(TokenKind::RBracket) {
            return Ok(constraints);
        }

        loop {
            constraints.push(
                self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                    .text,
            );
            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            self.expect(TokenKind::RBracket)?;
            break;
        }

        Ok(constraints)
    }

    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                if token.text == "_" {
                    Ok(Pattern::Wildcard)
                } else {
                    if self.check(TokenKind::LParen) {
                        return Err(ParseError {
                            message: "lowercase bindings cannot be used as constructor patterns"
                                .to_string(),
                            span: token.span(),
                        });
                    }
                    Ok(Pattern::Var(token.text))
                }
            }
            TokenKind::Colon => {
                self.bump();
                let atom = self.expect_atom_literal_name()?;
                Ok(Pattern::Atom(atom))
            }
            TokenKind::Var => {
                self.bump();
                if self.check(TokenKind::LParen)
                    && token
                        .text
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    self.bump();
                    if self.check(TokenKind::RParen) {
                        return Err(ParseError {
                            message: "constructor patterns require at least one argument"
                                .to_string(),
                            span: token.span(),
                        });
                    }
                    let mut parts = vec![Pattern::Atom(token.text)];
                    loop {
                        parts.push(self.parse_pattern()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Pattern::Tuple(parts))
                } else if token
                    .text
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    Ok(Pattern::Tuple(vec![Pattern::Atom(token.text)]))
                } else {
                    Ok(Pattern::Var(token.text))
                }
            }
            TokenKind::Int => {
                self.bump();
                Ok(Pattern::Int(parse_int_literal_token(&token)?))
            }
            TokenKind::Float => {
                self.bump();
                Ok(Pattern::Float(token.text.parse::<f64>().unwrap_or(0.0)))
            }
            TokenKind::LBracket => {
                self.bump();
                if self.check(TokenKind::RBracket) {
                    self.bump();
                    return Ok(Pattern::List(Vec::new()));
                }

                let first = self.parse_pattern()?;
                if self.consume_if(TokenKind::Pipe) {
                    let tail = self.parse_pattern()?;
                    self.expect(TokenKind::RBracket)?;
                    return Ok(Pattern::ListCons(Box::new(first), Box::new(tail)));
                }

                let mut items = vec![first];
                while self.consume_if(TokenKind::Comma) {
                    if self.check(TokenKind::RBracket) {
                        break;
                    }
                    items.push(self.parse_pattern()?);
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Pattern::List(items))
            }
            TokenKind::Hash => {
                self.bump();
                if self.consume_if(TokenKind::LBrace) {
                    if self.consume_if(TokenKind::RBrace) {
                        return Ok(Pattern::Map(Vec::new()));
                    }

                    let mut fields = Vec::new();
                    loop {
                        fields.push(self.parse_pattern_map_field()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }

                    self.expect(TokenKind::RBrace)?;
                    return Ok(Pattern::Map(fields));
                }

                let name = self.expect_ident()?;
                self.expect(TokenKind::LBrace)?;
                let mut fields = Vec::new();
                if !self.consume_if(TokenKind::RBrace) {
                    loop {
                        fields.push(self.parse_record_pattern_field()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }

                    self.expect(TokenKind::RBrace)?;
                }

                Ok(Pattern::Record { name, fields })
            }
            TokenKind::LBrace => {
                self.bump();
                if self.check(TokenKind::RBrace) {
                    self.bump();
                    return Ok(Pattern::Tuple(Vec::new()));
                }
                let mut items = Vec::new();
                loop {
                    items.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                Ok(Pattern::Tuple(items))
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_pattern()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment => {
                self.bump();
                self.parse_pattern()
            }
            TokenKind::DocBlockComment => {
                self.bump();
                self.parse_pattern()
            }
            _ => Err(ParseError {
                message: format!("unexpected token {:?} in pattern", token.kind),
                span: token.span(),
            }),
        }
    }

    fn parse_type_expr(&mut self, stop: &[TokenKind]) -> ParseResult<TypeExpr> {
        let start = self.current().start;
        let mut depth_p = 0;
        let mut depth_b = 0;
        let mut depth_bra = 0;
        let mut parts = Vec::new();

        while !self.check(TokenKind::EOF) {
            if self.check_any(stop)
                && depth_p == 0
                && depth_b == 0
                && depth_bra == 0
                && !self.is_qualified_type_dot(&parts)
            {
                break;
            }
            let token = self.bump();
            if matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::DocBlockComment
            ) {
                continue;
            }

            match token.kind {
                TokenKind::LParen => depth_p += 1,
                TokenKind::RParen if depth_p > 0 => depth_p -= 1,
                TokenKind::LBracket => depth_b += 1,
                TokenKind::RBracket if depth_b > 0 => depth_b -= 1,
                TokenKind::LBrace => depth_bra += 1,
                TokenKind::RBrace if depth_bra > 0 => depth_bra -= 1,
                _ => {}
            }
            parts.push(token.text);
        }

        if parts.is_empty() {
            return Err(ParseError {
                message: "expected type".to_string(),
                span: Span::new(start, self.current().end),
            });
        }

        let text = join_parts(&parts);
        if let Some(token) = invalid_runtime_type_token(&text) {
            return Err(ParseError {
                message: format!(
                    "runtime expression token '{token}' is not valid in type position"
                ),
                span: Span::new(start, self.previous().end),
            });
        }

        Ok(TypeExpr {
            text,
            span: Span::new(start, self.previous().end),
        })
    }

    fn parse_expr(&mut self) -> ParseResult<Expr> {
        let first = self.parse_single_expr()?;
        if !self.check(TokenKind::Semicolon) {
            return Ok(first);
        }

        let mut expressions = vec![first];
        while self.consume_if(TokenKind::Semicolon) {
            expressions.push(self.parse_single_expr()?);
        }
        Ok(Expr::Sequence(expressions))
    }

    fn parse_single_expr(&mut self) -> ParseResult<Expr> {
        if self.check(TokenKind::Let) {
            return self.parse_let_expr();
        }
        self.parse_binary_expr(0)
    }

    /// Parses a semicolon-scoped local binding expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `let` keyword.
    ///
    /// Output:
    /// - `Expr::Let` with one or more ordered bindings and a required final
    ///   body expression.
    ///
    /// Transformation:
    /// - Consumes `let Binding = Expr` pairs separated by semicolons. A
    ///   semicolon followed by another `Binding =` starts the next binding;
    ///   otherwise the semicolon starts the required final body expression.
    fn parse_let_expr(&mut self) -> ParseResult<Expr> {
        self.expect_keyword(TokenKind::Let)?;
        let mut bindings = vec![self.parse_let_binding()?];
        let mut body = None;

        while self.consume_if(TokenKind::Semicolon) {
            if self.is_let_binding_start() {
                bindings.push(self.parse_let_binding()?);
            } else {
                body = Some(Box::new(self.parse_expr()?));
                break;
            }
        }

        if body.is_none() {
            return Err(ParseError {
                message: "let expression requires an explicit result expression".to_string(),
                span: self.current().span(),
            });
        }

        Ok(Expr::Let { bindings, body })
    }

    /// Parses one local binding inside a `let` expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a canonical `Binding` name.
    ///
    /// Output:
    /// - A `LetBinding` containing the binding name and value expression.
    ///
    /// Transformation:
    /// - Accepts only lower-case binding names or ignored lower-case binding
    ///   names and rejects wildcard-only or uppercase constructor syntax.
    fn parse_let_binding(&mut self) -> ParseResult<LetBinding> {
        let name = self.expect_binding_name()?;
        self.expect(TokenKind::Equals)?;
        let value = self.parse_single_expr()?;
        Ok(LetBinding { name, value })
    }

    /// Reports whether the current cursor starts another `let` binding.
    ///
    /// Inputs:
    /// - Parser cursor after a semicolon inside a `let` expression.
    ///
    /// Output:
    /// - `true` when the next token pair is `Binding =`.
    ///
    /// Transformation:
    /// - Performs a non-consuming two-token lookahead so the parser can
    ///   distinguish another binding from the final body expression.
    fn is_let_binding_start(&self) -> bool {
        let Some(next) = self.tokens.get(self.pos + 1) else {
            return false;
        };
        self.is_binding_token(self.current()) && next.kind == TokenKind::Equals
    }

    fn parse_body_expr(&mut self) -> ParseResult<Expr> {
        self.parse_body_expr_with_clause_sep(None, false)
    }

    fn parse_body_expr_with_clause_sep(
        &mut self,
        clause_name: Option<&str>,
        is_constructor_clause: bool,
    ) -> ParseResult<Expr> {
        let mut expr = self.parse_single_expr()?;
        while self.consume_if(TokenKind::Equals) {
            expr = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
        }

        while self.consume_if(TokenKind::Comma) {
            let rest = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
            if !is_send_expr(&expr) {
                expr = rest;
            }
        }

        let mut expressions = Vec::new();
        while self.check(TokenKind::Semicolon) {
            if self.is_clause_separator_ahead(clause_name, is_constructor_clause) {
                break;
            }

            self.bump();
            expressions.push(self.parse_single_expr()?);
        }

        if !expressions.is_empty() {
            let mut values = vec![expr];
            values.append(&mut expressions);
            expr = Expr::Sequence(values);
        }
        Ok(expr)
    }

    fn is_clause_separator_ahead(
        &self,
        clause_name: Option<&str>,
        is_constructor_clause: bool,
    ) -> bool {
        if !matches!(self.current().kind, TokenKind::Semicolon) {
            return false;
        }

        let next = self.tokens.get(self.pos + 1);
        let next_next = self.tokens.get(self.pos + 2);

        if is_constructor_clause {
            return matches!(next, Some(token) if token.kind == TokenKind::LParen);
        }

        let Some(clause_name) = clause_name else {
            return false;
        };

        matches!(next, Some(token) if token.kind == TokenKind::Atom && token.text == clause_name)
            && matches!(next_next, Some(token) if token.kind == TokenKind::LParen)
    }

    fn parse_pattern_map_field(&mut self) -> ParseResult<MapField> {
        let key_token = self.current().clone();
        if key_token.kind != TokenKind::Atom && key_token.kind != TokenKind::Var {
            return Err(ParseError {
                message: "expected map field key atom".to_string(),
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) {
            false
        } else {
            return Err(ParseError {
                message: "expected := or => in map pattern".to_string(),
                span: self.current().span(),
            });
        };

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
    }

    fn parse_record_pattern_field(&mut self) -> ParseResult<MapField> {
        let key_token = self.current().clone();
        if key_token.kind != TokenKind::Atom && key_token.kind != TokenKind::Var {
            return Err(ParseError {
                message: "expected record field key atom".to_string(),
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) {
            false
        } else {
            return Err(ParseError {
                message: "expected = in record pattern".to_string(),
                span: self.current().span(),
            });
        };

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
    }

    fn parse_binary_expr(&mut self, min_prec: u8) -> ParseResult<Expr> {
        let mut left = self.parse_unary_expr()?;

        loop {
            if self.check_keyword("as") {
                let prec = 8;
                if prec < min_prec {
                    break;
                }
                self.consume_keyword("as");
                let target_type = self.parse_cast_target_type()?;
                left = Expr::Cast {
                    expr: Box::new(left),
                    target_type,
                };
                continue;
            }

            let (op, prec) = match self.current().kind {
                TokenKind::Plus => (Some(BinaryOp::Add), 6),
                TokenKind::Minus => (Some(BinaryOp::Sub), 6),
                TokenKind::Star => (Some(BinaryOp::Mul), 7),
                TokenKind::Slash => (Some(BinaryOp::Div), 7),
                TokenKind::EqEq => (Some(BinaryOp::EqEq), 5),
                TokenKind::EqEqEq => {
                    return Err(ParseError {
                        message: "deprecated equality operator '=:=', use '=='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::NotEq if self.current().text == "!=" => (Some(BinaryOp::NotEq), 5),
                TokenKind::NotEq => {
                    return Err(ParseError {
                        message: "deprecated inequality operator '/=', use '!='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::NotEqEq => {
                    return Err(ParseError {
                        message: "deprecated inequality operator '=/=', use '!='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::Lt => (Some(BinaryOp::Lt), 5),
                TokenKind::Gt => (Some(BinaryOp::Gt), 5),
                TokenKind::LtEq => (Some(BinaryOp::LtEq), 5),
                TokenKind::GtEq => (Some(BinaryOp::GtEq), 5),
                TokenKind::DivRem => (Some(BinaryOp::DivRem), 7),
                TokenKind::Rem => (Some(BinaryOp::Rem), 7),
                TokenKind::And => (Some(BinaryOp::And), 4),
                TokenKind::Or => (Some(BinaryOp::Or), 3),
                TokenKind::PipeForward => (Some(BinaryOp::PipeForward), 2),
                TokenKind::Bang => (Some(BinaryOp::Send), 1),
                _ => (None, 0),
            };

            let (op, prec) = match op {
                Some(op) => (op, prec),
                None => break,
            };

            if prec < min_prec {
                break;
            }

            self.bump();
            let right = self.parse_binary_expr(prec + 1)?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parses the target type for an explicit cast expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned immediately after contextual keyword `as`.
    ///
    /// Output:
    /// - A preserved `TypeExpr` naming the requested conversion target.
    ///
    /// Transformation:
    /// - Consumes type syntax until the next expression boundary. This keeps
    ///   `value as Type` below unary/postfix syntax and above arithmetic,
    ///   comparison, boolean, pipe, and send operators.
    fn parse_cast_target_type(&mut self) -> ParseResult<TypeExpr> {
        self.parse_type_expr(&[
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::EqEq,
            TokenKind::EqEqEq,
            TokenKind::NotEq,
            TokenKind::NotEqEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::DivRem,
            TokenKind::Rem,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::PipeForward,
            TokenKind::Bang,
            TokenKind::Comma,
            TokenKind::Semicolon,
            TokenKind::RParen,
            TokenKind::RBracket,
            TokenKind::RBrace,
            TokenKind::End,
            TokenKind::After,
            TokenKind::Catch,
            TokenKind::When,
            TokenKind::Arrow,
            TokenKind::Dot,
            TokenKind::EOF,
        ])
    }

    /// Parses one expression operand from the token stream.
    ///
    /// Input: the parser cursor at the start of an expression operand, including
    /// literals, names, keyword expressions, macro forms, and prefix operators.
    ///
    /// Output: an expression tree whose suffixes are added by the caller after
    /// the primary operand is recognized.
    ///
    /// Transformation: source tokens are classified according to the formal
    /// expression grammar; pattern-only wildcard syntax is rejected here so `_`
    /// cannot leak into value position.
    fn parse_unary_expr(&mut self) -> ParseResult<Expr> {
        let token = self.current().clone();
        let token_kind = token.kind.clone();
        let expr = match token_kind {
            TokenKind::Int => {
                self.bump();
                Expr::Int(parse_int_literal_token(&token)?)
            }
            TokenKind::Float => {
                self.bump();
                Expr::Float(token.text.parse::<f64>().unwrap_or(0.0))
            }
            TokenKind::Minus => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Bang => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Bang,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Atom if token.text == "not" => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Atom if token.text == "quote" => {
                self.bump();
                let inner = self.parse_single_expr()?;
                Expr::Quote(Box::new(inner))
            }
            TokenKind::Atom if token.text == "unquote" => {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Expr::Unquote(Box::new(inner))
            }
            TokenKind::Colon => {
                self.bump();
                Expr::Atom(self.expect_atom_literal_name()?)
            }
            TokenKind::Question => {
                self.bump();
                let name = self.expect_ident()?;
                let args = if self.consume_if(TokenKind::LParen) {
                    let args = self.parse_expr_list(TokenKind::RParen)?;
                    self.expect(TokenKind::RParen)?;
                    args
                } else {
                    Vec::new()
                };
                Expr::MacroCall { name, args }
            }
            TokenKind::Atom
                if BuiltinBlockMacro::from_name(&token.text).is_some()
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(token) if token.kind == TokenKind::LBrace
                    ) =>
            {
                let macro_kind =
                    BuiltinBlockMacro::from_name(&token.text).expect("known block macro");
                self.bump();
                let raw = self.parse_raw_block()?;
                self.parse_builtin_block_macro(macro_kind, raw)?
            }
            TokenKind::Atom
                if token.text != "_"
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(next) if next.kind == TokenKind::LBrace && token.end == next.start
                    ) =>
            {
                self.bump();
                let raw = self.parse_raw_block()?;
                Expr::RawMacro {
                    name: token.text.clone(),
                    raw,
                }
            }
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                let base_expr = if token_kind == TokenKind::Atom {
                    if token.text == "_" {
                        return Err(ParseError {
                            message: "wildcard '_' is only valid in pattern position".to_string(),
                            span: token.span(),
                        });
                    } else {
                        Expr::Var(token.text.clone())
                    }
                } else {
                    Expr::Var(token.text.clone())
                };

                if self.consume_if(TokenKind::Colon) {
                    let fun =
                        self.expect_lower_ident("expected lower-case remote function name")?;
                    self.expect(TokenKind::LParen)?;
                    let args = self.parse_expr_list(TokenKind::RParen)?;
                    self.expect(TokenKind::RParen)?;
                    Expr::Call {
                        callee: Box::new(Expr::Atom(fun)),
                        args,
                        remote: Some(token.text.clone()),
                        is_fun_value: false,
                    }
                } else {
                    let mut dotted = vec![token.text.clone()];
                    let mut lookahead = self.pos;
                    if token_kind == TokenKind::Var
                        && matches!(
                            self.tokens.get(lookahead),
                            Some(token) if token.kind == TokenKind::LBracket
                        )
                    {
                        let type_args = self.parse_required_trait_call_type_arg_text()?;
                        self.expect(TokenKind::Dot)?;
                        let fun =
                            self.expect_lower_ident("expected lower-case trait method name")?;
                        self.expect(TokenKind::LParen)?;
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            args,
                            remote: Some(format!("{}{type_args}", token.text)),
                            is_fun_value: false,
                        });
                    }

                    while matches!(self.tokens.get(lookahead), Some(token) if token.kind == TokenKind::Dot)
                        && matches!(
                            (self.tokens.get(lookahead), self.tokens.get(lookahead + 1)),
                            (Some(dot), Some(token))
                                if dot.end == token.start
                                    && matches!(token.kind, TokenKind::Atom | TokenKind::Var)
                        )
                    {
                        dotted.push(self.tokens[lookahead + 1].text.clone());
                        lookahead += 2;
                    }

                    if dotted.len() > 1
                        && matches!(
                            self.tokens.get(lookahead),
                            Some(token) if token.kind == TokenKind::LParen
                        )
                    {
                        if token_kind == TokenKind::Atom
                            && dotted.len() == 2
                            && matches!(
                                self.tokens.get(self.pos),
                                Some(dot) if dot.kind == TokenKind::Dot
                            )
                            && matches!(
                                self.tokens.get(self.pos + 1),
                                Some(name) if name.kind == TokenKind::Atom
                            )
                        {
                            self.expect(TokenKind::Dot)?;
                            let field =
                                self.expect_lower_ident("expected lower-case method name")?;
                            self.expect(TokenKind::LParen)?;
                            let args = self.parse_expr_list(TokenKind::RParen)?;
                            self.expect(TokenKind::RParen)?;
                            return Ok(Expr::Call {
                                callee: Box::new(Expr::FieldAccess {
                                    value: Box::new(base_expr),
                                    field,
                                }),
                                args,
                                remote: None,
                                is_fun_value: false,
                            });
                        }

                        if token_kind != TokenKind::Atom {
                            if dotted.len() == 2 {
                                self.expect(TokenKind::Dot)?;
                                let fun = self.expect_lower_ident(
                                    "expected lower-case remote function name",
                                )?;
                                self.expect(TokenKind::LParen)?;
                                let args = self.parse_expr_list(TokenKind::RParen)?;
                                self.expect(TokenKind::RParen)?;
                                return Ok(Expr::Call {
                                    callee: Box::new(Expr::Atom(fun)),
                                    args,
                                    remote: Some(token.text.clone()),
                                    is_fun_value: false,
                                });
                            }

                            return Err(ParseError {
                                message: "expected lower-case package root segment".to_string(),
                                span: token.span(),
                            });
                        }

                        let mut remote_parts = vec![token.text.clone()];
                        let mut fun = String::new();
                        for index in 1..dotted.len() {
                            self.expect(TokenKind::Dot)?;
                            if index == dotted.len() - 1 {
                                fun = self.expect_lower_ident(
                                    "expected lower-case remote function name",
                                )?;
                            } else {
                                remote_parts.push(self.expect_module_path_segment()?);
                            }
                        }
                        self.expect(TokenKind::LParen)?;
                        let remote = remote_parts.join(".");
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            args,
                            remote: Some(remote),
                            is_fun_value: false,
                        }
                    } else if self.consume_if(TokenKind::LParen) {
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(base_expr),
                            args,
                            remote: None,
                            is_fun_value: false,
                        }
                    } else {
                        base_expr
                    }
                }
            }
            TokenKind::String => {
                self.bump();
                Expr::Binary(token.text)
            }
            TokenKind::Binary => {
                self.bump();
                Expr::Binary(token.text)
            }
            TokenKind::LParen => self.parse_paren_or_lambda_expr()?,
            TokenKind::LBracket => {
                self.bump();
                if self.check(TokenKind::RBracket) {
                    self.bump();
                    Expr::List(Vec::new())
                } else {
                    let first = self.parse_expr()?;
                    if self.consume_if(TokenKind::Pipe) {
                        let checkpoint = self.pos;
                        let generator = self.parse_list_generator();
                        match generator {
                            Ok((pattern, source)) => {
                                let mut guard = None;
                                while self.consume_if(TokenKind::Comma) {
                                    let qualifier_checkpoint = self.pos;
                                    if self.parse_list_generator().is_ok() {
                                        return Err(ParseError {
                                            message: "multiple list comprehension generators are not supported in the formal parser path".to_string(),
                                            span: self.current().span(),
                                        });
                                    }
                                    self.pos = qualifier_checkpoint;
                                    let filter = self.parse_expr()?;
                                    guard = Some(combine_comprehension_filter_guard(guard, filter));
                                }
                                self.expect(TokenKind::RBracket)?;
                                Expr::ListComprehension {
                                    expr: Box::new(first),
                                    pattern,
                                    source: Box::new(source),
                                    guard,
                                }
                            }
                            Err(_) => {
                                self.pos = checkpoint;
                                let tail = self.parse_expr()?;
                                self.expect(TokenKind::RBracket)?;
                                Expr::ListCons(Box::new(first), Box::new(tail))
                            }
                        }
                    } else {
                        let mut items = vec![first];
                        while self.consume_if(TokenKind::Comma) {
                            items.push(self.parse_expr()?);
                        }
                        self.expect(TokenKind::RBracket)?;
                        Expr::List(items)
                    }
                }
            }
            TokenKind::LBrace => {
                self.bump();
                if self.check(TokenKind::RBrace) {
                    self.bump();
                    Expr::Tuple(Vec::new())
                } else {
                    let mut items = Vec::new();
                    loop {
                        items.push(self.parse_expr()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace)?;
                    Expr::Tuple(items)
                }
            }
            TokenKind::Hash => {
                self.bump();
                if self.consume_if(TokenKind::LBracket) {
                    if self.check(TokenKind::RBracket) {
                        self.bump();
                        Expr::FixedArray(Vec::new())
                    } else {
                        let mut elements = Vec::new();
                        loop {
                            elements.push(self.parse_expr()?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBracket)?;
                        Expr::FixedArray(elements)
                    }
                } else if self.consume_if(TokenKind::LBrace) {
                    if self.consume_if(TokenKind::RBrace) {
                        Expr::Map(Vec::new())
                    } else {
                        let mut fields = Vec::new();
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::Map)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                        Expr::Map(fields)
                    }
                } else {
                    let name = self.expect_ident()?;
                    self.expect(TokenKind::LBrace)?;
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::ErlangRecord)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                    }
                    Expr::RecordConstruct { name, fields }
                }
            }
            TokenKind::Case => {
                self.bump();
                let scrutinee = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let clauses = self.parse_keyword_expr_clauses(&[TokenKind::RBrace])?;
                self.expect(TokenKind::RBrace)?;
                Expr::Case {
                    scrutinee: Box::new(scrutinee),
                    clauses,
                }
            }
            TokenKind::Receive => {
                self.bump();
                self.expect(TokenKind::LBrace)?;
                let clauses =
                    self.parse_keyword_expr_clauses(&[TokenKind::After, TokenKind::RBrace])?;
                let mut after_clause = None;
                if self.consume_if(TokenKind::After) {
                    let trigger = self.parse_expr()?;
                    self.expect(TokenKind::Arrow)?;
                    let body = self.parse_expr()?;
                    after_clause = Some(ReceiveAfterClause {
                        trigger: Box::new(trigger),
                        body: Box::new(body),
                    });
                }
                self.expect(TokenKind::RBrace)?;
                Expr::Receive {
                    clauses,
                    after_clause,
                }
            }
            TokenKind::Try => {
                self.bump();
                let body = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let of_clauses = self.parse_keyword_expr_clauses(&[
                    TokenKind::Catch,
                    TokenKind::After,
                    TokenKind::RBrace,
                ])?;
                let mut catch_clauses = Vec::new();
                if self.consume_if(TokenKind::Catch) {
                    catch_clauses =
                        self.parse_keyword_expr_clauses(&[TokenKind::After, TokenKind::RBrace])?;
                }
                let after_clause = if self.consume_if(TokenKind::After) {
                    let trigger = self.parse_expr()?;
                    self.expect(TokenKind::Arrow)?;
                    let body = self.parse_expr()?;
                    Some(TryAfterClause {
                        trigger: Box::new(trigger),
                        body: Box::new(body),
                    })
                } else {
                    None
                };
                self.expect(TokenKind::RBrace)?;
                Expr::Try {
                    body: Box::new(body),
                    of_clauses,
                    catch_clauses,
                    after_clause,
                }
            }
            TokenKind::If => {
                self.bump();
                self.expect(TokenKind::LBrace)?;
                let mut clauses = Vec::new();
                loop {
                    let condition = self.parse_single_expr()?;
                    self.expect(TokenKind::Arrow)?;
                    let body = self.parse_single_expr()?;
                    clauses.push(IfClause { condition, body });
                    if self.consume_if(TokenKind::Semicolon) {
                        if self.check(TokenKind::RBrace) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                self.expect(TokenKind::RBrace)?;
                Expr::If { clauses }
            }
            TokenKind::Comment
            | TokenKind::DocComment
            | TokenKind::ModuleDocComment
            | TokenKind::DocBlockComment => {
                self.bump();
                self.parse_unary_expr()?
            }
            other => {
                return Err(ParseError {
                    message: format!("unexpected token {:?} in expression", other),
                    span: token.span(),
                })
            }
        };

        self.parse_expr_suffix(expr)
    }

    /// Parses explicit type arguments in a trait-targeted method call.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `[` in `Trait[Type].method(...)`.
    ///
    /// Output:
    /// - Bracketed type-argument text such as `[Int]` or `[List[String], T]`.
    ///
    /// Transformation:
    /// - Reuses the formal type-expression parser for each comma-separated
    ///   argument and preserves canonical spacing so syntax output and backend
    ///   trait lookup use the same normalized qualifier text.
    fn parse_required_trait_call_type_arg_text(&mut self) -> ParseResult<String> {
        if !self.consume_if(TokenKind::LBracket) {
            return Err(ParseError {
                message: "expected trait call type arguments".to_string(),
                span: self.current().span(),
            });
        }

        let mut args = Vec::new();
        if self.check(TokenKind::RBracket) {
            return Err(ParseError {
                message: "trait call type arguments cannot be empty".to_string(),
                span: self.current().span(),
            });
        }

        loop {
            args.push(
                self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                    .text,
            );
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBracket)?;

        Ok(format!("[{}]", args.join(", ")))
    }

    /// Parses either a parenthesized expression or the canonical lambda syntax.
    ///
    /// Inputs:
    /// - Parser cursor positioned at `(`.
    ///
    /// Output:
    /// - `Expr::Fun` when the parenthesized head is followed by `->`.
    /// - The enclosed expression when the source is ordinary grouping.
    ///
    /// Transformation:
    /// - Speculatively parses lambda parameter patterns and rewinds when the
    ///   tokens do not form `(patterns) -> Expr`. This keeps anonymous
    ///   functions expression-shaped without retaining the removed `fun ... end`
    ///   keyword form.
    fn parse_paren_or_lambda_expr(&mut self) -> ParseResult<Expr> {
        let start = self.current().start;
        let checkpoint = self.pos;
        self.expect(TokenKind::LParen)?;
        let mut patterns = Vec::new();
        let lambda_head = if self.check(TokenKind::RParen) {
            self.bump();
            true
        } else {
            loop {
                match self.parse_pattern_with_type_annotation() {
                    Ok(pattern) => patterns.push(pattern),
                    Err(_) => {
                        self.pos = checkpoint;
                        self.expect(TokenKind::LParen)?;
                        let inner = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        return Ok(inner);
                    }
                }
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
            self.consume_if(TokenKind::RParen)
        };

        if lambda_head && self.consume_if(TokenKind::Arrow) {
            let body = self.parse_expr()?;
            return Ok(Expr::Fun {
                clauses: vec![FunctionClause {
                    patterns,
                    body,
                    span: Span::new(start, self.previous().end),
                    guard: None,
                }],
            });
        }

        self.pos = checkpoint;
        self.expect(TokenKind::LParen)?;
        let inner = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        Ok(inner)
    }

    /// Parses semicolon-separated clauses for keyword expressions.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the first clause pattern.
    /// - `stops`: token kinds that close the surrounding keyword expression.
    ///
    /// Output:
    /// - Ordered `CaseClause` values containing pattern, optional guard, and
    ///   body expression payloads.
    ///
    /// Transformation:
    /// - Parses each clause body as one expression so the following semicolon
    ///   remains a clause separator rather than becoming an expression-sequence
    ///   operator. Nested `let` expressions still own their semicolon-scoped
    ///   binding/body syntax through `parse_single_expr`.
    fn parse_keyword_expr_clauses(&mut self, stops: &[TokenKind]) -> ParseResult<Vec<CaseClause>> {
        let mut clauses = Vec::new();
        if stops.iter().any(|stop| self.check(stop.clone())) {
            return Ok(clauses);
        }
        loop {
            let pattern = self.parse_pattern()?;
            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };
            self.expect(TokenKind::Arrow)?;
            let body = self.parse_single_expr()?;
            clauses.push(CaseClause {
                pattern,
                guard,
                body,
            });
            if self.consume_if(TokenKind::Semicolon) {
                if stops.iter().any(|stop| self.check(stop.clone())) {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(clauses)
    }

    fn parse_builtin_block_macro(
        &mut self,
        macro_kind: BuiltinBlockMacro,
        raw: String,
    ) -> ParseResult<Expr> {
        match macro_kind {
            BuiltinBlockMacro::Html => {
                let nodes = parse_html_nodes(&raw);
                Ok(Expr::HtmlBlock(HtmlBlockExpr {
                    macro_kind,
                    raw,
                    nodes,
                }))
            }
        }
    }

    /// Parses postfix expression suffixes after a primary expression.
    ///
    /// Inputs:
    /// - `expr`: already parsed expression that can receive postfix suffixes.
    ///
    /// Output:
    /// - Expression with all immediately following postfix suffixes applied.
    ///
    /// Transformation:
    /// - Repeatedly consumes record access/update, constructor extension,
    ///   template instantiation, method call, field access, function-value
    ///   invocation, and index suffixes, folding each suffix into the current
    ///   expression tree.
    fn parse_expr_suffix(&mut self, mut expr: Expr) -> ParseResult<Expr> {
        loop {
            if self.consume_if(TokenKind::Hash) {
                let name = self.expect_ident()?;
                if self.consume_if(TokenKind::LBrace) {
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::ErlangRecord)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                    }

                    expr = Expr::RecordUpdate {
                        value: Box::new(expr),
                        name,
                        fields,
                    };
                    continue;
                }

                self.expect(TokenKind::Dot)?;
                let field = self.expect_ident()?;
                expr = Expr::RecordAccess {
                    value: Box::new(expr),
                    name,
                    field,
                };
                continue;
            }

            if self.consume_if(TokenKind::With) {
                if !matches!(expr, Expr::Call { .. }) {
                    return Err(ParseError {
                        message: "constructor chain requires call expression before with"
                            .to_string(),
                        span: self.current().span(),
                    });
                }
                let record = self.parse_record_expr()?;
                expr = Expr::ConstructorChain {
                    base: Box::new(expr),
                    record: Box::new(record),
                };
                continue;
            }

            if self.check(TokenKind::LBrace)
                && matches!(expr, Expr::Var(_))
                && matches!(
                    self.tokens.get(self.pos.saturating_sub(1)),
                    Some(previous) if previous.end == self.current().start
                )
            {
                self.bump();
                let Expr::Var(name) = expr else {
                    unreachable!("template instantiation receiver was checked above");
                };
                let mut fields = Vec::new();
                if !self.consume_if(TokenKind::RBrace) {
                    loop {
                        fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                expr = Expr::TemplateInstantiate { name, fields };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                        (self.tokens.get(self.pos), self.tokens.get(self.pos + 1)),
                        (Some(dot), Some(token))
                            if dot.end == token.start && token.kind == TokenKind::LParen
                )
            {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let args = self.parse_expr_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    remote: None,
                    is_fun_value: true,
                };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                    (
                        self.tokens.get(self.pos),
                        self.tokens.get(self.pos + 1),
                        self.tokens.get(self.pos + 2)
                    ),
                    (Some(dot), Some(name), Some(open))
                        if dot.end == name.start
                            && name.end == open.start
                            && matches!(name.kind, TokenKind::Atom | TokenKind::Var)
                            && open.kind == TokenKind::LParen
                )
            {
                self.bump();
                let field = self.expect_lower_ident("expected lower-case method name")?;
                self.expect(TokenKind::LParen)?;
                let args = self.parse_expr_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(Expr::FieldAccess {
                        value: Box::new(expr),
                        field,
                    }),
                    args,
                    remote: None,
                    is_fun_value: false,
                };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                        (self.tokens.get(self.pos), self.tokens.get(self.pos + 1)),
                        (Some(dot), Some(token))
                            if dot.end == token.start
                                && matches!(token.kind, TokenKind::Atom | TokenKind::Var)
                )
            {
                self.bump();
                let field = self.expect_lower_ident("expected lower-case field name")?;
                expr = Expr::FieldAccess {
                    value: Box::new(expr),
                    field,
                };
                continue;
            }

            if self.consume_if(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index(Box::new(expr), Box::new(index));
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn parse_record_expr(&mut self) -> ParseResult<Expr> {
        let name = self.expect(TokenKind::Var)?.text;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;
        }
        Ok(Expr::RecordConstruct { name, fields })
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

#[derive(Debug)]
struct HtmlBlockParser {
    chars: Vec<char>,
    pos: usize,
}

impl HtmlBlockParser {
    fn parse(raw: &str) -> ParseResult<Vec<HtmlNode>> {
        let mut parser = Self {
            chars: raw.chars().collect(),
            pos: 0,
        };
        let nodes = parser.parse_nodes(None, false)?;
        parser.skip_ws();
        if parser.eof() {
            Ok(nodes)
        } else {
            Err(ParseError {
                message: "invalid html source".to_string(),
                span: Span::new(0, raw.len()),
            })
        }
    }

    fn parse_nodes(
        &mut self,
        stop_tag: Option<&str>,
        stop_slot_block: bool,
    ) -> ParseResult<Vec<HtmlNode>> {
        let mut nodes = Vec::new();
        while !self.eof() {
            self.skip_ws();
            if self.eof() {
                break;
            }

            if stop_slot_block && self.consume_if("}") {
                return Ok(nodes);
            }

            if self.consume_if("</") {
                let name = self.parse_identifier()?;
                self.skip_ws();
                self.expect_char('>')?;

                if let Some(expected) = stop_tag {
                    if name == expected {
                        return Ok(nodes);
                    }
                }
                return Err(ParseError {
                    message: format!("unexpected closing tag </{}>", name),
                    span: Span::new(self.pos, self.pos),
                });
            }

            if self.check_named_slot_start() {
                nodes.push(self.parse_named_slot()?);
                continue;
            }

            if self.consume_if("<") {
                nodes.push(self.parse_html_element()?);
                continue;
            }

            nodes.extend(self.parse_text_nodes(stop_slot_block)?);
        }

        if let Some(name) = stop_tag {
            Err(ParseError {
                message: format!("missing closing tag </{}>", name),
                span: Span::new(self.pos, self.pos),
            })
        } else if stop_slot_block {
            Err(ParseError {
                message: "missing closing brace for named slot".to_string(),
                span: Span::new(self.pos, self.pos),
            })
        } else {
            Ok(nodes)
        }
    }

    fn parse_html_element(&mut self) -> ParseResult<HtmlNode> {
        let name = self.parse_identifier()?;
        let attrs = self.parse_attrs()?;

        if self.consume_if("/") {
            self.expect_char('>')?;
            return Ok(HtmlNode::Element(HtmlElement {
                name,
                attrs,
                children: Vec::new(),
            }));
        }

        self.expect_char('>')?;
        let children = self.parse_nodes(Some(&name), false)?;
        Ok(HtmlNode::Element(HtmlElement {
            name,
            attrs,
            children,
        }))
    }

    fn parse_named_slot(&mut self) -> ParseResult<HtmlNode> {
        self.expect_char('@')?;
        let name = self.parse_identifier()?;
        self.skip_ws();
        self.expect_char('{')?;
        let children = self.parse_nodes(None, true)?;
        Ok(HtmlNode::NamedSlot(HtmlNamedSlot { name, children }))
    }

    fn parse_attrs(&mut self) -> ParseResult<Vec<HtmlAttr>> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.eof() || self.check_char('>') || self.check_char('/') {
                break;
            }

            let name = self.parse_identifier()?;
            let value = if self.consume_if("=") {
                self.skip_ws();
                Some(self.parse_attribute_value()?)
            } else {
                None
            };
            attrs.push(HtmlAttr { name, value });
        }
        Ok(attrs)
    }

    fn parse_attribute_value(&mut self) -> ParseResult<HtmlAttrValue> {
        if self.consume_if("\"") {
            Ok(HtmlAttrValue::Text(self.consume_until('"')))
        } else if self.consume_if("'") {
            Ok(HtmlAttrValue::Text(self.consume_until('\'')))
        } else if self.consume_if("{") {
            let expr_text = self.parse_braced_expression()?;
            Ok(HtmlAttrValue::Expr(parse_terlan_expr(&expr_text)?))
        } else {
            let start = self.pos;
            while !self.eof() && !self.current().is_whitespace() && !self.check_char('>') {
                self.pos += 1;
            }

            if self.pos == start {
                Err(ParseError {
                    message: "expected attribute value".to_string(),
                    span: Span::new(self.pos, self.pos),
                })
            } else {
                Ok(HtmlAttrValue::Text(self.slice(start, self.pos)))
            }
        }
    }

    fn parse_text_nodes(&mut self, stop_slot_block: bool) -> ParseResult<Vec<HtmlNode>> {
        let mut nodes = Vec::new();
        let mut text = String::new();

        while !self.eof()
            && !self.check_char('<')
            && !self.check_named_slot_start()
            && !(stop_slot_block && self.check_char('}'))
        {
            if self.consume_if("{") {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    nodes.push(HtmlNode::Text(trimmed));
                }
                text.clear();

                let expr = self.parse_braced_expression()?;
                nodes.push(HtmlNode::Expr(parse_terlan_expr(&expr)?));
                continue;
            }

            text.push(self.current());
            self.pos += 1;
        }

        let text = text.trim().to_string();
        if !text.is_empty() {
            nodes.push(HtmlNode::Text(text));
        }

        Ok(nodes)
    }

    fn parse_braced_expression(&mut self) -> ParseResult<String> {
        let start = self.pos;
        let mut depth = 1usize;
        let mut quote = None;
        while !self.eof() {
            let ch = self.current();
            if let Some(current_quote) = quote {
                if ch == '\\' && current_quote == '"' && self.pos + 1 < self.chars.len() {
                    self.pos += 2;
                    continue;
                }

                self.pos += 1;
                if ch == current_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                self.pos += 1;
                continue;
            }

            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    let expression = self.slice(start, self.pos).trim().to_string();
                    self.pos += 1;
                    return Ok(expression);
                }
            }

            self.pos += 1;
        }

        Err(ParseError {
            message: "unterminated interpolated expression".to_string(),
            span: Span::new(start, start),
        })
    }

    fn parse_identifier(&mut self) -> ParseResult<String> {
        let start = self.pos;
        while !self.eof() {
            let c = self.current();
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            Err(ParseError {
                message: "expected html identifier".to_string(),
                span: Span::new(self.pos, self.pos),
            })
        } else {
            Ok(self.slice(start, self.pos))
        }
    }

    fn check_named_slot_start(&self) -> bool {
        if self.eof() || self.current() != '@' {
            return false;
        }

        let mut pos = self.pos + 1;
        if pos >= self.chars.len()
            || !(self.chars[pos].is_ascii_alphabetic() || self.chars[pos] == '_')
        {
            return false;
        }

        while pos < self.chars.len() {
            let ch = self.chars[pos];
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == ':' {
                pos += 1;
            } else {
                break;
            }
        }

        while pos < self.chars.len() && self.chars[pos].is_whitespace() {
            pos += 1;
        }

        pos < self.chars.len() && self.chars[pos] == '{'
    }

    fn expect_char(&mut self, ch: char) -> ParseResult<()> {
        if self.check_char(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError {
                message: format!("expected '{}'", ch),
                span: Span::new(self.pos, self.pos),
            })
        }
    }

    fn consume_until(&mut self, end: char) -> String {
        let start = self.pos;
        while !self.eof() && self.current() != end {
            self.pos += 1;
        }
        let value = self.slice(start, self.pos);
        let _ = self.consume_if_char(end);
        value
    }

    fn consume_if_char(&mut self, expected: char) -> bool {
        if self.check_char(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while !self.eof() && self.current().is_whitespace() {
            self.pos += 1;
        }
    }

    fn check_char(&self, expected: char) -> bool {
        !self.eof() && self.current() == expected
    }

    fn check_str(&self, expected: &str) -> bool {
        let chars: Vec<char> = expected.chars().collect();
        if self.pos + chars.len() > self.chars.len() {
            return false;
        }
        (0..chars.len()).all(|i| self.chars[self.pos + i] == chars[i])
    }

    fn consume_if(&mut self, expected: &str) -> bool {
        if self.check_str(expected) {
            self.pos += expected.chars().count();
            return true;
        }
        false
    }

    fn current(&self) -> char {
        self.chars[self.pos]
    }

    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }
}

fn parse_html_nodes(raw: &str) -> Vec<HtmlNode> {
    HtmlBlockParser::parse(raw).unwrap_or_else(|_| vec![HtmlNode::Text(raw.to_string())])
}

pub fn parse_terlan_expr(raw: &str) -> ParseResult<Expr> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;

    let raw = normalize_interpolated_html_expr(raw.to_string());
    let tokens = match lex(raw.as_str()) {
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
    let expr = parser.parse_expr()?;
    if !parser.check(TokenKind::EOF) {
        return Err(ParseError {
            message: "unexpected tokens after expression".to_string(),
            span: parser.current().span(),
        });
    }

    Ok(expr)
}

fn normalize_interpolated_html_expr(raw: String) -> String {
    let trimmed = raw.trim();
    if let Some(normalized) = normalize_for_html_expr(trimmed) {
        return normalized;
    }
    if starts_with_html_tag(trimmed) {
        return format!("html {{ {} }}", trimmed);
    }

    normalize_case_html_branches(trimmed)
}

fn starts_with_html_tag(raw: &str) -> bool {
    let mut chars = raw.chars();
    chars.next() == Some('<')
        && chars
            .next()
            .is_some_and(|ch| ch == '/' || ch.is_ascii_alphabetic())
}

fn normalize_for_html_expr(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("for ")?;
    let raw_offset = raw.len() - rest.len();
    let chars = raw.chars().collect::<Vec<_>>();
    let body_start = chars
        .iter()
        .enumerate()
        .skip(raw_offset)
        .find_map(|(idx, ch)| (*ch == '{').then_some(idx))?;
    let body_end = find_matching_brace(&chars, body_start)?;
    if chars[body_end + 1..].iter().any(|ch| !ch.is_whitespace()) {
        return None;
    }

    let header = chars[raw_offset..body_start]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let (pattern, source) = header.split_once("<-")?;
    let body = chars[body_start + 1..body_end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let item = normalize_interpolated_html_expr(body);

    Some(format!(
        "[{} | {} <- {}]",
        item,
        pattern.trim(),
        source.trim()
    ))
}

fn find_matching_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut pos = open;
    let mut depth = 0usize;
    let mut quote = None;

    while pos < chars.len() {
        let ch = chars[pos];
        if let Some(current_quote) = quote {
            pos += 1;
            if ch == current_quote {
                quote = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            pos += 1;
            continue;
        }

        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(pos);
            }
        }

        pos += 1;
    }

    None
}

fn normalize_case_html_branches(raw: &str) -> String {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut pos = 0usize;

    while pos < chars.len() {
        if pos + 1 < chars.len() && chars[pos] == '-' && chars[pos + 1] == '>' {
            out.push_str("->");
            pos += 2;

            let ws_start = pos;
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            out.extend(chars[ws_start..pos].iter());

            if pos < chars.len() && chars[pos] == '<' {
                if let Some(end) = find_html_fragment_end(&chars, pos) {
                    out.push_str("html { ");
                    out.extend(chars[pos..end].iter());
                    out.push_str(" }");
                    pos = end;
                    continue;
                }
            }

            continue;
        }

        out.push(chars[pos]);
        pos += 1;
    }

    out
}

fn find_html_fragment_end(chars: &[char], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut stack = Vec::<String>::new();

    while pos < chars.len() {
        if chars[pos] != '<' {
            pos += 1;
            continue;
        }

        let closing = pos + 1 < chars.len() && chars[pos + 1] == '/';
        pos += if closing { 2 } else { 1 };
        let name_start = pos;
        while pos < chars.len()
            && (chars[pos].is_ascii_alphanumeric()
                || chars[pos] == '_'
                || chars[pos] == '-'
                || chars[pos] == ':')
        {
            pos += 1;
        }
        if name_start == pos {
            return None;
        }
        let name = chars[name_start..pos].iter().collect::<String>();

        let mut quote = None;
        let mut self_closing = false;
        while pos < chars.len() {
            let ch = chars[pos];
            if let Some(current_quote) = quote {
                pos += 1;
                if ch == current_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                pos += 1;
                continue;
            }

            if ch == '>' {
                self_closing = pos > start && chars[pos.saturating_sub(1)] == '/';
                pos += 1;
                break;
            }

            pos += 1;
        }

        if closing {
            if stack.pop().as_deref() != Some(name.as_str()) {
                return None;
            }
            if stack.is_empty() {
                return Some(pos);
            }
        } else if !self_closing {
            stack.push(name);
        } else if stack.is_empty() {
            return Some(pos);
        }
    }

    None
}

/// Expression field grammar context for key class and separator validation.
///
/// Inputs: selected by the caller based on the production being parsed.
/// Output: passed to expression-field parsing as a compact policy value.
/// Transformation: distinguishes Terlan source records/templates from Erlang
/// record interop without changing the emitted field representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExprFieldKind {
    Map,
    TerlanRecord,
    ErlangRecord,
}

impl Parser {
    /// Parses one expression field in a map, Terlan record/template, or
    /// Erlang record interop expression.
    ///
    /// Inputs: the parser cursor must point at the field key and `kind`
    /// selects the grammar context for accepted key classes and separators.
    /// Output: a structured expression field, or a syntax diagnostic at the
    /// offending token.
    /// Transformation: consumes the key, field separator, and value expression,
    /// preserving the key spelling in the syntax tree.
    fn parse_record_expr_field(&mut self, kind: ExprFieldKind) -> ParseResult<MapExprField> {
        let key_token = self.current().clone();
        let valid_key = match kind {
            ExprFieldKind::Map | ExprFieldKind::ErlangRecord => {
                key_token.kind == TokenKind::Atom || key_token.kind == TokenKind::Var
            }
            ExprFieldKind::TerlanRecord => key_token.kind == TokenKind::Atom,
        };
        if !valid_key {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::TerlanRecord => "expected lower-case field name".to_string(),
                    ExprFieldKind::Map | ExprFieldKind::ErlangRecord => {
                        "expected map field key atom".to_string()
                    }
                },
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) && kind == ExprFieldKind::Map {
            false
        } else {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::Map => "expected := or => in map expression".to_string(),
                    ExprFieldKind::TerlanRecord | ExprFieldKind::ErlangRecord => {
                        "expected = in struct field".to_string()
                    }
                },
                span: self.current().span(),
            });
        };

        let value = self.parse_expr()?;
        Ok(MapExprField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
    }

    fn parse_list_generator(&mut self) -> ParseResult<(Pattern, Expr)> {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::LtMinus)?;
        let source = self.parse_expr()?;
        Ok((pattern, source))
    }

    fn parse_expr_list(&mut self, end: TokenKind) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.check(end) {
            return Ok(args);
        }

        loop {
            args.push(self.parse_expr()?);
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }

        Ok(args)
    }

    fn expect_keyword(&mut self, expected: TokenKind) -> ParseResult<()> {
        self.expect(expected).map(|_| ())
    }

    fn consume_keyword(&mut self, expected: &str) -> bool {
        if self.check_keyword(expected) {
            self.pos += 1;
            return true;
        }
        false
    }

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

    fn consume_if(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn check_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|kind| self.check(kind.clone()))
    }

    fn skip_comments(&mut self) {
        while self.check(TokenKind::Comment) {
            self.pos += 1;
        }
    }

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

    fn take_item_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::DocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    fn take_module_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::ModuleDocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        self.pos += 1;
        token
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.pos - 1]
    }

    fn check_keyword(&self, expected: &str) -> bool {
        matches!(self.current().kind, TokenKind::Atom | TokenKind::Var)
            && self.current().text == expected
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn is_qualified_type_dot(&self, parts: &[String]) -> bool {
        if !self.check(TokenKind::Dot) || parts.is_empty() {
            return false;
        }
        let previous = self.tokens.get(self.pos.saturating_sub(1));
        let current = self.current();
        let next = self.tokens.get(self.pos + 1);

        match (previous, next) {
            (Some(previous), Some(next)) => {
                previous.end == current.start
                    && next.start == current.end
                    && is_identifier_like_token(&previous.kind)
                    && is_identifier_like_token(&next.kind)
            }
            _ => false,
        }
    }
}

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

fn is_identifier_like_token(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Atom | TokenKind::Ident | TokenKind::Var)
}

/// Validates a module path captured by broader import parsing.
///
/// Inputs: ordered module path segments and their source spans.
/// Outputs: `Ok(())` when the package root is lower-case and later segments are
/// identifiers, or a parse diagnostic on an invalid root.
/// Transformation: checks the imported module prefix after the parser has
/// separated it from the imported symbol name, preserving upper-case imported
/// symbols and upper-case module namespace segments.
fn validate_module_path_segments(segments: &[(String, Span)]) -> ParseResult<()> {
    if let Some((segment, span)) = segments.first() {
        if !segment
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            return Err(ParseError {
                message: "expected lower-case package root segment".to_string(),
                span: *span,
            });
        }
    }

    Ok(())
}

/// Validates a normal import alias against the canonical import-symbol class
/// rule.
///
/// Inputs: the imported symbol name, alias text, and alias source span.
/// Outputs: `Ok(())` when both names have the same lower/upper identifier
/// class, or a parse diagnostic anchored to the alias.
/// Transformation: classifies the first character of both names and rejects
/// aliases that would change a lower import into an upper name, or an upper
/// import into a lower name.
fn validate_import_alias_class(name: &str, alias: &str, alias_span: Span) -> ParseResult<()> {
    let name_is_upper = name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase());
    let alias_is_upper = alias
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase());

    if name_is_upper != alias_is_upper {
        return Err(ParseError {
            message: "import alias must preserve identifier class".to_string(),
            span: alias_span,
        });
    }

    Ok(())
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
///   left-to-right source order while reusing the current single-guard AST
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

fn is_raw_declaration_name(name: &str) -> bool {
    matches!(name, "target" | "native" | "machine" | "static")
}

fn is_send_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BinaryOp {
            op: BinaryOp::Send,
            ..
        }
    )
}

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

fn validate_constructor_clause_shapes(clauses: &[ConstructorClause]) -> ParseResult<()> {
    for (idx, left) in clauses.iter().enumerate() {
        for right in clauses.iter().skip(idx + 1) {
            let left_varargs = left.params.iter().any(|param| param.is_varargs);
            let right_varargs = right.params.iter().any(|param| param.is_varargs);

            if left_varargs && right_varargs {
                return Err(ParseError {
                    message: "constructor has ambiguous varargs clauses".to_string(),
                    span: right.span,
                });
            }

            if !left_varargs && !right_varargs {
                let left_range = constructor_clause_arity_range(left);
                let right_range = constructor_clause_arity_range(right);
                if ranges_overlap(left_range, right_range) {
                    return Err(ParseError {
                        message: "constructor has ambiguous arity clauses".to_string(),
                        span: right.span,
                    });
                }
            }
        }
    }

    Ok(())
}

fn constructor_clause_arity_range(clause: &ConstructorClause) -> (usize, usize) {
    let max = clause.params.len();
    let min = clause
        .params
        .iter()
        .filter(|param| param.default.is_none())
        .count();
    (min, max)
}

fn ranges_overlap(left: (usize, usize), right: (usize, usize)) -> bool {
    left.0 <= right.1 && right.0 <= left.1
}

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

fn invalid_runtime_type_token(input: &str) -> Option<&'static str> {
    const INVALID: &[&str] = &[
        "case", "receive", "if", "when", "and", "&&", "or", "||", "not", "|>", "==", "!=", "=:=",
        "/=", "=/=", "*", "/", "div", "rem", "!",
    ];

    for token in INVALID {
        if token.chars().all(|ch| ch.is_ascii_alphabetic()) {
            if input
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .any(|word| word == *token)
            {
                return Some(token);
            }
        } else if input.contains(token) {
            return Some(token);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::ast::{BuiltinBlockMacro, Decl, Expr, HtmlAttrValue, HtmlNode, UnaryOp};
    use crate::{parse_interface_module, parse_module, parse_terlan_expr};

    /// Verifies release core collection contracts stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when all three release modules parse as normal source
    ///   modules and keep their canonical module names.
    ///
    /// Transformation:
    /// - Parses release contracts with compiler intrinsic annotations and
    ///   placeholder bodies without typechecking or backend emission, proving
    ///   the P0.3 release source shape remains grammar-stable.
    #[test]
    fn parses_release_core_collection_contracts() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.tl"),
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.tl"),
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.tl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection contract");
            assert_eq!(module.name, expected_module);
        }
    }

    /// Verifies release iterator/iterable modules stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when both modules parse in source mode and keep their
    ///   canonical module names.
    ///
    /// Transformation:
    /// - Parses release traversal modules without typechecking or backend
    ///   emission, proving P0.4b exposes traversal contracts while allowing
    ///   source-implemented helpers such as `Iterator.each`.
    #[test]
    fn parses_release_traversal_contracts() {
        let contracts = [
            (
                "std.collections.Iterator",
                include_str!("../../../std/collections/iterator.tl"),
            ),
            (
                "std.collections.Iterable",
                include_str!("../../../std/collections/iterable.tl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection trait module");
            assert_eq!(module.name, expected_module);
        }
    }

    #[test]
    fn formal_expr_precedence_keeps_send_above_full_pipe_chain() {
        let expr = parse_terlan_expr("A ! B |> C + D * E").expect("parse formal precedence");
        let Expr::BinaryOp { op, left: _, right } = expr else {
            panic!("expected send expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Send));

        let Expr::BinaryOp { op, left: _, right } = right.as_ref() else {
            panic!("expected pipe expression on send right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected additive expression on pipe right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Add));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::ast::BinaryOp::Mul,
                ..
            }
        ));
    }

    /// Verifies the boolean precedence chain introduced by the canonical EBNF.
    ///
    /// Inputs:
    /// - A source expression containing `or`, `and`, comparison, arithmetic,
    ///   pipe, and send operators.
    ///
    /// Output:
    /// - Test passes when parsing preserves `send < pipe < or < and < cmp`.
    ///
    /// Transformation:
    /// - Parses one expression through the recursive-descent parser and
    ///   inspects the nested binary operator tree.
    #[test]
    fn formal_boolean_operators_preserve_ebnf_precedence() {
        let expr =
            parse_terlan_expr("A ! B |> C or D and E == F + G").expect("parse boolean precedence");
        let Expr::BinaryOp { op, right, .. } = expr else {
            panic!("expected send expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Send));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected pipe expression on send right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected or expression on pipe right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Or));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected and expression on or right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::And));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected comparison expression on and right side");
        };
        assert!(matches!(op, crate::ast::BinaryOp::EqEq));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::ast::BinaryOp::Add,
                ..
            }
        ));
    }

    /// Verifies explicit cast syntax follows the canonical precedence chain.
    ///
    /// Inputs:
    /// - Expressions containing `as`, multiplication, pipe, and keyword forms.
    ///
    /// Output:
    /// - Test passes when `Cast` binds above multiplication and below postfix
    ///   primary parsing, including keyword expressions.
    ///
    /// Transformation:
    /// - Parses representative expressions and inspects the preserved syntax
    ///   tree instead of resolving the conversion semantically.
    #[test]
    fn formal_cast_expr_preserves_ebnf_precedence() {
        let expr = parse_terlan_expr("Value as Int * Count").expect("parse cast before multiply");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected multiplication expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Mul));
        assert!(matches!(
            left.as_ref(),
            Expr::Cast {
                target_type,
                ..
            } if target_type.text == "Int"
        ));

        let expr =
            parse_terlan_expr("case Option { :none -> 0; value -> value } as Int |> inspect()")
                .expect("parse casted keyword expression before pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Cast { expr, target_type } = left.as_ref() else {
            panic!("expected cast expression on pipe left side");
        };
        assert_eq!(target_type.text, "Int");
        assert!(matches!(expr.as_ref(), Expr::Case { .. }));
    }

    /// Verifies that canonical Terlan source rejects backend-style equality
    /// spellings.
    ///
    /// Inputs:
    /// - Three source expressions using deprecated equality spellings.
    ///
    /// Output:
    /// - Test passes when all deprecated spellings fail parsing.
    ///
    /// Transformation:
    /// - Parses each expression through the recursive-descent parser and
    ///   asserts the comparison operator guard fires before syntax output is
    ///   accepted.
    #[test]
    fn formal_deprecated_equality_operators_are_rejected() {
        for operator in ["=:=", "/=", "=/="] {
            let source = format!("left {operator} right");
            let error = parse_terlan_expr(&source)
                .err()
                .expect("deprecated equality spelling should fail");

            assert!(
                error.message.contains("deprecated"),
                "unexpected diagnostic for {operator}: {}",
                error.message
            );
        }
    }

    /// Verifies that `rem` keeps a distinct AST operator instead of collapsing
    /// into `div`.
    ///
    /// Inputs:
    /// - A source expression using the formal `rem` multiplicative operator.
    ///
    /// Output:
    /// - Test passes when the parsed expression carries `BinaryOp::Rem`.
    ///
    /// Transformation:
    /// - Parses one expression through the recursive-descent parser and
    ///   inspects the binary operator identity preserved for syntax-output and
    ///   backend lowering.
    #[test]
    fn formal_rem_operator_preserves_distinct_binary_op() {
        let expr = parse_terlan_expr("x rem y").expect("parse rem expression");
        let Expr::BinaryOp { op, .. } = expr else {
            panic!("expected rem binary expression");
        };

        assert!(matches!(op, crate::ast::BinaryOp::Rem));
    }

    #[test]
    fn formal_keyword_expr_participates_in_pipe_expression() {
        let expr = parse_terlan_expr(
            r#"
            case Option {
              None -> 0;
                    Ok(value) -> value
            } |> inspect()
            "#,
        )
        .expect("parse keyword expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Case { clauses, .. } = left.as_ref() else {
            panic!("expected case expression as pipe left side");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::ast::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::ast::Pattern::Atom(name)] if name == "None")
        ));
        assert!(matches!(
            &clauses[1].pattern,
            crate::ast::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::ast::Pattern::Atom(name), crate::ast::Pattern::Var(var)] if name == "Ok" && var == "value")
        ));
    }

    #[test]
    fn formal_raw_atom_patterns_are_literal_patterns() {
        let module = parse_module(
            r#"
            module atoms.

            value(Status: Status): Int ->
                case Status {
                    :none -> 0;
                    :empty -> 1
                }.
            "#,
        )
        .expect("parse raw atom patterns");

        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function");
        };
        let Expr::Case { clauses, .. } = &function.clauses[0].body else {
            panic!("expected case expression");
        };
        assert!(matches!(&clauses[0].pattern, crate::ast::Pattern::Atom(name) if name == "none"));
        assert!(matches!(&clauses[1].pattern, crate::ast::Pattern::Atom(name) if name == "empty"));
    }

    /// Verifies expanded pattern families accepted by the A0.25 syntax
    /// baseline.
    ///
    /// Inputs:
    /// - A module containing map, list-cons, literal, tuple, and
    ///   constructor-style patterns.
    ///
    /// Output:
    /// - Test passes when each pattern family is preserved in the syntax AST.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser, locates case
    ///   clauses, and inspects the pattern variants and selected guard fields.
    #[test]
    fn formal_pattern_expansion_preserves_ast_shapes() {
        let module = parse_module(
            r#"
            module pattern_shapes.

            map_pattern(value: Map): Int ->
              case value {
                #{kind := :ok, count => n} when n > 0 -> n;
                #{} -> 0
              }.

            list_cons_pattern(values: List[Int]): Int ->
              case values {
                [head | tail] when head > 0 -> head;
                [] -> 0
              }.

            literal_patterns(value: Dynamic): Int ->
              case value {
                :none -> 0;
                1.5 -> 1;
                {left, right} -> 2
              }.

            constructor_patterns(value: Dynamic): Int ->
              case value {
                None -> 0;
                Ok(item) -> item
              }.
            "#,
        )
        .expect("parse pattern expansion");

        let Decl::Function(map_function) = &module.declarations[0] else {
            panic!("expected map pattern function");
        };
        let Expr::Case { clauses, .. } = &map_function.clauses[0].body else {
            panic!("expected map pattern case");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::ast::Pattern::Map(fields) if fields.len() == 2)
        );
        assert!(clauses[0].guard.is_some());
        assert!(
            matches!(&clauses[1].pattern, crate::ast::Pattern::Map(fields) if fields.is_empty())
        );

        let Decl::Function(cons_function) = &module.declarations[1] else {
            panic!("expected cons pattern function");
        };
        let Expr::Case { clauses, .. } = &cons_function.clauses[0].body else {
            panic!("expected cons pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::ast::Pattern::ListCons(_, _)
        ));
        assert!(clauses[0].guard.is_some());

        let Decl::Function(literal_function) = &module.declarations[2] else {
            panic!("expected literal pattern function");
        };
        let Expr::Case { clauses, .. } = &literal_function.clauses[0].body else {
            panic!("expected literal pattern case");
        };
        assert!(matches!(&clauses[0].pattern, crate::ast::Pattern::Atom(name) if name == "none"));
        assert!(
            matches!(&clauses[1].pattern, crate::ast::Pattern::Float(value) if (*value - 1.5).abs() < f64::EPSILON)
        );
        assert!(
            matches!(&clauses[2].pattern, crate::ast::Pattern::Tuple(items) if items.len() == 2)
        );

        let Decl::Function(constructor_function) = &module.declarations[3] else {
            panic!("expected constructor pattern function");
        };
        let Expr::Case { clauses, .. } = &constructor_function.clauses[0].body else {
            panic!("expected constructor pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::ast::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::ast::Pattern::Atom(name)] if name == "None")
        ));
        assert!(matches!(
            &clauses[1].pattern,
            crate::ast::Pattern::Tuple(items)
                if matches!(
                    items.as_slice(),
                    [crate::ast::Pattern::Atom(name), crate::ast::Pattern::Var(var)]
            if name == "Ok" && var == "item"
                )
        ));
    }

    /// Verifies every parser-visible declaration class in the canonical
    /// declaration inventory.
    ///
    /// Inputs:
    /// - A module containing imports, type, opaque type, struct, constructor,
    ///   trait, method, template, config macro, and function declarations.
    ///
    /// Output:
    /// - Test passes when parser declaration variants appear in the expected
    ///   order and module identity is stored separately from declarations.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and maps each
    ///   declaration variant to the same inventory classes used by grammar
    ///   fixture validation.
    #[test]
    fn formal_declaration_inventory_covers_parser_decl_classes() {
        let module = parse_module(
            r#"
            module declaration.inventory.

            import std.core.String.
            import type std.core.Option.
            import std.core.Option.{map as map_option, Option as MaybeOption}.
            import markdown "./readme.md" as readme.

            pub type Alias[T] = {:ok, value: T} | :none.
            pub opaque type Secret = Int.

            pub struct User {
              id: Int,
              name: String = ""
            }.

            pub constructor User {
              (id: Int, name: String): User -> #{
                id := id,
                name := name
              }
            }.

            pub trait Show[T] {
              show(value: T): String.
            }.

            (self: User) display(): User -> self.

            template Card from "./card.html" {
              title: String
            }.

            target js {
              runtime: oxc
            }.

            pub identity(value: Int): Int -> value.
            "#,
        )
        .expect("parse declaration inventory");

        assert_eq!(module.name, "declaration.inventory");
        let classes = module
            .declarations
            .iter()
            .map(|decl| match decl {
                Decl::Import(_) => "import_decl",
                Decl::Type(type_decl) if type_decl.is_opaque => "opaque_type_decl",
                Decl::Type(_) => "type_decl",
                Decl::Struct(_) => "struct_decl",
                Decl::Constructor(_) => "constructor_decl",
                Decl::Function(_) => "function_decl",
                Decl::Method(_) => "method_decl",
                Decl::Trait(_) => "trait_decl",
                Decl::TraitImpl(_) => "trait_impl_decl",
                Decl::Template(_) => "template_decl",
                Decl::Raw(_) => "raw_decl",
                Decl::Export(_) => panic!("source parser must not produce export declarations"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            classes,
            vec![
                "import_decl",
                "import_decl",
                "import_decl",
                "import_decl",
                "type_decl",
                "opaque_type_decl",
                "struct_decl",
                "constructor_decl",
                "trait_decl",
                "method_decl",
                "template_decl",
                "raw_decl",
                "function_decl"
            ]
        );
    }

    /// Verifies the A0.27 type-family syntax inventory.
    ///
    /// Inputs:
    /// - A module containing aliases, opaque aliases, unions, tuples, named
    ///   tuple fields, map types, arrow types, generic references, lists, and
    ///   type literals.
    ///
    /// Output:
    /// - Test passes when type declarations parse and preserve their type text
    ///   for later semantic/type-family validation.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   selected preserved `TypeExpr` text and opaque/public flags.
    #[test]
    fn formal_type_family_inventory_preserves_type_expr_text() {
        let module = parse_module(
            r#"
            module types.family.inventory.

            pub type Maybe[T] = :none | {:some, value: T}.
            type Pair = {left: Int, right: String}.
            type IgnoredField = {_: Int, value: String}.
            type Lookup[K, V] = #{key := K, value => V}.
            type Mapper[A, B] = (A) -> B.
            type Nested = std.core.Option[String].
            type Names = [String].
            type LiteralUnion = :empty | :'Interop.Empty' | 0 | 1.5 | "ready".

            pub opaque type Secret[T] = #{value := T}.
            pub opaque type Handle.
            "#,
        )
        .expect("parse type-family inventory");

        assert_eq!(module.declarations.len(), 10);

        let Decl::Type(maybe) = &module.declarations[0] else {
            panic!("expected Maybe type");
        };
        assert!(maybe.is_public);
        assert_eq!(maybe.params, vec!["T"]);
        assert_eq!(maybe.variants.len(), 2);
        assert!(maybe.variants[0].text.contains("none"));
        assert!(maybe.variants[1].text.contains("value"));

        let Decl::Type(mapper) = &module.declarations[4] else {
            panic!("expected Mapper type");
        };
        assert_eq!(mapper.params, vec!["A", "B"]);
        assert_eq!(mapper.variants.len(), 1);
        assert!(mapper.variants[0].text.contains("->"));

        let Decl::Type(nested) = &module.declarations[5] else {
            panic!("expected Nested type");
        };
        assert!(nested.variants[0].text.contains("std.core.Option"));
        assert!(nested.variants[0].text.contains("[String]"));

        let Decl::Type(secret) = &module.declarations[8] else {
            panic!("expected Secret opaque type");
        };
        assert!(secret.is_public);
        assert!(secret.is_opaque);
        assert_eq!(secret.params, vec!["T"]);
        assert!(secret.variants[0].text.contains("value"));

        let Decl::Type(handle) = &module.declarations[9] else {
            panic!("expected Handle opaque type");
        };
        assert!(handle.is_public);
        assert!(handle.is_opaque);
        assert!(handle.variants.is_empty());
    }

    /// Verifies type-position diagnostics for runtime expression syntax.
    ///
    /// Inputs:
    /// - A type declaration whose right-hand side starts with a `case`
    ///   expression.
    ///
    /// Output:
    /// - Test passes when parsing fails before the type can enter later
    ///   compiler phases.
    ///
    /// Transformation:
    /// - Parses one malformed module and asserts the stable runtime-token
    ///   diagnostic remains attached to type parsing.
    #[test]
    fn formal_type_family_rejects_runtime_expression_tokens() {
        let error = parse_module(
            r#"
            module bad.bad_type.

            type Foo = case x { y -> z }.
            "#,
        )
        .err()
        .expect("runtime expression in type should fail");

        assert!(
            error
                .message
                .contains("runtime expression token 'case' is not valid in type position"),
            "unexpected diagnostic: {}",
            error.message
        );
    }

    /// Verifies the A0.28 method receiver syntax baseline.
    ///
    /// Inputs:
    /// - A module with a struct and two receiver method declarations,
    ///   including receiver type arguments, method parameters, visibility, and
    ///   field access in a method body.
    ///
    /// Output:
    /// - Test passes when methods are accepted as structured `MethodDecl`
    ///   declarations and preserve receiver, method, and body data.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   the structured receiver-method AST used by later syntax output,
    ///   typechecking, and backend lowering.
    #[test]
    fn formal_method_receiver_inventory_preserves_validated_methods() {
        let module = parse_module(
            r#"
            module methods.receiver.inventory.

            struct Box {
              value: Int
            }.

            (self: Box[Int]) value(): Int -> self.value.

            pub (self: Box[Int]) replace(value: Int): Box[Int] -> self.
            "#,
        )
        .expect("parse method receiver inventory");

        assert_eq!(module.declarations.len(), 3);
        assert!(matches!(&module.declarations[0], Decl::Struct(_)));

        let Decl::Method(value_method) = &module.declarations[1] else {
            panic!("expected first method");
        };
        assert_eq!(value_method.name, "value");
        assert_eq!(value_method.receiver.name, "self");
        assert_eq!(value_method.receiver.annotation.text, "Box[Int]");
        assert!(!value_method.receiver.is_mutable);

        let Decl::Method(replace_method) = &module.declarations[2] else {
            panic!("expected second method");
        };
        assert_eq!(replace_method.name, "replace");
        assert_eq!(replace_method.params.len(), 1);
        assert!(replace_method.is_public);
        assert!(!replace_method.receiver.is_mutable);
    }

    /// Verifies mutable receiver syntax is parsed without enabling semantics.
    ///
    /// Inputs:
    /// - A module with a receiver method declared as `(mut self: Box[Int])`.
    ///
    /// Output:
    /// - Test passes when the method is preserved as a structured declaration
    ///   and the receiver metadata records `is_mutable`.
    ///
    /// Transformation:
    /// - Parses the contextual `mut` marker before the receiver binding and
    ///   stores it on the receiver parameter for later semantic validation.
    #[test]
    fn formal_method_receiver_inventory_preserves_mutable_receiver_marker() {
        let module = parse_module(
            r#"
            module methods.receiver.mutable.

            struct Box {
              value: Int
            }.

            pub (mut self: Box[Int]) replace(value: Int): Box[Int] -> self.
            "#,
        )
        .expect("parse mutable method receiver inventory");

        let Decl::Method(method) = &module.declarations[1] else {
            panic!("expected mutable receiver method");
        };
        assert_eq!(method.name, "replace");
        assert_eq!(method.receiver.name, "self");
        assert_eq!(method.receiver.annotation.text, "Box[Int]");
        assert!(method.receiver.is_mutable);
    }

    /// Verifies method receiver/name diagnostics required by A0.28.
    ///
    /// Inputs:
    /// - Three malformed method declarations with an upper-case receiver
    ///   binding, lower-case receiver type, and upper-case method name.
    ///
    /// Output:
    /// - Test passes when each malformed method fails with the expected stable
    ///   diagnostic fragment.
    ///
    /// Transformation:
    /// - Parses each module independently and compares the diagnostic message
    ///   against the receiver/method grammar rule that was violated.
    #[test]
    fn formal_method_receiver_diagnostics_reject_invalid_method_heads() {
        let cases = [
            (
                r#"
                module bad.uppercase_method_receiver_name.

                struct User {
                  id: Int
                }.

                (Self: User) identity(): User -> Self.
                "#,
                "expected lower-case method receiver name",
            ),
            (
                r#"
                module bad.lowercase_method_receiver_type.

                (self: user) identity(): user -> self.
                "#,
                "expected upper-case type name",
            ),
            (
                r#"
                module bad.uppercase_method_name.

                struct User {
                  id: Int
                }.

                (self: User) Rename(): User -> self.
                "#,
                "expected lower-case method name",
            ),
        ];

        for (source, expected) in cases {
            let error = parse_module(source)
                .err()
                .expect("invalid method head should fail");
            assert!(
                error.message.contains(expected),
                "expected diagnostic containing `{expected}`, got `{}`",
                error.message
            );
        }
    }

    /// Verifies unsupported annotation subjects fail with a stable diagnostic.
    ///
    /// Inputs:
    /// - Modules containing subject-bearing annotation forms that are
    ///   unambiguous without line-boundary information.
    ///
    /// Output:
    /// - Parser diagnostics containing the A0.32 unsupported-subject message.
    ///
    /// Transformation:
    /// - Parses each source module and confirms annotation subjects are stopped
    ///   before declaration routing or backend phases can observe them.
    #[test]
    fn formal_annotation_subjects_are_rejected_before_declaration_routing() {
        let cases = [
            r#"
            module bad.annotation_upper_subject.

            @compiler.inline User
            type User = Int.
            "#,
            r#"
            module bad.annotation_qualified_subject.

            @target std.core {
              enabled: true
            }
            type User = Int.
            "#,
            r#"
            module bad.annotation_literal_subject.

            @doc "User type"
            type User = Int.
            "#,
        ];

        for source in cases {
            let error = parse_module(source)
                .err()
                .expect("annotation subject should fail");
            assert!(
                error
                    .message
                    .contains("annotation subjects are not supported in Terlan 0.0.1"),
                "unexpected diagnostic: {}",
                error.message
            );
        }
    }

    /// Verifies declaration-leading annotations still support lower-case
    /// functions despite the subject rejection pass.
    ///
    /// Inputs:
    /// - A module with a declaration-leading `@test` annotation before a
    ///   lower-case function declaration.
    ///
    /// Output:
    /// - A parsed module containing one annotated function declaration.
    ///
    /// Transformation:
    /// - Exercises the ambiguous lower-identifier case that is intentionally
    ///   left to declaration parsing until lexer line-boundary data exists.
    #[test]
    fn formal_declaration_annotation_before_function_still_parses() {
        let module = parse_module(
            r#"
            module ok.annotation_function.

            @test
            passes(): Bool -> true.
            "#,
        )
        .expect("declaration-leading annotation");

        assert_eq!(module.declarations.len(), 1);
        assert_eq!(module.declaration_annotations.len(), 1);
        assert_eq!(module.declaration_annotations[0][0].path, vec!["test"]);
    }

    /// Verifies the A0.29 trait and primitive conformance syntax inventory.
    ///
    /// Inputs:
    /// - A module declaring `Show`, `Parse`, `Convertable`, and `Textual`
    ///   traits plus functions that call trait methods for primitive `Bool`.
    ///
    /// Output:
    /// - Test passes when trait declarations, super-trait references, method
    ///   signatures, and trait method calls are preserved by the parser.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser, inspects trait
    ///   declaration metadata, and confirms trait calls remain ordinary
    ///   function declarations for later semantic conformance resolution.
    #[test]
    fn formal_trait_conformance_inventory_preserves_trait_surface() {
        let module = parse_module(
            r#"
            module traits.conformance.inventory.

            pub trait Show[T] {
              to_string(value: T): String.
            }.

            pub trait Parse[T] {
              from_string(value: String): Option[T].
            }.

            pub trait Convertable[From, To] {
              convert(value: From): To.
            }.

            pub trait Textual[T] extends Convertable[T, String], Convertable[String, T] {
            }.

            render_bool(value: Bool): String ->
              Show.to_string(value).

            parse_bool(value: String): Option[Bool] ->
              Parse.from_string(value).
            "#,
        )
        .expect("parse trait conformance inventory");

        assert_eq!(module.declarations.len(), 6);

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert!(show.is_public);
        assert_eq!(show.name, "Show");
        assert_eq!(show.params, vec!["T"]);
        assert_eq!(show.methods.len(), 1);
        assert_eq!(show.methods[0].name, "to_string");
        assert_eq!(show.methods[0].return_type.text, "String");

        let Decl::Trait(parse) = &module.declarations[1] else {
            panic!("expected Parse trait");
        };
        assert_eq!(parse.methods[0].name, "from_string");
        assert!(parse.methods[0].return_type.text.contains("Option"));

        let Decl::Trait(textual) = &module.declarations[3] else {
            panic!("expected Textual trait");
        };
        assert_eq!(textual.super_traits.len(), 2);
        assert!(textual.super_traits[0].contains("Convertable"));
        assert!(textual.super_traits[1].contains("String"));

        assert!(matches!(&module.declarations[4], Decl::Function(_)));
        assert!(matches!(&module.declarations[5], Decl::Function(_)));
    }

    /// Verifies declaration-site trait conformance syntax preserves the
    /// Java-style `implements` form without requiring an explicit impl block.
    ///
    /// Inputs:
    /// - A struct declaring `implements Show[User]`.
    /// - A receiver method satisfying that conformance.
    ///
    /// Output:
    /// - Parsed declaration shapes and conformance metadata.
    ///
    /// Transformation:
    /// - Parses the source through the formal recursive-descent parser and
    ///   confirms declaration-site conformance is preserved on the struct while
    ///   behavior remains an ordinary receiver method.
    #[test]
    fn formal_trait_conformance_syntax_supports_implements_with_receiver_method() {
        let module = parse_module(
            r#"
            module traits.conformance.forms.

            pub trait Show[T] {
              to_string(value: T): String.
            }.

            pub struct User implements Show[User] {
              id: Int,
              name: String
            }.

            pub (user: User) to_string(): String ->
              user.name.
            "#,
        )
        .expect("parse declaration-site conformance form");

        assert_eq!(module.declarations.len(), 3);

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert_eq!(show.methods.len(), 1);
        assert!(show.methods[0].default_body.is_none());

        let Decl::Struct(user) = &module.declarations[1] else {
            panic!("expected User struct");
        };
        assert_eq!(user.implements.len(), 1);
        assert_eq!(user.implements[0].text, "Show[User]");

        assert!(
            matches!(&module.declarations[2], Decl::Method(method) if method.name == "to_string")
        );
    }

    /// Verifies explicit trait implementation blocks are parsed as adapter
    /// conformances.
    ///
    /// Inputs:
    /// - A module with `impl Show[ExternalUser] for ExternalUser`.
    ///
    /// Output:
    /// - Parsed `TraitImplDecl` with one implementation method.
    ///
    /// Transformation:
    /// - Confirms explicit adapter conformance is structured separately from
    ///   declaration-site `implements` and from raw declarations.
    #[test]
    fn formal_trait_conformance_syntax_supports_explicit_impl_blocks() {
        let module = parse_module(
            r#"
            module traits.conformance.adapter.

            pub impl Show[ExternalUser] for ExternalUser {
              to_string(value: ExternalUser): String ->
                value.name.
            }.
            "#,
        )
        .expect("parse explicit conformance adapter");

        assert_eq!(module.declarations.len(), 1);
        let Decl::TraitImpl(external_impl) = &module.declarations[0] else {
            panic!("expected explicit trait impl");
        };
        assert!(external_impl.is_public);
        assert_eq!(external_impl.trait_ref.text, "Show[ExternalUser]");
        assert_eq!(external_impl.for_type.text, "ExternalUser");
        assert_eq!(external_impl.methods.len(), 1);
        assert_eq!(external_impl.methods[0].name, "to_string");
        assert_eq!(external_impl.methods[0].clauses.len(), 1);
    }

    /// Verifies traits may provide default method bodies.
    ///
    /// Inputs:
    /// - A trait with one signature-only method and one default method.
    ///
    /// Output:
    /// - Trait method metadata indicating which method owns a default body.
    ///
    /// Transformation:
    /// - Parses default trait behavior without introducing an external impl
    ///   declaration, matching the Java-style default-method model.
    #[test]
    fn formal_trait_conformance_syntax_supports_trait_default_methods() {
        let module = parse_module(
            r#"
            module traits.conformance.defaults.

            pub trait Show[T] {
              to_string(value: T): String.
              debug(value: T): String -> to_string(value).
            }.
            "#,
        )
        .expect("parse default trait method");

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert_eq!(show.methods.len(), 2);
        assert!(show.methods[0].default_body.is_none());
        assert!(show.methods[1].default_body.is_some());
    }

    #[test]
    fn formal_cons_list_expr_is_distinct_from_generator_expr() {
        let cons = parse_terlan_expr("[Head | Tail]").expect("parse cons list expression");
        assert!(matches!(cons, Expr::ListCons(_, _)));

        let generator = parse_terlan_expr("[Item | Item <- Items]").expect("parse generator");
        assert!(matches!(generator, Expr::ListComprehension { .. }));
    }

    #[test]
    fn formal_list_comprehension_rejects_unrepresented_extra_generators() {
        let err = parse_terlan_expr("[Item | Item <- Items, Other <- Others]")
            .err()
            .expect("multiple generators should be rejected");

        assert!(
            err.message
                .contains("multiple list comprehension generators are not supported"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn formal_list_comprehension_accepts_stacked_filters_as_guard() {
        let expr = parse_terlan_expr("[Item | Item <- Items, Item > 0, Item < 10]")
            .expect("stacked list comprehension filters should parse");

        let Expr::ListComprehension {
            guard: Some(guard), ..
        } = expr
        else {
            panic!("expected guarded list comprehension");
        };
        let Expr::BinaryOp {
            op, left, right, ..
        } = guard.as_ref()
        else {
            panic!("expected combined filter guard");
        };
        assert!(matches!(op, crate::ast::BinaryOp::And));
        assert!(matches!(
            left.as_ref(),
            Expr::BinaryOp {
                op: crate::ast::BinaryOp::Gt,
                ..
            }
        ));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::ast::BinaryOp::Lt,
                ..
            }
        ));
    }

    /// Verifies collection expressions accepted by the A0.24 syntax baseline.
    ///
    /// Inputs:
    /// - Source expressions for list, cons-list, generator, fixed-array, and
    ///   map forms.
    ///
    /// Output:
    /// - Test passes when each expression maps to its dedicated syntax AST
    ///   variant.
    ///
    /// Transformation:
    /// - Parses each expression through the recursive-descent parser and
    ///   inspects the collection-specific AST shape.
    #[test]
    fn formal_collection_exprs_preserve_ast_shapes() {
        let list = parse_terlan_expr("[1, 2, 3]").expect("parse list expression");
        assert!(matches!(list, Expr::List(items) if items.len() == 3));

        let cons = parse_terlan_expr("[Head | Tail]").expect("parse cons list expression");
        assert!(matches!(cons, Expr::ListCons(_, _)));

        let generator =
            parse_terlan_expr("[Item * 2 | Item <- Items]").expect("parse list generator");
        assert!(matches!(
            generator,
            Expr::ListComprehension { guard: None, .. }
        ));

        let fixed = parse_terlan_expr("#[255, 128, 0]").expect("parse fixed array");
        assert!(matches!(fixed, Expr::FixedArray(items) if items.len() == 3));

        let map = parse_terlan_expr("#{name := \"Ada\", age => 42}").expect("parse map");
        let Expr::Map(fields) = map else {
            panic!("expected map expression");
        };
        assert_eq!(fields.len(), 2);
        assert!(fields[0].required);
        assert!(!fields[1].required);
    }

    /// Verifies binary segment syntax is preserved by the syntax parser.
    ///
    /// Inputs:
    /// - A binary literal containing size and segment-type annotations.
    ///
    /// Output:
    /// - Test passes when the parser preserves the full binary literal text.
    ///
    /// Transformation:
    /// - Parses the binary literal as an expression and checks that semantic
    ///   segment lowering remains deferred beyond the syntax phase.
    #[test]
    fn formal_binary_segments_are_preserved_as_binary_literal_text() {
        let expr = parse_terlan_expr("<<head:16/big-unsigned-integer, tail/binary>>")
            .expect("parse binary segment literal");

        let Expr::Binary(text) = expr else {
            panic!("expected binary literal");
        };
        assert!(text.contains("head:16/big-unsigned-integer"));
        assert!(text.contains("tail/binary"));
    }

    #[test]
    fn formal_receive_expr_parses_as_keyword_expression() {
        let expr = parse_terlan_expr(
            r#"
            receive {
                {:ok, value} -> value;
                :stop -> 0
            } |> inspect()
            "#,
        )
        .expect("parse receive expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Receive { clauses, .. } = left.as_ref() else {
            panic!("expected receive expression as pipe left side");
        };
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn formal_receive_expr_parses_after_clause() {
        let expr = parse_terlan_expr(
            r#"
            receive {
                {:ok, value} -> value;
                after
                    0 -> fallback()
                } |> inspect()
            "#,
        )
        .expect("parse receive after expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Receive {
            clauses,
            after_clause,
        } = left.as_ref()
        else {
            panic!("expected receive expression as pipe left side");
        };
        assert_eq!(clauses.len(), 1);
        let after_clause = after_clause
            .as_ref()
            .expect("expected receive after clause");
        assert!(matches!(after_clause.trigger.as_ref(), Expr::Int(0)));
        assert!(matches!(
            after_clause.body.as_ref(),
            Expr::Call { remote: None, .. }
        ));
    }

    #[test]
    fn formal_try_expr_parses_of_and_catch_clauses() {
        let expr = parse_terlan_expr(
            r#"
            try risky() {
                {:ok, value} -> value
            catch
                :error -> 0
            } |> inspect()
            "#,
        )
        .expect("parse try expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Try {
            of_clauses,
            catch_clauses,
            ..
        } = left.as_ref()
        else {
            panic!("expected try expression as pipe left side");
        };
        assert_eq!(of_clauses.len(), 1);
        assert_eq!(catch_clauses.len(), 1);
    }

    #[test]
    fn formal_try_expr_parses_after_clause() {
        let expr = parse_terlan_expr(
            r#"
            try risky() {
                after
                0 -> cleanup()
            } |> inspect()
            "#,
        )
        .expect("parse try after expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Try { after_clause, .. } = left.as_ref() else {
            panic!("expected try expression as pipe left side");
        };
        let after_clause = after_clause.as_ref().expect("expected try after clause");
        assert!(matches!(after_clause.trigger.as_ref(), Expr::Int(0)));
        assert!(matches!(
            after_clause.body.as_ref(),
            Expr::Call { remote: None, .. }
        ));
    }

    /// Verifies guarded clauses in keyword expressions.
    ///
    /// Inputs:
    /// - A module containing guarded `case`, `receive`, and `try` clauses.
    ///
    /// Output:
    /// - Test passes when each keyword expression preserves a guard expression
    ///   on its first clause.
    ///
    /// Transformation:
    /// - Parses a module through the recursive-descent parser, locates the
    ///   function bodies, and inspects the keyword-expression clause guards.
    #[test]
    fn formal_keyword_exprs_preserve_clause_guards() {
        let module = parse_module(
            r#"
            module keyword_guards.

            guarded_case(value: Int): Int ->
              case value {
                n when n > 0 -> n;
                _ -> 0
              }.

            guarded_receive(): Int ->
              receive {
                value when value > 0 -> value;
                _ -> 0
              }.

            guarded_try(): Int ->
              try risky() {
                value when value > 0 -> value;
                _ -> 0
              catch
                reason when reason != :fatal -> 0;
                _ -> -1
              }.
            "#,
        )
        .expect("parse guarded keyword expressions");

        let Decl::Function(case_function) = &module.declarations[0] else {
            panic!("expected case function");
        };
        let Expr::Case { clauses, .. } = &case_function.clauses[0].body else {
            panic!("expected case expression");
        };
        assert!(clauses[0].guard.is_some());

        let Decl::Function(receive_function) = &module.declarations[1] else {
            panic!("expected receive function");
        };
        let Expr::Receive { clauses, .. } = &receive_function.clauses[0].body else {
            panic!("expected receive expression");
        };
        assert!(clauses[0].guard.is_some());

        let Decl::Function(try_function) = &module.declarations[2] else {
            panic!("expected try function");
        };
        let Expr::Try {
            of_clauses,
            catch_clauses,
            ..
        } = &try_function.clauses[0].body
        else {
            panic!("expected try expression");
        };
        assert!(of_clauses[0].guard.is_some());
        assert!(catch_clauses[0].guard.is_some());
    }

    /// Verifies quote and unquote participate in formal keyword-expression
    /// coverage.
    ///
    /// Inputs:
    /// - A source expression using `quote unquote(value)`.
    ///
    /// Output:
    /// - Test passes when parsing preserves `Expr::Quote(Expr::Unquote(_))`.
    ///
    /// Transformation:
    /// - Parses one expression through the recursive-descent parser and checks
    ///   the exact nested keyword-expression AST shape.
    #[test]
    fn formal_quote_unquote_exprs_parse_as_keyword_expressions() {
        let expr = parse_terlan_expr("quote unquote(value)").expect("parse quote/unquote");

        let Expr::Quote(inner) = expr else {
            panic!("expected quote expression");
        };
        assert!(matches!(inner.as_ref(), Expr::Unquote(_)));
    }

    /// Verifies receiver method-call suffixes parse before field suffixes.
    ///
    /// Inputs:
    /// - Expression source using `user.display_name("short")`.
    ///
    /// Output:
    /// - Test passes when the expression is a call whose callee is a field-access
    ///   expression.
    ///
    /// Transformation:
    /// - Parses the canonical method-call postfix syntax and validates the AST
    ///   shape used by later receiver-method resolution.
    #[test]
    fn formal_method_call_suffix_parses_before_field_access() {
        let expr = parse_terlan_expr(r#"user.display_name("short")"#)
            .expect("parse receiver method call suffix");
        let Expr::Call {
            callee,
            args,
            is_fun_value,
            ..
        } = expr
        else {
            panic!("expected method call expression");
        };
        assert!(!is_fun_value);
        assert_eq!(args.len(), 1);
        let Expr::FieldAccess { value, field } = callee.as_ref() else {
            panic!("expected field-access callee");
        };
        assert_eq!(field, "display_name");
        assert!(matches!(value.as_ref(), Expr::Var(name) if name == "user"));
    }

    #[test]
    fn formal_unary_expr_preserves_precedence() {
        let expr = parse_terlan_expr("not Ready == false").expect("parse unary not precedence");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected comparison expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::EqEq));
        assert!(matches!(
            left.as_ref(),
            Expr::UnaryOp {
                op: UnaryOp::Not,
                ..
            }
        ));

        let expr = parse_terlan_expr("-A * B").expect("parse unary neg precedence");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected multiply expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::Mul));
        assert!(matches!(
            left.as_ref(),
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                ..
            }
        ));
    }

    #[test]
    fn formal_remote_call_expr_parses_colon_syntax() {
        let expr = parse_terlan_expr("io_lib:format(\"~p\", []) |> inspect()")
            .expect("parse colon remote call in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        let Expr::Call {
            callee,
            remote,
            args,
            is_fun_value: _,
        } = left.as_ref()
        else {
            panic!("expected remote call expression as pipe left side");
        };
        assert_eq!(remote.as_deref(), Some("io_lib"));
        assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "format"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn formal_remote_fun_ref_is_not_source_syntax() {
        let err = parse_terlan_expr("fun math:double/1 |> inspect()")
            .expect_err("remote fun refs are not canonical source syntax");

        assert!(
            err.message.contains("unexpected tokens after expression")
                || err.message.contains("expected"),
            "unexpected diagnostic: {}",
            err.message
        );
    }

    #[test]
    fn formal_macro_expr_parses_as_primary_expr() {
        let expr = parse_terlan_expr("?MODULE |> inspect()").expect("parse macro expr in pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        assert!(matches!(
            left.as_ref(),
            Expr::MacroCall { name, args } if name == "MODULE" && args.is_empty()
        ));

        let expr = parse_terlan_expr("?assert_equal(A, B)").expect("parse macro call expr");
        assert!(matches!(
            expr,
            Expr::MacroCall { name, args } if name == "assert_equal" && args.len() == 2
        ));
    }

    #[test]
    fn formal_raw_macro_expr_requires_immediate_raw_block() {
        let expr = parse_terlan_expr("sql{select * from users} |> inspect()")
            .expect("parse raw macro expr in pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
        assert!(matches!(
            left.as_ref(),
            Expr::RawMacro { name, raw } if name == "sql" && raw == "select * from users"
        ));

        let spaced = parse_terlan_expr("sql {select * from users}");
        assert!(
            spaced.is_err(),
            "spaced raw macro should not parse as expression"
        );
    }

    #[test]
    fn formal_constructor_chain_expr_parses_with_record_expr() {
        let expr = parse_terlan_expr("User(id, name) with Admin { id = id, name = name }")
            .expect("parse constructor chain expr");

        let Expr::ConstructorChain { base, record } = expr else {
            panic!("expected constructor chain expression");
        };
        assert!(matches!(
            base.as_ref(),
            Expr::Call {
                remote: None,
                args,
                ..
            } if args.len() == 2
        ));
        assert!(matches!(
            record.as_ref(),
            Expr::RecordConstruct { name, fields } if name == "Admin" && fields.len() == 2
        ));
    }

    #[test]
    fn formal_nullary_constructor_pattern_call_is_rejected() {
        let err = parse_module(
            r#"
            module bad_constructor_pattern.

            value(Option: Option): Int ->
                case Option {
                    None() -> 0
                }.
            "#,
        )
        .expect_err("reject nullary constructor pattern call");
        assert_eq!(
            err.message,
            "constructor patterns require at least one argument"
        );
    }

    /// Verifies canonical callable constraint-list parsing.
    ///
    /// Inputs:
    /// - A module containing a generic function with `[Eq[A], Show[A]]` after
    ///   its parameter list.
    ///
    /// Output:
    /// - Parsed function declaration with preserved generic-bound strings.
    ///
    /// Transformation:
    /// - Exercises the canonical EBNF constraint-list position and confirms
    ///   constraints are kept for typechecker lowering.
    #[test]
    fn parses_function_declaration_with_constraint_list() {
        let source = r#"
module bounds_demo.

pub debug[A](X: A, Y: A)[Eq[A], Show[A]]: Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> <<"neq">>
    }.
"#;

        let module = parse_module(source).expect("parse constraint-list function");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };
        assert_eq!(function.name, "debug");
        assert_eq!(function.params.len(), 2);
        assert_eq!(
            function.generic_bounds,
            vec!["Eq[A]".to_string(), "Show[A]".to_string()]
        );
    }

    /// Verifies canonical constraint lists on non-function callable forms.
    ///
    /// Inputs:
    /// - A module containing a trait method, receiver method, and explicit impl
    ///   method with post-parameter constraint lists.
    ///
    /// Output:
    /// - Parsed declarations whose `generic_bounds` preserve each constraint
    ///   as type-reference text.
    ///
    /// Transformation:
    /// - Exercises all callable parser paths that share the canonical
    ///   `[TraitRef]` constraint-list syntax.
    #[test]
    fn parses_method_trait_method_and_impl_method_constraint_lists() {
        let source = r#"
module bounds_surfaces.

struct User {
    name: String
}.

pub trait Show[T] {
    show[A](value: A)[Eq[A]]: String.
}.

pub (user: User) label[A](value: A)[Show[A]]: String ->
    Show.show(value).

pub impl Show[User] for User {
    show[A](value: A)[Eq[A]]: String ->
        "user".
}.
"#;

        let module = parse_module(source).expect("parse constraint-list surfaces");

        let trait_decl = match &module.declarations[1] {
            Decl::Trait(trait_decl) => trait_decl,
            _ => panic!("expected trait declaration"),
        };
        assert_eq!(
            trait_decl.methods[0].generic_bounds,
            vec!["Eq[A]".to_string()]
        );

        let method_decl = match &module.declarations[2] {
            Decl::Method(method_decl) => method_decl,
            _ => panic!("expected method declaration"),
        };
        assert_eq!(method_decl.generic_bounds, vec!["Show[A]".to_string()]);

        let impl_decl = match &module.declarations[3] {
            Decl::TraitImpl(impl_decl) => impl_decl,
            _ => panic!("expected trait impl declaration"),
        };
        assert_eq!(
            impl_decl.methods[0].generic_bounds,
            vec!["Eq[A]".to_string()]
        );
    }

    #[test]
    fn parses_module_and_item_doc_comments() {
        let source = r#"
//! Math helpers.
//! Second module line.

module mathx.

/// Adds one.
/// Second function line.
pub add(X: Int): Int ->
    X + 1.

/// Optional value.
pub type Option[T] =
      none
    | {some, T}.
"#;

        let module = parse_module(source).expect("parse docs");
        assert_eq!(module.docs, vec!["Math helpers.", "Second module line."]);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(function.docs, vec!["Adds one.", "Second function line."]);
            }
            _ => panic!("expected documented function"),
        }
        match &module.declarations[1] {
            Decl::Type(type_decl) => {
                assert_eq!(type_decl.docs, vec!["Optional value."]);
            }
            _ => panic!("expected documented type"),
        }
    }

    #[test]
    fn parses_module_and_item_doc_block_comments() {
        let source = r#"
/**
 * Math helpers.
 *
 * @module mathx
 */
module mathx.

/**
 * Adds one.
 *
 * @param x The value to increment.
 * @returns The incremented value.
 */
@test
pub add(x: Int): Int ->
    x + 1.

/**
 * Optional value.
 *
 * @type T The wrapped value type.
 */
pub type Option[T] =
      none
    | {some, T}.
"#;

        let module = parse_module(source).expect("parse block docs");
        assert_eq!(module.docs, vec!["Math helpers.\n\n@module mathx"]);
        assert_eq!(module.declaration_annotations[0][0].path, vec!["test"]);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(
                    function.docs,
                    vec![
                        "Adds one.\n\n@param x The value to increment.\n@returns The incremented value."
                    ]
                );
            }
            _ => panic!("expected documented function"),
        }
        match &module.declarations[1] {
            Decl::Type(type_decl) => {
                assert_eq!(
                    type_decl.docs,
                    vec!["Optional value.\n\n@type T The wrapped value type."]
                );
            }
            _ => panic!("expected documented type"),
        }
    }

    #[test]
    fn parses_public_constructor_with_varargs_and_defaults() {
        let source = r#"
module queue.

/// Builds queues.
pub constructor Queue[T] {
    (): Queue[T] ->
        empty();

    (Items: List[T]): Queue[T] ->
        from_list(Items);

    (...Items: T): Queue[T] ->
        from_list(Items)
}.

pub constructor Range {
    (Start: Int, End: Int, Step: Int = 1): Range ->
        make(Start, End, Step)
}.
"#;

        let module = parse_module(source).expect("parse constructors");
        match &module.declarations[0] {
            Decl::Constructor(constructor) => {
                assert!(constructor.is_public);
                assert_eq!(constructor.docs, vec!["Builds queues."]);
                assert_eq!(constructor.name, "Queue");
                assert_eq!(constructor.params, vec!["T"]);
                assert_eq!(constructor.clauses.len(), 3);
                assert!(constructor.clauses[2].params[0].is_varargs);
            }
            _ => panic!("expected queue constructor"),
        }
        match &module.declarations[1] {
            Decl::Constructor(constructor) => {
                let step = &constructor.clauses[0].params[2];
                assert_eq!(step.name, "Step");
                assert!(step.default.is_some());
            }
            _ => panic!("expected range constructor"),
        }
    }

    #[test]
    fn rejects_constructor_varargs_before_other_params() {
        let source = r#"
module bad.

pub constructor Queue[T] {
    (...Items: T, Last: T): Queue[T] ->
        from_list(Items)
}.
"#;

        let err = parse_module(source).expect_err("invalid varargs");
        assert_eq!(err.message, "constructor varargs parameter must be last");
    }

    #[test]
    fn rejects_ambiguous_constructor_clause_shapes() {
        let duplicate_exact = r#"
module bad.

pub constructor Pair {
    (A: Int): Pair ->
        make(A);

    (B: Binary): Pair ->
        make(B)
}.
"#;

        let err = parse_module(duplicate_exact).expect_err("ambiguous exact arity");
        assert_eq!(err.message, "constructor has ambiguous arity clauses");

        let overlapping_defaults = r#"
module bad.

pub constructor Range {
    (Start: Int, End: Int = 10): Range ->
        make(Start, End);

    (Start: Int): Range ->
        make(Start, 10)
}.
"#;

        let err = parse_module(overlapping_defaults).expect_err("ambiguous default arity");
        assert_eq!(err.message, "constructor has ambiguous arity clauses");

        let duplicate_varargs = r#"
module bad.

pub constructor Items[T] {
    (...Items: T): Items[T] ->
        Items;

    (First: T, ...Rest: T): Items[T] ->
        Rest
}.
"#;

        let err = parse_module(duplicate_varargs).expect_err("ambiguous varargs");
        assert_eq!(err.message, "constructor has ambiguous varargs clauses");
    }

    #[test]
    fn rejects_misplaced_module_doc_comments() {
        let source = r#"
module misplaced_docs.

//! Late module docs.
pub id(X: Int): Int ->
    X.
"#;

        let err = parse_module(source).expect_err("reject misplaced module docs");
        assert_eq!(
            err.message,
            "module doc comments (`//!`) must appear before the module declaration"
        );

        let interface_source = r#"
module misplaced_interface_docs.

//! Late module docs.
pub id(X: Int): Int.
"#;

        let interface_err =
            parse_interface_module(interface_source).expect_err("reject misplaced interface docs");
        assert_eq!(
            interface_err.message,
            "module doc comments (`//!`) must appear before the module declaration"
        );
    }

    #[test]
    fn rejects_misplaced_module_doc_blocks() {
        let source = r#"
module misplaced_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_doc_block
 */
pub id(x: Int): Int ->
    x.
"#;

        let err = parse_module(source).expect_err("reject misplaced module doc block");
        assert_eq!(
            err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );

        let interface_source = r#"
module misplaced_interface_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_interface_doc_block
 */
pub id(x: Int): Int.
"#;

        let interface_err = parse_interface_module(interface_source)
            .expect_err("reject misplaced interface doc block");
        assert_eq!(
            interface_err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );
    }

    #[test]
    fn parses_struct_field_doc_comments() {
        let source = r#"
module users.

/// A user account.
pub struct User {
    /// Stable internal ID.
    id: Int,

    /// Display name.
    name: Text
}.
"#;

        let module = parse_module(source).expect("parse struct docs");
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert_eq!(struct_decl.docs, vec!["A user account."]);
                assert_eq!(struct_decl.fields[0].docs, vec!["Stable internal ID."]);
                assert_eq!(struct_decl.fields[1].docs, vec!["Display name."]);
            }
            _ => panic!("expected documented struct"),
        }
    }

    #[test]
    fn parses_public_macro_declaration() {
        let source = r#"
module mathx.

pub macro unless(X: Expr, Y: Expr): Expr ->
    quote X.
"#;

        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_module(source).expect("parse");
        assert_eq!(module.name, "mathx");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Function(function) => assert!(function.is_macro),
            _ => panic!("expected function declaration"),
        }
    }

    #[test]
    fn parses_public_trait_as_decl() {
        let source = r#"
module trait_demo.

/// Show trait docs.
pub trait Show[A] {
    show(Value: A): Text.
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert!(trait_decl.is_public);
                assert_eq!(trait_decl.name, "Show");
                assert_eq!(trait_decl.params[0], "A");
                assert_eq!(trait_decl.docs, vec!["Show trait docs."]);
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_raw_block_declaration_without_trailing_dot() {
        let source = r#"
module native_meta.

target erlang with safe_native.

native core module ArrayNative {
    #[nif(normal)]
    length[T](A: Array[T]): Int.
}
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 2);
        match &module.declarations[1] {
            Decl::Raw(raw) => {
                assert_eq!(raw.kind, "native");
                assert!(raw.text.contains("ArrayNative"));
            }
            _ => panic!("expected raw native declaration"),
        }
    }

    #[test]
    fn parses_public_struct_declaration() {
        let source = r#"
module users.

pub struct User {
    id: Int,
    name: Text,
    email: Text = :none
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert!(struct_decl.is_public);
                assert_eq!(struct_decl.name, "User");
                assert_eq!(struct_decl.fields.len(), 3);
                assert_eq!(struct_decl.fields[0].name, "id");
                assert_eq!(struct_decl.fields[1].name, "name");
                assert_eq!(struct_decl.fields[2].name, "email");
                match &struct_decl.fields[2].default {
                    Some(default) => match default {
                        Expr::Atom(atom) => assert_eq!(atom, "none"),
                        _ => panic!("expected atom default expression"),
                    },
                    None => panic!("expected default expression"),
                }
            }
            _ => panic!("expected struct declaration"),
        }
    }

    #[test]
    fn parses_trait_as_trait_decl() {
        let source = r#"
module traits.

pub trait Show {}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.name, "Show");
                assert!(trait_decl.params.is_empty());
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_trait_decl_extends() {
        let source = r#"
module traits.

pub trait Monoid[A] extends Semigroup[A], Eq[A] {
    combine(X: A, Y: A): A.
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.name, "Monoid");
                assert_eq!(trait_decl.params, vec!["A"]);
                assert_eq!(trait_decl.super_traits, vec!["Semigroup[A]", "Eq[A]"]);
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_function_declaration_with_angle_generic_bounds() {
        let source = r#"
module bounds_demo.

pub debug<A: Eq + Show>(X: A, Y: A): Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> <<"neq">>
    }.
"#;

        let module = parse_module(source).expect("parse generic bounds function");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };
        assert_eq!(function.name, "debug");
        assert_eq!(function.params.len(), 2);
        assert_eq!(function.params[0].annotation.text, "A");
        assert_eq!(function.params[1].annotation.text, "A");
    }

    #[test]
    fn parses_trait_method_with_angle_generic_bounds() {
        let source = r#"
module bounds_trait.

pub trait Logger[A] {
    debug<A: Eq + Show>(Value: A): Text.
}.
"#;

        let module = parse_module(source).expect("parse trait method bounds");
        let trait_decl = match &module.declarations[0] {
            Decl::Trait(trait_decl) => trait_decl,
            _ => panic!("expected trait declaration"),
        };
        let method = &trait_decl.methods[0];
        assert_eq!(method.name, "debug");
        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].annotation.text, "A");
    }

    #[test]
    fn parses_quote_and_unquote_expressions() {
        let source = r#"
module sym.

pub macro expand(C: Ast, X: Expr): Expr ->
    quote unquote(X).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let expr = &function.clauses[0].body;
        match expr {
            Expr::Quote(inner) => match inner.as_ref() {
                Expr::Unquote(_) => {}
                _ => panic!("expected unquote inside quote"),
            },
            _ => panic!("expected quoted expression"),
        }
    }

    #[test]
    fn parses_html_block_expressions() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <main><h1>Hello</h1></main>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => {
                assert_eq!(html.macro_kind, BuiltinBlockMacro::Html);
                assert_eq!(html.nodes.len(), 1);
                match &html.nodes[0] {
                    HtmlNode::Element(element) => {
                        assert_eq!(element.name, "main");
                        assert_eq!(element.children.len(), 1);
                        match &element.children[0] {
                            HtmlNode::Element(child) => {
                                assert_eq!(child.name, "h1");
                                match &child.children[0] {
                                    HtmlNode::Text(text) => assert_eq!(text, "Hello"),
                                    _ => panic!("expected child text"),
                                }
                            }
                            _ => panic!("expected nested h1 element"),
                        }
                    }
                    _ => panic!("expected main element"),
                }
            }
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_named_slot_children() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <page-shell title="Markdown">
            @view1 {
                <welcome-content></welcome-content>
            }
        </page-shell>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.name, "page-shell");
                    assert_eq!(element.children.len(), 1);
                    match &element.children[0] {
                        HtmlNode::NamedSlot(slot) => {
                            assert_eq!(slot.name, "view1");
                            assert_eq!(slot.children.len(), 1);
                            match &slot.children[0] {
                                HtmlNode::Element(child) => {
                                    assert_eq!(child.name, "welcome-content")
                                }
                                _ => panic!("expected slot child element"),
                            }
                        }
                        _ => panic!("expected named slot child"),
                    }
                }
                _ => panic!("expected page-shell element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_attributes() {
        let source = r#"
module views.

pub view(Primary: Text, Enabled: Bool): Html[none] ->
    html {
        <button class="primary" disabled={true} id='save'>Save</button>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.attrs.len(), 3);
                    assert_eq!(element.name, "button");

                    let crate::ast::HtmlAttr { name, value } = &element.attrs[0];
                    assert_eq!(name, "class");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Text(value) => assert_eq!(value, "primary"),
                        _ => panic!("expected text value"),
                    }

                    let crate::ast::HtmlAttr { name, value } = &element.attrs[1];
                    assert_eq!(name, "disabled");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Expr(Expr::Var(name)) => assert_eq!(name, "true"),
                        _ => panic!("expected expression value"),
                    }

                    let crate::ast::HtmlAttr { name, value } = &element.attrs[2];
                    assert_eq!(name, "id");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Text(value) => assert_eq!(value, "save"),
                        _ => panic!("expected text value"),
                    }
                }
                _ => panic!("expected button element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_interpolation_nodes() {
        let source = r#"
module views.

pub view(Title: Text): Html[none] ->
    html {
        <h1>{Title}</h1>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.children.len(), 1);
                    match &element.children[0] {
                        HtmlNode::Expr(Expr::Var(name)) => assert_eq!(name, "Title"),
                        _ => panic!("expected interpolated expression"),
                    }
                }
                _ => panic!("expected h1 element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_case_branch_nodes_in_interpolation() {
        let source = r#"
module views.

pub view(Admin: Bool): Html[none] ->
    html {
        <div>
            {case Admin {
                true -> <span class="admin">Admin</span>;
                false -> <span>Viewer</span>
            }}
        </div>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let div = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected div element"),
            },
            _ => panic!("expected html block"),
        };
        match &div.children[0] {
            HtmlNode::Expr(Expr::Case { clauses, .. }) => {
                assert_eq!(clauses.len(), 2);
                match &clauses[0].body {
                    Expr::HtmlBlock(html) => match &html.nodes[0] {
                        HtmlNode::Element(element) => assert_eq!(element.name, "span"),
                        _ => panic!("expected span element"),
                    },
                    _ => panic!("expected html branch body"),
                }
            }
            _ => panic!("expected case interpolation"),
        }
    }

    #[test]
    fn parses_html_for_nodes_in_interpolation() {
        let source = r#"
module views.

pub view(Users: List[Text]): Html[none] ->
    html {
        <ul>
            {for User <- Users {
                <li>{User}</li>
            }}
        </ul>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let ul = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected ul element"),
            },
            _ => panic!("expected html block"),
        };
        match &ul.children[0] {
            HtmlNode::Expr(Expr::ListComprehension { expr, .. }) => match expr.as_ref() {
                Expr::HtmlBlock(html) => match &html.nodes[0] {
                    HtmlNode::Element(element) => assert_eq!(element.name, "li"),
                    _ => panic!("expected li element"),
                },
                _ => panic!("expected html list item"),
            },
            _ => panic!("expected list rendering interpolation"),
        }
    }

    #[test]
    fn parses_nested_html_elements() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <section>
            <article><p>Nested</p></article>
        </section>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let element = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected section element"),
            },
            _ => panic!("expected html block"),
        };
        assert_eq!(element.name, "section");
        assert_eq!(element.children.len(), 1);
        let article = match &element.children[0] {
            HtmlNode::Element(element) => element,
            _ => panic!("expected article element"),
        };
        assert_eq!(article.name, "article");
        assert_eq!(article.children.len(), 1);
    }

    #[test]
    fn parses_typed_fun_parameters() {
        let source = r#"
module callbackx.

pub run(X: Int): Int ->
    apply((N: Int) -> N + 1, X).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::Call { args, .. } => match &args[0] {
                Expr::Fun { clauses } => assert_eq!(clauses[0].patterns.len(), 1),
                _ => panic!("expected fun argument"),
            },
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn parses_constructor_style_patterns() {
        let source = r#"
module syntax.

pub simplify(E: Expr): Expr ->
    case E {
        Call(:atom, [x, y]) ->
            call(x, y);
        _ ->
            E
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        let case_clauses = match expr {
            Expr::Case { clauses, .. } => clauses,
            _ => panic!("expected case"),
        };
        let first = &case_clauses[0].pattern;
        match first {
            crate::ast::Pattern::Tuple(items) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    crate::ast::Pattern::Atom(name) => assert_eq!(name, "Call"),
                    _ => panic!("expected constructor atom"),
                }
                match &items[1] {
                    crate::ast::Pattern::Atom(name) => assert_eq!(name, "atom"),
                    _ => panic!("expected raw atom argument"),
                }
            }
            _ => panic!("expected tuple pattern"),
        }
    }

    #[test]
    fn parses_remote_call_expression() {
        let source = r#"
module remote.

pub add(): Int ->
    io_lib:format("~p", []).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::Call {
                remote: Some(module),
                ..
            } => assert_eq!(module, "io_lib"),
            _ => panic!("expected remote call"),
        }
    }

    /// Verifies explicit trait-target method calls parse as remote calls.
    ///
    /// Inputs:
    /// - A module using `Parse[Int].from_string("42")`.
    ///
    /// Output:
    /// - Test passes when the call is preserved with `Parse[Int]` as the
    ///   remote qualifier and `from_string` as the method name.
    ///
    /// Transformation:
    /// - Parses bracketed type arguments in expression qualifier position
    ///   without introducing general postfix generic call syntax.
    #[test]
    fn parses_explicit_trait_target_call_expression() {
        let source = r#"
module traits.parse_target.

pub parse(): Option[Int] ->
    Parse[Int].from_string("42").
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::Call {
                callee,
                remote: Some(module),
                ..
            } => {
                assert_eq!(module, "Parse[Int]");
                assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "from_string"));
            }
            _ => panic!("expected explicit trait-target call"),
        }
    }

    #[test]
    fn parses_struct_field_access_sugar() {
        let source = r#"
module fields.

pub name(User: User): Text ->
    User.name.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::FieldAccess { value, field } => {
                assert_eq!(field, "name");
                match value.as_ref() {
                    Expr::Var(name) => assert_eq!(name, "User"),
                    _ => panic!("expected field receiver"),
                }
            }
            _ => panic!("expected field access"),
        }
    }

    #[test]
    fn parses_template_instantiation_expr() {
        let source = r#"
module template_instantiation.

pub view(Title: Text, User: User): Html[none] ->
    Page{ title = Title, user = User }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::TemplateInstantiate { name, fields } => {
                assert_eq!(name, "Page");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].key, "title");
                assert!(matches!(fields[0].value.as_ref(), Expr::Var(name) if name == "Title"));
                assert_eq!(fields[1].key, "user");
                assert!(matches!(fields[1].value.as_ref(), Expr::Var(name) if name == "User"));
            }
            _ => panic!("expected template instantiation"),
        }
    }

    #[test]
    fn parses_eqeq_and_divrem_operators() {
        let source = r#"
module ops.

pub add(X: Int, Y: Int): Int ->
    X == Y + X div Y.
"#;

        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert_eq!(format!("{:?}", op), "EqEq");
            }
            _ => panic!("expected binary op"),
        }
    }

    #[test]
    fn parses_greater_than_or_equal_operator() {
        let source = r#"
module compare.

pub non_negative(X: Int): Bool ->
    X >= 0.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert!(matches!(op, crate::ast::BinaryOp::GtEq));
            }
            _ => panic!("expected binary op"),
        }
    }

    /// Verifies that the old Kleisli composition operator is not A0 syntax.
    ///
    /// Inputs:
    /// - A module body containing the removed `>=>` operator.
    ///
    /// Output:
    /// - Test passes when parsing rejects the source.
    ///
    /// Transformation:
    /// - Exercises the recursive-descent parser after the canonical EBNF
    ///   removed `>=>` from `CmpOp`.
    #[test]
    fn rejects_kleisli_compose_operator_from_canonical_syntax() {
        let source = r#"
module kleisli_demo.

pub authenticate(): Kleisli[AuthResult, Text, User] ->
    decode_token() >=> load_user() >=> require_admin().
"#;

        parse_module(source).expect_err("kleisli composition operator should be rejected");
    }

    #[test]
    fn parses_pipe_forward_operator() {
        let source = r#"
module pipe_demo.

pub demo(X: Int): Int ->
    X |> add(1).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert!(matches!(op, crate::ast::BinaryOp::PipeForward));
            }
            _ => panic!("expected pipe forward binary op"),
        }
    }

    #[test]
    fn parses_send_operator() {
        let source = r#"
module protocol_ok.

pub inc(P: Pid[Counter]): ok ->
    P ! inc,
    ok.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => assert!(matches!(op, crate::ast::BinaryOp::Send)),
            _ => panic!("expected send expression"),
        }
    }

    #[test]
    fn parses_fixed_array_expression_syntax() {
        let source = r#"
module arrays.

pub rgb(): FixedArray[3, Int] ->
    #[255, 128, 0].
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };

        match &function.clauses[0].body {
            Expr::FixedArray(elements) => {
                assert_eq!(elements.len(), 3);
            }
            _ => panic!("expected fixed array expression"),
        }
    }

    #[test]
    fn interface_parser_accepts_macros_and_types() {
        let source = r#"
module iface.
pub macro expand(X: Expr, Y: Expr): Expr.
pub type Flag = Bool.
"#;
        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_interface_module(source).expect("parse interface");
        assert_eq!(module.declarations.len(), 2);
        assert!(matches!(&module.declarations[0], Decl::Function(_)));
        assert!(matches!(&module.declarations[1], Decl::Type(_)));
    }

    /// Verifies interface files can summarize explicit trait conformance
    /// declarations.
    ///
    /// Inputs:
    /// - A `.tli`-style module containing a trait declaration and
    ///   `pub impl TraitRef for Type` signature block.
    ///
    /// Output:
    /// - Structured trait and trait implementation declarations.
    ///
    /// Transformation:
    /// - Exercises the interface declaration router after `pub` and proves
    ///   conformance summaries preserve signatures without requiring method
    ///   bodies.
    #[test]
    fn interface_parser_preserves_pub_impl_declarations() {
        let source = r#"
module trait_iface.

pub trait Show[A] {
    show(Value: A): Text.
}.

pub impl Show[Int] for Int {
    show(Value: Int): Text.
}.
"#;

        let module = parse_interface_module(source).expect("parse interface impl");
        assert_eq!(module.declarations.len(), 2);
        assert!(matches!(&module.declarations[0], Decl::Trait(_)));
        let Decl::TraitImpl(impl_decl) = &module.declarations[1] else {
            panic!("expected trait impl declaration");
        };
        assert_eq!(impl_decl.trait_ref.text, "Show[Int]");
        assert_eq!(impl_decl.for_type.text, "Int");
        assert_eq!(impl_decl.methods.len(), 1);
        assert!(impl_decl.methods[0].clauses.is_empty());
    }

    /// Verifies interface files may summarize public type headers.
    ///
    /// Inputs:
    /// - A `.tli`-style module containing `pub type ExternalUser.`.
    ///
    /// Output:
    /// - Parsed type declaration with no variants.
    ///
    /// Transformation:
    /// - Exercises interface-only type parsing so generated `.typi` files can
    ///   preserve nominal public types without requiring source-form bodies.
    #[test]
    fn interface_parser_accepts_bodyless_public_type_headers() {
        let source = r#"
module provider_iface.

pub type ExternalUser.
"#;

        let module = parse_interface_module(source).expect("parse bodyless interface type");
        assert_eq!(module.declarations.len(), 1);
        let Decl::Type(type_decl) = &module.declarations[0] else {
            panic!("expected type declaration");
        };
        assert_eq!(type_decl.name, "ExternalUser");
        assert!(type_decl.variants.is_empty());
        assert!(type_decl.is_public);
    }

    #[test]
    fn parses_dotted_imports_and_qualified_remote_calls() {
        let source = r#"
module algebra_demo.

import std.Algebra.{Semigroup, Monoid, Sum}.
import std.Collections.List.

pub total(Xs: List[Int]): Int ->
    Sum.value(std.Algebra.combine_all(List.map(Xs, (X) -> Sum(X)))).
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 3);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.module_name, "std.Algebra");
                assert_eq!(import.items.len(), 3);
            }
            _ => panic!("expected import"),
        }
        match &module.declarations[1] {
            Decl::Import(import) => {
                assert_eq!(import.module_name, "std.Collections");
                assert_eq!(import.items[0].name, "List");
            }
            _ => panic!("expected import"),
        }
    }

    #[test]
    fn parses_file_imports() {
        let source = r#"
module templates_demo.

import file "./templates/user_card.tl.html" as UserCard.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::ast::ImportKind::File);
                assert_eq!(
                    import.source_path.as_deref(),
                    Some("./templates/user_card.tl.html")
                );
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "UserCard");
            }
            _ => panic!("expected file import"),
        }
    }

    #[test]
    fn parses_css_imports() {
        let source = r#"
module styles_demo.

import css "./styles/page.css" as PageCss.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::ast::ImportKind::Css);
                assert_eq!(import.source_path.as_deref(), Some("./styles/page.css"));
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "PageCss");
            }
            _ => panic!("expected css import"),
        }
    }

    #[test]
    fn parses_markdown_imports() {
        let source = r#"
module posts_demo.

import markdown "./posts/hello.md" as HelloPost.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::ast::ImportKind::Markdown);
                assert_eq!(import.source_path.as_deref(), Some("./posts/hello.md"));
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "HelloPost");
            }
            _ => panic!("expected markdown import"),
        }
    }

    #[test]
    fn parses_static_route_declarations_as_raw_declarations() {
        let source = r#"
module site.

static route "/" ->
    home().
"#;

        let module = parse_module(source).expect("parse static route declaration");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Raw(raw) => {
                assert_eq!(raw.kind, "static");
                assert!(raw.text.contains("route"));
                assert!(raw.text.contains("/"));
                assert!(raw.text.contains("home"));
            }
            _ => panic!("expected raw static route declaration"),
        }
    }

    #[test]
    fn parses_template_declarations() {
        let source = r#"
module template_demo.

template Page from "./templates/page.tl.html" {
    title: Text,
    user: User
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Template(template) => {
                assert_eq!(template.name, "Page");
                assert_eq!(template.source_path, "./templates/page.tl.html");
                assert_eq!(template.props.len(), 2);
                assert_eq!(template.props[0].name, "title");
                assert_eq!(template.props[0].annotation.text, "Text");
                assert_eq!(template.props[1].name, "user");
                assert_eq!(template.props[1].annotation.text, "User");
            }
            _ => panic!("expected template declaration"),
        }
    }

    #[test]
    fn parses_qualified_type_names_in_function_signatures() {
        let source = r#"
module opaque_demo.

pub make(Value: Int): users_opaque_interface.UserId ->
    users_opaque_interface.user_id(Value).

pub declared(Value: users_opaque_interface.UserId): users_opaque_interface.UserId ->
    Value.
"#;

        let module = parse_module(source).expect("parse qualified type names");
        assert_eq!(module.declarations.len(), 2);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(function.return_type.text, "users_opaque_interface.UserId");
            }
            _ => panic!("expected function"),
        }
        match &module.declarations[1] {
            Decl::Function(function) => {
                assert_eq!(
                    function.params[0].annotation.text,
                    "users_opaque_interface.UserId"
                );
                assert_eq!(function.return_type.text, "users_opaque_interface.UserId");
                assert_eq!(function.clauses.len(), 1);
            }
            _ => panic!("expected function signature"),
        }
    }

    #[test]
    fn parses_hkt_type_params_and_variance_surface_syntax() {
        let source = r#"
module hkt_demo.

pub type Kleisli[F[_], -A, B] =
    kleisli(run: (A) -> F[B]).

pub example(K: Kleisli[Result[_, db_error], Text, Int]): Kleisli[DbResult, Text, Int] ->
    K.
"#;

        let module = parse_module(source).expect("parse hkt module");
        match &module.declarations[0] {
            Decl::Type(type_decl) => {
                assert_eq!(type_decl.params.len(), 3);
                assert!(type_decl.params[0].contains("F"));
                assert!(type_decl.params[0].contains("_"));
                assert!(type_decl.params[1].contains("-"));
            }
            _ => panic!("expected type declaration"),
        }
    }

    #[test]
    fn parses_terli_style_interface_with_pub_signatures() {
        let source = r#"
module cache_contract.

pub type Cache = Int.

pub get(Cache: Cache, Key: Binary): Result[Binary, not_found].
pub put(Cache: Cache, Key: Binary, Value: Binary): ok.
"#;

        let module = parse_interface_module(source).expect("parse interface");
        assert_eq!(module.declarations.len(), 3);
        assert!(matches!(
            &module.declarations[0],
            Decl::Struct(_) | Decl::Type(_)
        ));
        assert!(matches!(&module.declarations[1], Decl::Function(_)));
        assert!(matches!(&module.declarations[2], Decl::Function(_)));

        if let Decl::Type(cache_type) = &module.declarations[0] {
            assert!(cache_type.is_public);
            assert_eq!(cache_type.name, "Cache");
        } else {
            panic!("expected type declaration");
        }
    }

    #[test]
    fn rejects_bodyless_let_expression() {
        let source = r#"
module let_requires_result.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price; total = subtotal + tax.
"#;

        let error = parse_module(source).expect_err("bodyless let should fail");
        assert!(
            error
                .message
                .contains("let expression requires an explicit result expression"),
            "unexpected diagnostic: {:?}",
            error
        );
    }
}
