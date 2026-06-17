use super::*;

impl Parser {
    /// Parses a struct declaration.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `struct`.
    /// - Parser cursor positioned at the `struct` keyword.
    ///
    /// Output:
    /// - A structured `StructDecl` with fields, derives, implements clauses,
    ///   visibility, and source span.
    ///
    /// Transformation:
    /// - Consumes the struct header, optional derivation and conformance
    ///   clauses, field declarations with optional defaults, and the required
    ///   declaration terminator.
    pub(super) fn parse_struct_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
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
    /// Parses a trait declaration.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `trait`.
    /// - Parser cursor positioned at the `trait` keyword.
    ///
    /// Output:
    /// - A structured `TraitDecl` with type parameters, super-traits, method
    ///   signatures/defaults, visibility, and source span.
    ///
    /// Transformation:
    /// - Consumes the trait header and body, preserving method docs and
    ///   optional default method bodies for later conformance checking.
    pub(super) fn parse_trait_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
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
    pub(super) fn parse_trait_impl_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
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
    ///   method entries as signatures so `.terli` files can summarize
    ///   conformances without bodies.
    pub(super) fn parse_trait_impl_interface_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
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
    /// Parses one trait method entry.
    ///
    /// Inputs:
    /// - `docs`: documentation already consumed before the method entry.
    /// - Parser cursor positioned at the lower-case trait method name.
    ///
    /// Output:
    /// - A `TraitMethodDecl` with parameters, return type, generic bounds,
    ///   optional default body, docs, and source span.
    ///
    /// Transformation:
    /// - Consumes a trait method signature and optional `->` default body,
    ///   normalizing it into the trait-method parse tree shape.
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
    /// Parses a source type declaration.
    ///
    /// Inputs:
    /// - `is_opaque`: whether the caller matched `opaque type`.
    /// - `is_public`: whether `pub` was consumed before the declaration.
    ///
    /// Output:
    /// - A `TypeDecl` whose body is required for non-opaque source type
    ///   declarations.
    ///
    /// Transformation:
    /// - Delegates to the shared type declaration parser with source-mode body
    ///   requirements.
    pub(super) fn parse_type_decl(
        &mut self,
        is_opaque: bool,
        is_public: bool,
    ) -> ParseResult<Decl> {
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
    pub(super) fn parse_type_interface_decl(
        &mut self,
        is_opaque: bool,
        is_public: bool,
    ) -> ParseResult<Decl> {
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
    /// Parses one type parameter text.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the start of a type parameter.
    ///
    /// Output:
    /// - Preserved type parameter text.
    ///
    /// Transformation:
    /// - Reuses type-expression parsing until the next generic-parameter
    ///   separator so declaration and callable generic syntax share one parser.
    pub(super) fn parse_type_param_text(&mut self) -> ParseResult<String> {
        let ty = self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?;
        Ok(ty.text)
    }
}
