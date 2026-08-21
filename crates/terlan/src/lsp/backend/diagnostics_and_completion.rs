use super::super::*;

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
    pub(in super::super) fn new(client: Client) -> Self {
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
    pub(in super::super) async fn publish_document_diagnostics(
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
    pub(in super::super) fn resolver_diagnostics_for_document(
        document: &OpenDocument,
    ) -> Vec<Diagnostic> {
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
    pub(in super::super) fn type_diagnostics_for_document(
        document: &OpenDocument,
    ) -> Vec<Diagnostic> {
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
    pub(in super::super) fn template_diagnostics_for_document(
        document: &OpenDocument,
    ) -> Vec<Diagnostic> {
        document
            .template_diagnostics
            .iter()
            .map(|diagnostic| Diagnostic {
                range: diagnostic
                    .span
                    .map(|span| OpenDocument::range_from_html_span(&document.text, span))
                    .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0))),
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
    #[cfg(test)]
    pub(in super::super) fn document_symbols_for_text(text: &str) -> Vec<DocumentSymbol> {
        let Ok(module) = parse_module_as_syntax_output(text) else {
            return Vec::new();
        };
        vec![Self::module_document_symbol(text, &module)]
    }

    /// Builds symbols using the open document's extension-selected source mode.
    pub(in super::super) fn document_symbols_for_document(
        document: &OpenDocument,
    ) -> Vec<DocumentSymbol> {
        let Ok(module) = document.parse_syntax() else {
            return Vec::new();
        };
        vec![Self::module_document_symbol(&document.text, &module)]
    }

    /// Emits read-only semantic tokens for the compile-time constant surface.
    pub(in super::super) fn value_lifecycle_semantic_tokens(
        document: &OpenDocument,
    ) -> SemanticTokens {
        let mut constant_names = HashSet::new();
        if let Ok(module) = document.parse_syntax() {
            for declaration in &module.declarations {
                match &declaration.payload {
                    SyntaxDeclarationPayload::Constant { name, .. } => {
                        constant_names.insert(name.clone());
                    }
                    SyntaxDeclarationPayload::Type { valued_arms, .. } => {
                        constant_names.extend(valued_arms.iter().map(|arm| arm.name.clone()));
                    }
                    SyntaxDeclarationPayload::Trait { constants, .. } => {
                        constant_names
                            .extend(constants.iter().map(|constant| constant.name.clone()));
                    }
                    SyntaxDeclarationPayload::TraitImpl { constants, .. } => {
                        constant_names
                            .extend(constants.iter().map(|constant| constant.name.clone()));
                    }
                    SyntaxDeclarationPayload::Import { items, .. } => {
                        constant_names.extend(items.iter().filter_map(|item| {
                            let name = item.as_alias.as_ref().unwrap_or(&item.name);
                            is_semantic_constant_name(name).then(|| name.clone())
                        }));
                    }
                    _ => {}
                }
            }
        }

        let mut absolute = lex(&document.text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|token| {
                let token_type = if token.kind == TokenKind::Const {
                    0
                } else if constant_names.contains(&token.text) {
                    1
                } else {
                    return None;
                };
                let range = OpenDocument::range_from_span(
                    &document.text,
                    &Span::new(token.start, token.end),
                );
                Some((range.start, range.end, token_type))
            })
            .collect::<Vec<_>>();
        if let Some((_, index)) = Self::binding_navigation(document) {
            absolute.extend(index.all().iter().map(|occurrence| {
                let range = OpenDocument::range_from_span(&document.text, &occurrence.span);
                (range.start, range.end, 1)
            }));
        }
        absolute.sort_by_key(|(start, _, _)| (start.line, start.character));
        absolute.dedup_by_key(|(start, end, _)| (*start, *end));

        let mut previous_line = 0;
        let mut previous_start = 0;
        let data = absolute
            .into_iter()
            .map(|(start, end, token_type)| {
                let delta_line = start.line - previous_line;
                let delta_start = if delta_line == 0 {
                    start.character - previous_start
                } else {
                    start.character
                };
                previous_line = start.line;
                previous_start = start.character;
                SemanticToken {
                    delta_line,
                    delta_start,
                    length: end.character.saturating_sub(start.character),
                    token_type,
                    token_modifiers_bitset: if token_type == 1 { 1 } else { 0 },
                }
            })
            .collect();
        SemanticTokens {
            result_id: None,
            data,
        }
    }

    /// Builds completion items for source text at a cursor position.
    ///
    /// Inputs:
    /// - `uri`: document URI used to discover generated interface summaries.
    /// - `document`: current open-document snapshot.
    /// - `position`: cursor position from the editor.
    ///
    /// Output:
    /// - Completion items for local raw shapes and imported public shapes.
    ///
    /// Transformation:
    /// - Parses syntax output and generated summaries to expose the reserved
    ///   shape surface to editors without enabling semantic expansion.
    pub(in super::super) fn completion_items_for_position(
        uri: &Url,
        document: &OpenDocument,
        position: Position,
    ) -> Vec<CompletionItem> {
        if document.byte_offset_from_position(position).is_none() {
            return Vec::new();
        }
        let Ok(module) = document.parse_syntax() else {
            return Vec::new();
        };

        let byte_offset = document
            .byte_offset_from_position(position)
            .unwrap_or_default();
        if let Some(items) =
            Self::receiver_member_completion_items(uri, &module, document, byte_offset)
        {
            return items;
        }

        let mut items = Vec::new();
        items.extend(Self::local_symbol_completion_items(
            &module,
            document,
            byte_offset,
        ));
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Type {
                    name,
                    representation,
                    valued_arms,
                    ..
                } => {
                    items.push(Self::type_completion_item(
                        name.clone(),
                        format!("type {name}"),
                        CompletionItemKind::TYPE_PARAMETER,
                        declaration.docs.as_slice(),
                    ));
                    for arm in valued_arms {
                        items.push(Self::constant_completion_item(
                            format!("{name}.{}", arm.name),
                            format!(
                                "valued-union constant {name}.{}: {}",
                                arm.name,
                                representation
                                    .as_ref()
                                    .map(|ty| ty.text.as_str())
                                    .unwrap_or(name)
                            ),
                            &[],
                        ));
                    }
                }
                SyntaxDeclarationPayload::Struct { name, .. } => {
                    items.push(Self::type_completion_item(
                        name.clone(),
                        format!("struct {name}"),
                        CompletionItemKind::STRUCT,
                        declaration.docs.as_slice(),
                    ));
                }
                SyntaxDeclarationPayload::Constructor { name, clauses, .. } => {
                    if let Some(clause) = clauses.first() {
                        items.push(Self::constructor_completion_item(
                            name.clone(),
                            format!(
                                "constructor {}/{} -> {}",
                                name,
                                clause.params.len(),
                                clause.return_type.text
                            ),
                            declaration.docs.as_slice(),
                        ));
                    }
                }
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    items.push(Self::function_completion_item(
                        name.clone(),
                        format!(
                            "{}function {}/{} -> {}",
                            Self::pure_display_prefix(Self::declaration_has_marker_annotation(
                                declaration,
                                &["pure"]
                            )),
                            name,
                            params.len(),
                            return_type.text
                        ),
                        declaration.docs.as_slice(),
                    ));
                }
                SyntaxDeclarationPayload::Constant {
                    name, annotation, ..
                } => {
                    items.push(Self::constant_completion_item(
                        name.clone(),
                        format!("const {name}: {}", annotation.text),
                        declaration.docs.as_slice(),
                    ));
                }
                SyntaxDeclarationPayload::ConstFunction {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    items.push(Self::function_completion_item(
                        name.clone(),
                        format!(
                            "const function {name}/{} -> {}",
                            params.len(),
                            return_type.text
                        ),
                        declaration.docs.as_slice(),
                    ));
                }
                SyntaxDeclarationPayload::Trait {
                    name, constants, ..
                } => {
                    items.push(Self::type_completion_item(
                        name.clone(),
                        format!("trait {name}"),
                        CompletionItemKind::INTERFACE,
                        declaration.docs.as_slice(),
                    ));
                    for constant in constants {
                        items.push(Self::constant_completion_item(
                            format!("{name}.{}", constant.name),
                            format!(
                                "trait-associated constant {name}.{}: {}",
                                constant.name, constant.annotation.text
                            ),
                            constant.docs.as_slice(),
                        ));
                    }
                }
                SyntaxDeclarationPayload::Raw { raw_kind, text } => {
                    if let Some((name, detail, _kind)) =
                        Self::raw_shape_symbol_parts(raw_kind, text)
                    {
                        items.push(Self::shape_completion_item(
                            name,
                            detail,
                            declaration.docs.as_slice(),
                        ));
                    }
                }
                _ => {}
            }
        }

        let interfaces = OpenDocuments::imported_interfaces_for_uri(uri, &module);
        for module_name in Self::target_compatible_completion_imported_modules(&module) {
            if let Some(interface) = interfaces.get(&module_name) {
                for shape in interface.shapes.values() {
                    items.push(Self::shape_completion_item(
                        shape.name.clone(),
                        format!("shape {module_name}.{}", shape.name),
                        shape.docs.as_slice(),
                    ));
                }
                for function in Self::imported_public_function_signatures(interface) {
                    items.push(Self::function_completion_item(
                        function.name.clone(),
                        format!(
                            "{}function {module_name}.{}/{} -> {}",
                            Self::pure_display_prefix(function.pure),
                            function.name,
                            function.params.len(),
                            function.return_type
                        ),
                        function.docs.as_slice(),
                    ));
                }
                for constant in interface.constants.values() {
                    items.push(Self::constant_completion_item(
                        constant.name.clone(),
                        format!(
                            "const {module_name}.{}: {} = {}",
                            constant.name, constant.annotation, constant.value_text
                        ),
                        constant.docs.as_slice(),
                    ));
                }
                for (owner, union) in &interface.valued_unions {
                    for arm in &union.arms {
                        items.push(Self::constant_completion_item(
                            format!("{owner}.{}", arm.name),
                            format!(
                                "valued-union constant {module_name}.{owner}.{}: {} = {}",
                                arm.name, union.representation, arm.value_text
                            ),
                            &[],
                        ));
                    }
                }
                for constant in interface.associated_constants.values() {
                    items.push(Self::constant_completion_item(
                        constant.name.clone(),
                        format!(
                            "trait-associated constant {module_name}.{}: {} = {}",
                            constant.name, constant.annotation, constant.value_text
                        ),
                        constant.docs.as_slice(),
                    ));
                }
                for function in interface.const_functions.values() {
                    items.push(Self::function_completion_item(
                        function.name.clone(),
                        format!(
                            "const function {module_name}.{}/{} -> {}",
                            function.name,
                            function.params.len(),
                            function.return_type
                        ),
                        function.docs.as_slice(),
                    ));
                }
                for constructor in Self::imported_public_constructor_signatures(interface) {
                    items.push(Self::constructor_completion_item(
                        constructor.name.clone(),
                        format!(
                            "constructor {module_name}.{}/{} -> {}",
                            constructor.name,
                            constructor.params.len(),
                            constructor.return_type
                        ),
                        constructor.docs.as_slice(),
                    ));
                }
                let mut public_types = interface
                    .public_types
                    .iter()
                    .filter(|name| !interface.private_types.contains(*name))
                    .collect::<Vec<_>>();
                public_types.sort();
                for name in public_types {
                    let is_struct = interface.struct_fields.contains_key(name);
                    items.push(Self::type_completion_item(
                        name.clone(),
                        if is_struct {
                            format!("struct {module_name}.{name}")
                        } else {
                            format!("type {module_name}.{name}")
                        },
                        if is_struct {
                            CompletionItemKind::STRUCT
                        } else {
                            CompletionItemKind::TYPE_PARAMETER
                        },
                        interface
                            .type_docs
                            .get(name)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                    ));
                }
                let mut traits = interface.traits.values().collect::<Vec<_>>();
                traits.sort_by(|left, right| left.name.cmp(&right.name));
                for trait_signature in traits {
                    items.push(Self::type_completion_item(
                        trait_signature.name.clone(),
                        format!("trait {module_name}.{}", trait_signature.name),
                        CompletionItemKind::INTERFACE,
                        trait_signature.docs.as_slice(),
                    ));
                }
            }
        }

        items.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.detail.cmp(&right.detail))
        });
        items.dedup_by(|left, right| left.label == right.label && left.detail == right.detail);
        items
    }

    /// Builds receiver member completions for typed function parameters.
    ///
    /// Inputs:
    /// - `uri`: document URI used to load provider summaries.
    /// - `module`: parsed syntax-output module.
    /// - `document`: current open document.
    /// - `byte_offset`: cursor byte offset.
    ///
    /// Output:
    /// - Field and method completion items when the cursor follows
    ///   `receiver.` and the receiver is a typed parameter in the active
    ///   function.
    /// - `None` when the completion request is not a supported receiver field
    ///   context.
    ///
    /// Transformation:
    /// - Resolves the receiver's annotated parameter type and projects local or
    ///   imported public struct-field and receiver-method metadata into LSP
    ///   member completions.
    pub(in super::super) fn receiver_member_completion_items(
        uri: &Url,
        module: &SyntaxModuleOutput,
        document: &OpenDocument,
        byte_offset: usize,
    ) -> Option<Vec<CompletionItem>> {
        let receiver = Self::receiver_name_before_member_access(&document.text, byte_offset)?;
        let receiver_type = Self::active_parameter_type(module, byte_offset, &receiver)?;
        let type_name = Self::base_type_name(&receiver_type);
        let mut items = Self::local_struct_field_completion_items(module, type_name);
        items.extend(Self::local_receiver_method_completion_items(
            module, type_name,
        ));
        items.extend(Self::local_impl_method_completion_items(module, type_name));

        let interfaces = OpenDocuments::imported_interfaces_for_uri(uri, module);
        for module_name in Self::target_compatible_completion_imported_modules(module) {
            let Some(interface) = interfaces.get(&module_name) else {
                continue;
            };
            if let Some(fields) = interface.struct_fields.get(type_name) {
                for field in fields.iter().filter(|field| !field.is_private) {
                    items.push(Self::field_completion_item(
                        field.name.clone(),
                        format!("field {type_name}.{}: {}", field.name, field.annotation),
                    ));
                }
            }
            items.extend(Self::imported_receiver_method_completion_items(
                interface, type_name,
            ));
        }

        if items.is_empty() {
            return None;
        }
        items.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.detail.cmp(&right.detail))
        });
        items.dedup_by(|left, right| left.label == right.label && left.detail == right.detail);
        Some(items)
    }

    /// Builds receiver-method completions from local method declarations.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `type_name`: receiver type name to inspect.
    ///
    /// Output:
    /// - Method completion items whose receiver annotation matches `type_name`.
    ///
    /// Transformation:
    /// - Reads structured receiver-method declarations and keeps the displayed
    ///   arity in dotted-call form, excluding the receiver itself.
    pub(in super::super) fn local_receiver_method_completion_items(
        module: &SyntaxModuleOutput,
        type_name: &str,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !*is_public || Self::base_type_name(&receiver.annotation.text) != type_name {
                continue;
            }
            items.push(Self::method_completion_item(
                name.clone(),
                format!(
                    "{}method {type_name}.{}/{} -> {}",
                    Self::pure_display_prefix(Self::declaration_has_marker_annotation(
                        declaration,
                        &["pure"]
                    )),
                    name,
                    params.len(),
                    return_type.text
                ),
                declaration.docs.as_slice(),
            ));
        }
        items
    }

    /// Builds receiver-method completions from local explicit impl methods.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `type_name`: receiver type name to inspect.
    ///
    /// Output:
    /// - Method completion items for public impl blocks targeting `type_name`.
    ///
    /// Transformation:
    /// - Treats the first impl method parameter as the receiver when its type
    ///   matches the impl target, mirroring receiver-call dispatch.
    pub(in super::super) fn local_impl_method_completion_items(
        module: &SyntaxModuleOutput,
        type_name: &str,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::TraitImpl {
                for_type,
                is_public,
                methods,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !*is_public || Self::base_type_name(&for_type.text) != type_name {
                continue;
            }
            for method in methods {
                let Some(receiver) = method.params.first() else {
                    continue;
                };
                if Self::base_type_name(&receiver.annotation.text) != type_name {
                    continue;
                }
                items.push(Self::method_completion_item(
                    method.name.clone(),
                    format!(
                        "method {type_name}.{}/{} -> {}",
                        method.name,
                        method.params.len().saturating_sub(1),
                        method.return_type.text
                    ),
                    &[],
                ));
            }
        }
        items
    }

    /// Builds receiver-method completions from imported public interfaces.
    ///
    /// Inputs:
    /// - `interface`: generated provider summary loaded for the current file.
    /// - `type_name`: receiver type name to inspect.
    ///
    /// Output:
    /// - Imported receiver-method completion items whose first parameter is the
    ///   receiver type.
    ///
    /// Transformation:
    /// - Uses HIR interface function metadata so imported method completions
    ///   obey public/private and generated-summary boundaries.
    pub(in super::super) fn imported_receiver_method_completion_items(
        interface: &crate::terlan_hir::ModuleInterface,
        type_name: &str,
    ) -> Vec<CompletionItem> {
        let mut signatures = interface
            .function_overloads
            .values()
            .flat_map(|overloads| overloads.iter())
            .filter(|signature| {
                signature.public
                    && signature.receiver_method
                    && signature
                        .params
                        .first()
                        .map(|param| Self::base_type_name(&param.annotation) == type_name)
                        .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        signatures.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.params.len().cmp(&right.params.len()))
                .then_with(|| left.return_type.cmp(&right.return_type))
        });

        signatures
            .into_iter()
            .map(|signature| {
                Self::method_completion_item(
                    signature.name.clone(),
                    format!(
                        "{}method {type_name}.{}/{} -> {}",
                        Self::pure_display_prefix(signature.pure),
                        signature.name,
                        signature.params.len().saturating_sub(1),
                        signature.return_type
                    ),
                    signature.docs.as_slice(),
                )
            })
            .collect()
    }

    /// Finds the receiver identifier before a member-access completion point.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `byte_offset`: cursor byte offset.
    ///
    /// Output:
    /// - Receiver identifier when the cursor follows `receiver.`.
    ///
    /// Transformation:
    /// - Performs a small lexical check around the cursor and reuses the same
    ///   identifier-bound helper as definition navigation.
    pub(in super::super) fn receiver_name_before_member_access(
        text: &str,
        byte_offset: usize,
    ) -> Option<String> {
        let bytes = text.as_bytes();
        let mut cursor = byte_offset.min(bytes.len());
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        if cursor == 0 || bytes.get(cursor - 1) != Some(&b'.') {
            return None;
        }
        let (receiver, _, _) = Self::identifier_before_byte_offset(text, cursor - 1)?;
        Some(receiver)
    }

    /// Finds a typed parameter in the function containing the cursor.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `byte_offset`: cursor byte offset.
    /// - `name`: parameter name to resolve.
    ///
    /// Output:
    /// - Parameter annotation text.
    ///
    /// Transformation:
    /// - Uses function declaration spans to keep parameter type lookup scoped to
    ///   the active function.
    pub(in super::super) fn active_parameter_type(
        module: &SyntaxModuleOutput,
        byte_offset: usize,
        name: &str,
    ) -> Option<String> {
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Function { params, .. } = &declaration.payload else {
                continue;
            };
            if byte_offset < declaration.span.start || byte_offset > declaration.span.end {
                continue;
            }
            if let Some(param) = params.iter().find(|param| param.name == name) {
                return Some(param.annotation.text.clone());
            }
        }
        None
    }

    /// Returns the unqualified base type name for simple generic annotations.
    ///
    /// Inputs:
    /// - `type_name`: syntax-output annotation text.
    ///
    /// Output:
    /// - Base name without generic arguments or module qualifier.
    ///
    /// Transformation:
    /// - Keeps receiver field completion conservative for simple struct names
    ///   while still tolerating `pkg.Type[T]` annotations.
    pub(in super::super) fn base_type_name(type_name: &str) -> &str {
        type_name
            .split('[')
            .next()
            .unwrap_or(type_name)
            .rsplit('.')
            .next()
            .unwrap_or(type_name)
            .trim()
    }
}
