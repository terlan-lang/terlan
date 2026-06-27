use super::*;

impl Parser {
    /// Parses a canonical Terlan source module.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the beginning of a token stream after
    ///   lexical analysis.
    ///
    /// Output:
    /// - A parse-tree `Module` containing module docs, declaration annotations,
    ///   declarations, and source span metadata.
    ///
    /// Transformation:
    /// - Consumes leading module docs, validates the module declaration, then
    ///   dispatches each body item to the declaration parser that owns its
    ///   syntax class. Public clause groups are buffered so repeated function
    ///   clauses with the same name are merged into one function declaration.
    pub(super) fn parse_module(&mut self) -> ParseResult<Module> {
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
                TokenKind::Atom if self.current().text == "annotation" => {
                    declarations.push(attach_docs(self.parse_annotation_schema_decl(false)?, docs));
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

    /// Parses a canonical Terlan interface module.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the beginning of an interface token
    ///   stream after lexical analysis.
    ///
    /// Output:
    /// - A parse-tree `Module` whose declarations are restricted to interface
    ///   summary forms.
    ///
    /// Transformation:
    /// - Consumes leading module docs, validates the module declaration, then
    ///   dispatches each interface item to signature/type/interface-specific
    ///   declaration parsers without accepting implementation bodies.
    pub(super) fn parse_interface_module(&mut self) -> ParseResult<Module> {
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
                TokenKind::Atom if self.current().text == "annotation" => {
                    declarations.push(attach_docs(self.parse_annotation_schema_decl(false)?, docs));
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
}
