use super::*;
use crate::terlan_syntax::type_name_to_atom_payload;

/// Rejects annotations that are not valid on trait method declarations.
fn validate_trait_method_annotations(annotations: &[Annotation]) -> ParseResult<bool> {
    let mut pure = None;
    for annotation in annotations {
        let is_pure = annotation.path.len() == 1
            && annotation
                .path
                .first()
                .is_some_and(|segment| segment == "pure");
        if !is_pure {
            return Err(ParseError {
                message: format!(
                    "annotation @{} is not supported on trait methods",
                    annotation.path.join(".")
                ),
                span: annotation.span,
            });
        }
        if annotation.args.is_some()
            || !annotation.entries.is_empty()
            || !annotation.values.is_empty()
        {
            return Err(ParseError {
                message: "@pure does not accept metadata".to_string(),
                span: annotation.span,
            });
        }
        if pure.replace(annotation.span).is_some() {
            return Err(ParseError {
                message: "duplicate @pure trait method annotation".to_string(),
                span: annotation.span,
            });
        }
    }
    Ok(pure.is_some())
}

impl Parser {
    /// Parses a struct declaration.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `struct`.
    /// - Parser cursor positioned at the `struct` keyword.
    ///
    /// Output:
    /// - A structured `StructDecl` with fields, includes, implements clauses,
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
        let generic_params = self.consume_generic_params_if_present()?;
        let mut includes = Vec::new();
        if self.consume_if(TokenKind::Includes) {
            loop {
                includes.push(
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
                let field_key =
                    self.parse_record_field_key("expected lower-case struct field name")?;
                self.expect(TokenKind::Colon)?;
                let annotation = self.parse_type_expr(&[
                    TokenKind::Comma,
                    TokenKind::RBrace,
                    TokenKind::Equals,
                    TokenKind::FatArrow,
                ])?;
                if self.check(TokenKind::FatArrow) {
                    return Err(ParseError {
                        message: "`=>` implication constraints are not valid on struct fields; put the constraint in the owning generic parameter list".to_string(),
                        span: self.current().span(),
                    });
                }
                let default = if self.consume_if(TokenKind::Equals) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                fields.push(StructFieldDecl {
                    name: field_key.name,
                    annotation,
                    default,
                    is_private: field_key.is_private,
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
            generic_params,
            includes,
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
        let mut constants = Vec::new();
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
            let annotations = self.parse_leading_annotations()?;
            self.skip_comments();
            if self.check(TokenKind::RBrace) {
                break;
            }

            if self.check(TokenKind::Const) {
                if !annotations.is_empty() {
                    return Err(ParseError {
                        message: "annotations are not supported on trait associated constants"
                            .to_string(),
                        span: annotations[0].span,
                    });
                }
                let mut constant = self.parse_associated_const()?;
                constant.docs = docs;
                constants.push(constant);
                self.consume_if(TokenKind::Semicolon);
                continue;
            }

            let is_pure = validate_trait_method_annotations(&annotations)?;
            let method = self.parse_trait_method(docs, is_pure)?;
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
            constants,
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
        if self.current().text == "not" {
            self.bump();
            let trait_ref = self.parse_type_expr(&[TokenKind::LBracket])?;
            self.expect(TokenKind::LBracket)?;
            let for_type = self.parse_type_expr(&[TokenKind::RBracket])?;
            self.expect(TokenKind::RBracket)?;
            if self.check(TokenKind::LBrace) {
                return Err(ParseError {
                    message: "negative trait impl declarations cannot have a body".to_string(),
                    span: self.current().span(),
                });
            }
            self.expect(TokenKind::Dot)?;
            return Ok(Decl::TraitImpl(TraitImplDecl {
                trait_ref,
                generic_params: Vec::new(),
                for_type,
                methods: Vec::new(),
                constants: Vec::new(),
                is_negative: true,
                is_public,
                docs: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }
        let (trait_ref, generic_params) = self.parse_trait_impl_ref()?;
        if self.check(TokenKind::FatArrow) || self.check(TokenKind::Arrow) {
            return Err(ParseError {
                message:
                    "Contract impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for now"
                        .to_string(),
                span: self.current().span(),
            });
        }
        self.expect_keyword(TokenKind::For)?;
        let for_type = self.parse_type_expr(&[TokenKind::LBrace])?;
        self.expect(TokenKind::LBrace)?;

        let mut methods = Vec::new();
        let mut constants = Vec::new();
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

            if self.check(TokenKind::Const) {
                constants.push(self.parse_impl_const()?);
                self.consume_if(TokenKind::Semicolon);
                continue;
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
            generic_params,
            for_type,
            methods,
            constants,
            is_negative: false,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a positive impl trait reference and separates implication binders.
    fn parse_trait_impl_ref(&mut self) -> ParseResult<(TypeExpr, Vec<String>)> {
        let start = self.current().start;
        let base = self.parse_type_expr(&[
            TokenKind::LBracket,
            TokenKind::For,
            TokenKind::FatArrow,
            TokenKind::Arrow,
        ])?;
        if !self.consume_if(TokenKind::LBracket) {
            return Ok((base, Vec::new()));
        }

        let mut args = Vec::new();
        let mut generic_params = Vec::new();
        loop {
            if self.trait_impl_arg_has_implication() {
                let param = self.parse_type_param_text()?;
                let subject = param
                    .split_once("=>")
                    .map(|(subject, _)| subject.trim().to_string())
                    .ok_or_else(|| ParseError {
                        message: "expected structural implication in impl type parameter"
                            .to_string(),
                        span: self.current().span(),
                    })?;
                args.push(subject);
                generic_params.push(param);
            } else {
                args.push(
                    self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                        .text,
                );
            }

            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBracket)?;

        Ok((
            TypeExpr {
                text: format!("{}[{}]", base.text, args.join(", ")),
                span: Span::new(start, self.previous().end),
            },
            generic_params,
        ))
    }

    /// Reports whether the current impl type argument owns a top-level `=>`.
    fn trait_impl_arg_has_implication(&self) -> bool {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;

        for token in self.tokens.iter().skip(self.pos) {
            let at_top_level = paren_depth == 0 && bracket_depth == 0 && brace_depth == 0;
            if at_top_level {
                match token.kind {
                    TokenKind::FatArrow => return true,
                    TokenKind::Comma | TokenKind::RBracket | TokenKind::EOF => return false,
                    _ => {}
                }
            }

            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace => brace_depth = brace_depth.saturating_sub(1),
                _ => {}
            }
        }

        false
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
    fn parse_trait_method(
        &mut self,
        docs: Vec<String>,
        is_pure: bool,
    ) -> ParseResult<TraitMethodDecl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case trait method name")?;
        let generic_params = self.consume_generic_params_if_present()?;
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
        self.validate_param_defaults_trailing(&params)?;
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
            generic_params,
            params,
            return_type,
            generic_bounds,
            default_body,
            is_pure,
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
    ///   requirements and singleton atom shorthand enabled for non-opaque
    ///   aliases such as `pub type Ready.`.
    pub(super) fn parse_type_decl(
        &mut self,
        is_opaque: bool,
        is_public: bool,
    ) -> ParseResult<Decl> {
        self.parse_type_decl_with_body_requirement(is_opaque, is_public, !is_opaque, true)
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
        self.parse_type_decl_with_body_requirement(is_opaque, is_public, false, false)
    }
    /// Parses a type declaration with caller-selected body strictness.
    ///
    /// Inputs:
    /// - `is_opaque`: whether the declaration starts with `opaque type`.
    /// - `is_public`: declaration-site visibility.
    /// - `body_required`: whether missing `=` is an error.
    /// - `allow_atom_shorthand`: whether missing `=` on a source alias creates
    ///   an `Atom["..."]` singleton body.
    ///
    /// Output:
    /// - A structured `TypeDecl`.
    ///
    /// Transformation:
    /// - Consumes the type header, optional implements clause, optional union
    ///   body, and terminating `.`, while keeping source-mode shorthand and
    ///   interface-mode summary declarations explicit at the call site.
    fn parse_type_decl_with_body_requirement(
        &mut self,
        is_opaque: bool,
        is_public: bool,
        body_required: bool,
        allow_atom_shorthand: bool,
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
        if self.consume_if(TokenKind::Colon) {
            if is_opaque {
                return Err(ParseError {
                    message: "valued unions cannot be opaque".to_string(),
                    span: self.previous().span(),
                });
            }
            let representation = self.parse_type_expr(&[TokenKind::Equals])?;
            self.expect(TokenKind::Equals)?;
            let valued_arms = self.parse_valued_union_arms()?;
            self.expect(TokenKind::Dot)?;
            return Ok(Decl::Type(TypeDecl {
                name,
                params,
                variants: Vec::new(),
                representation: Some(representation),
                valued_arms,
                implements: Vec::new(),
                is_public,
                is_opaque: false,
                docs: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }
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
        } else if body_required && allow_atom_shorthand && !is_opaque && params.is_empty() {
            variants.push(TypeExpr {
                text: format!("Atom[\"{}\"]", type_name_to_atom_payload(&name)),
                span: Span::new(self.current().start, self.current().start),
            });
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
            representation: None,
            valued_arms: Vec::new(),
            implements,
            is_public,
            is_opaque,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_valued_union_arms(&mut self) -> ParseResult<Vec<ValuedUnionArmDecl>> {
        let mut arms = Vec::new();
        loop {
            let start = self.current().start;
            let name_token = self.current().clone();
            if name_token.kind != TokenKind::Var
                || !crate::terlan_syntax::parser::constants::is_screaming_snake_case(
                    &name_token.text,
                )
            {
                return Err(ParseError {
                    message: "valued-union arm name must use SCREAMING_SNAKE_CASE".to_string(),
                    span: name_token.span(),
                });
            }
            self.bump();
            self.expect(TokenKind::Equals)?;
            let value = self.parse_expr()?;
            arms.push(ValuedUnionArmDecl {
                name: name_token.text,
                value,
                span: Span::new(start, self.previous().end),
            });
            if !self.consume_if(TokenKind::Pipe) {
                break;
            }
        }
        Ok(arms)
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
}
