use super::super::*;

impl Backend {
    /// Builds deterministic provider-file candidates for an imported module.
    ///
    /// Inputs:
    /// - `current_dir`: directory containing the consumer document.
    /// - `module_name`: imported module name, using Terlan dotted spelling.
    ///
    /// Output:
    /// - Candidate source/interface/summary paths in lookup order.
    ///
    /// Transformation:
    /// - Supports both `provider.terli` flat summaries and
    ///   `std/summaries/std.core.Option.typi` generated-summary layout without
    ///   making the language server depend on package-manager state.
    pub(in super::super) fn provider_definition_candidates(
        current_dir: &Path,
        module_name: &str,
    ) -> Vec<PathBuf> {
        let slash_module = module_name.replace('.', "/");
        vec![
            current_dir.join(format!("{module_name}.terli")),
            current_dir.join(format!("{module_name}.typi")),
            current_dir.join(format!("{module_name}.terl")),
            current_dir.join(format!("{slash_module}.terli")),
            current_dir.join(format!("{slash_module}.typi")),
            current_dir.join(format!("{slash_module}.terl")),
            current_dir
                .join("std")
                .join("summaries")
                .join(format!("{module_name}.typi")),
        ]
    }

    /// Returns the source identifier under a byte offset.
    ///
    /// Inputs:
    /// - `text`: source document text.
    /// - `byte_offset`: byte offset produced from an LSP position.
    ///
    /// Output:
    /// - Identifier text when the offset touches a Terlan identifier.
    /// - `None` when the offset is outside text or on punctuation/whitespace.
    ///
    /// Transformation:
    /// - Expands left and right over ASCII identifier characters. This matches
    ///   the current Terlan identifier subset used by the parser and keeps
    ///   definition lookup conservative for dotted module-member references.
    pub(crate) fn identifier_at_byte_offset(text: &str, byte_offset: usize) -> Option<String> {
        Self::identifier_bounds_at_byte_offset(text, byte_offset)
            .map(|(identifier, _start, _end)| identifier)
    }

    /// Returns the source identifier and byte bounds under a byte offset.
    ///
    /// Inputs:
    /// - `text`: source document text.
    /// - `byte_offset`: byte offset produced from an LSP position.
    ///
    /// Output:
    /// - Identifier text with start/end byte offsets when the offset touches a
    ///   Terlan identifier.
    /// - `None` when the offset is outside text or on punctuation/whitespace.
    ///
    /// Transformation:
    /// - Expands left and right over ASCII identifier characters. The bounds
    ///   let definition lookup inspect receiver-call punctuation without
    ///   reparsing the current document.
    pub(in super::super) fn identifier_bounds_at_byte_offset(
        text: &str,
        byte_offset: usize,
    ) -> Option<(String, usize, usize)> {
        if byte_offset > text.len() || !text.is_char_boundary(byte_offset) {
            return None;
        }
        let bytes = text.as_bytes();
        let mut start = byte_offset;
        if start == text.len() && start > 0 {
            start -= 1;
        }
        if !Self::is_identifier_byte(*bytes.get(start)?) {
            if start == 0 || !Self::is_identifier_byte(bytes[start - 1]) {
                return None;
            }
            start -= 1;
        }
        while start > 0 && Self::is_identifier_byte(bytes[start - 1]) {
            start -= 1;
        }

        let mut end = start;
        while end < bytes.len() && Self::is_identifier_byte(bytes[end]) {
            end += 1;
        }
        (end > start).then(|| (text[start..end].to_string(), start, end))
    }

    /// Reports whether an identifier is used as a receiver member.
    ///
    /// Inputs:
    /// - `text`: source document text.
    /// - `identifier_start`: byte offset where the identifier starts.
    ///
    /// Output:
    /// - `true` when the previous non-whitespace byte is `.`.
    ///
    /// Transformation:
    /// - Uses a lightweight lexical check to distinguish `value.show()` from
    ///   ordinary references while keeping LSP navigation independent from the
    ///   typechecker.
    pub(in super::super) fn is_receiver_member_reference(
        text: &str,
        identifier_start: usize,
    ) -> bool {
        text.as_bytes()
            .get(..identifier_start)
            .and_then(|prefix| prefix.iter().rev().find(|byte| !byte.is_ascii_whitespace()))
            .is_some_and(|byte| *byte == b'.')
    }

    /// Checks whether a byte is part of a Terlan identifier.
    ///
    /// Inputs:
    /// - `byte`: candidate source byte.
    ///
    /// Output:
    /// - `true` for ASCII letters, digits, and underscore.
    ///
    /// Transformation:
    /// - Mirrors the initial LSP identifier lookup subset without depending on
    ///   parser internals or allocating.
    pub(crate) fn is_identifier_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    /// Finds a symbol selection range by name.
    ///
    /// Inputs:
    /// - `symbols`: nested document symbols.
    /// - `name`: identifier text under the cursor.
    ///
    /// Output:
    /// - Selection range for the first matching symbol.
    /// - `None` when no symbol name matches.
    ///
    /// Transformation:
    /// - Walks module and child symbols in document order, preserving the same
    ///   ordering editors already receive from `textDocument/documentSymbol`.
    pub(in super::super) fn find_symbol_selection_range(
        symbols: &[DocumentSymbol],
        name: &str,
    ) -> Option<Range> {
        for symbol in symbols {
            if symbol.name == name {
                return Some(symbol.selection_range);
            }
            if let Some(children) = &symbol.children {
                if let Some(range) = Self::find_symbol_selection_range(children, name) {
                    return Some(range);
                }
            }
        }
        None
    }

    /// Finds a named child symbol below a named parent symbol.
    ///
    /// Inputs:
    /// - `symbols`: nested document symbols.
    /// - `parent_name`: owner declaration name.
    /// - `child_name`: nested declaration name.
    /// - `child_kind`: expected nested symbol kind.
    ///
    /// Output:
    /// - Selection range for the matching child.
    ///
    /// Transformation:
    /// - Recurses until the parent is found, then searches only that parent’s
    ///   immediate children so field navigation stays tied to its struct.
    pub(in super::super) fn find_child_symbol_selection_range(
        symbols: &[DocumentSymbol],
        parent_name: &str,
        child_name: &str,
        child_kind: SymbolKind,
    ) -> Option<Range> {
        for symbol in symbols {
            if symbol.name == parent_name {
                return symbol.children.as_deref().and_then(|children| {
                    children
                        .iter()
                        .find(|child| child.name == child_name && child.kind == child_kind)
                        .map(|child| child.selection_range)
                });
            }
            if let Some(children) = &symbol.children {
                if let Some(range) = Self::find_child_symbol_selection_range(
                    children,
                    parent_name,
                    child_name,
                    child_kind,
                ) {
                    return Some(range);
                }
            }
        }
        None
    }

    /// Finds the best method target for receiver-call definition lookup.
    ///
    /// Inputs:
    /// - `symbols`: nested document symbols.
    /// - `name`: receiver member name under the cursor.
    ///
    /// Output:
    /// - Impl method range when present, otherwise a concrete receiver method,
    ///   otherwise a trait method.
    ///
    /// Transformation:
    /// - Walks the outline tree but ranks method declarations by runtime
    ///   specificity so `value.method()` jumps to executable code before an
    ///   abstract trait requirement.
    pub(in super::super) fn find_receiver_method_selection_range(
        symbols: &[DocumentSymbol],
        name: &str,
    ) -> Option<Range> {
        Self::receiver_method_candidates(symbols, name)
            .into_iter()
            .min_by_key(|(priority, _range)| *priority)
            .map(|(_priority, range)| range)
    }

    /// Collects matching method candidates for receiver-call navigation.
    ///
    /// Inputs:
    /// - `symbols`: nested document symbols.
    /// - `name`: method name.
    ///
    /// Output:
    /// - Priority/range pairs, where lower priority is preferred.
    ///
    /// Transformation:
    /// - Keeps ranking separate from traversal so new method-like symbols can
    ///   join the navigation policy without changing request handling.
    pub(in super::super) fn receiver_method_candidates(
        symbols: &[DocumentSymbol],
        name: &str,
    ) -> Vec<(u8, Range)> {
        let mut candidates = Vec::new();
        for symbol in symbols {
            if symbol.name == name && symbol.kind == SymbolKind::METHOD {
                if let Some(priority) = Self::receiver_method_priority(symbol.detail.as_deref()) {
                    candidates.push((priority, symbol.selection_range));
                }
            }
            if let Some(children) = &symbol.children {
                candidates.extend(Self::receiver_method_candidates(children, name));
            }
        }
        candidates
    }

    /// Ranks method-like document symbols for receiver-call definition lookup.
    ///
    /// Inputs:
    /// - `detail`: LSP detail text produced by the document-symbol projector.
    ///
    /// Output:
    /// - Lower priority for more concrete executable definitions.
    /// - `None` for non-receiver-navigation method symbols.
    ///
    /// Transformation:
    /// - Treats explicit impl bodies as the strongest target, concrete receiver
    ///   methods as next-best, and trait signatures/defaults as fallback.
    pub(in super::super) fn receiver_method_priority(detail: Option<&str>) -> Option<u8> {
        match detail {
            Some("impl method") => Some(0),
            Some("pub receiver method") | Some("receiver method") => Some(1),
            Some("trait default method") => Some(2),
            Some("trait method") => Some(3),
            _ => None,
        }
    }

    /// Builds the top-level module document symbol.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `module`: parsed syntax-output module.
    ///
    /// Output:
    /// - One module symbol with declaration children.
    ///
    /// Transformation:
    /// - Converts the module span and declaration payloads into the nested LSP
    ///   symbol shape expected by editors.
    pub(in super::super) fn module_document_symbol(
        text: &str,
        module: &SyntaxModuleOutput,
    ) -> DocumentSymbol {
        let module_span = Span::new(module.span.start, module.span.end);
        let module_range = OpenDocument::range_from_span(text, &module_span);
        let selection_range = Self::symbol_selection_range(text, &module_span, &module.module_name)
            .unwrap_or(module_range);
        let children = module
            .declarations
            .iter()
            .filter_map(|declaration| {
                Self::declaration_document_symbol(text, &declaration.payload, &declaration.span)
            })
            .collect::<Vec<_>>();

        Self::document_symbol(
            module.module_name.clone(),
            Some("module".to_string()),
            SymbolKind::MODULE,
            module_range,
            selection_range,
            Some(children),
        )
    }

    /// Builds one LSP document symbol across supported `lsp-types` versions.
    ///
    /// Inputs:
    /// - Symbol display fields plus range and optional children.
    ///
    /// Output:
    /// - A `DocumentSymbol` accepted by the pinned editor LSP dependency.
    ///
    /// Transformation:
    /// - Centralizes the legacy `deprecated` compatibility field required by
    ///   `lsp-types` 0.94 while keeping current callers on the replacement
    ///   `tags` field.
    pub(in super::super) fn document_symbol(
        name: String,
        detail: Option<String>,
        kind: SymbolKind,
        range: Range,
        selection_range: Range,
        children: Option<Vec<DocumentSymbol>>,
    ) -> DocumentSymbol {
        let value = serde_json::json!({
            "name": name,
            "detail": detail,
            "kind": kind,
            "tags": null,
            "range": range,
            "selectionRange": selection_range,
            "children": children,
        });
        match serde_json::from_value(value) {
            Ok(symbol) => symbol,
            Err(error) => {
                unreachable!("typed LSP document-symbol fields must deserialize: {error}")
            }
        }
    }

    /// Builds one declaration document symbol.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `payload`: syntax-output declaration payload.
    /// - `span`: declaration source span.
    ///
    /// Output:
    /// - LSP document symbol when the declaration has a user-facing name.
    ///
    /// Transformation:
    /// - Maps compiler declaration variants to stable editor symbol names and
    ///   broad LSP symbol kinds.
    pub(in super::super) fn declaration_document_symbol(
        text: &str,
        payload: &SyntaxDeclarationPayload,
        span: &crate::terlan_syntax::ebnf::EbnfSourceSpan,
    ) -> Option<DocumentSymbol> {
        let (name, detail, kind) = Self::declaration_symbol_parts(payload)?;
        let source_span = Span::new(span.start, span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, &name).unwrap_or(range);
        let children = Self::declaration_symbol_children(text, payload);
        Some(Self::document_symbol(
            name,
            Some(detail),
            kind,
            range,
            selection_range,
            children,
        ))
    }

    /// Builds nested document symbols owned by a declaration.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `payload`: syntax-output declaration payload.
    ///
    /// Output:
    /// - Struct field, trait method, or impl method children when the
    ///   declaration has nested editor-visible members.
    /// - `None` for declarations without nested symbols.
    ///
    /// Transformation:
    /// - Reuses syntax-output child spans so editor outline trees stay aligned
    ///   with parser ownership and source order.
    pub(in super::super) fn declaration_symbol_children(
        text: &str,
        payload: &SyntaxDeclarationPayload,
    ) -> Option<Vec<DocumentSymbol>> {
        let children = match payload {
            SyntaxDeclarationPayload::Struct { fields, .. } => fields
                .iter()
                .map(|field| Self::struct_field_document_symbol(text, field))
                .collect::<Vec<_>>(),
            SyntaxDeclarationPayload::Type { valued_arms, .. } => valued_arms
                .iter()
                .map(|arm| {
                    Self::nested_constant_document_symbol(
                        text,
                        &arm.name,
                        "valued-union constant",
                        &arm.span,
                    )
                })
                .collect::<Vec<_>>(),
            SyntaxDeclarationPayload::Trait {
                methods, constants, ..
            } => methods
                .iter()
                .map(|method| Self::trait_method_document_symbol(text, method))
                .chain(constants.iter().map(|constant| {
                    Self::nested_constant_document_symbol(
                        text,
                        &constant.name,
                        "trait-associated constant",
                        &constant.span,
                    )
                }))
                .collect::<Vec<_>>(),
            SyntaxDeclarationPayload::TraitImpl {
                methods, constants, ..
            } => methods
                .iter()
                .map(|method| Self::impl_method_document_symbol(text, method))
                .chain(constants.iter().map(|constant| {
                    Self::nested_constant_document_symbol(
                        text,
                        &constant.name,
                        "trait-associated constant implementation",
                        &constant.span,
                    )
                }))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        (!children.is_empty()).then_some(children)
    }

    /// Builds an outline child for a valued-union or trait-owned constant.
    pub(in super::super) fn nested_constant_document_symbol(
        text: &str,
        name: &str,
        detail: &str,
        span: &crate::terlan_syntax::ebnf::EbnfSourceSpan,
    ) -> DocumentSymbol {
        let source_span = Span::new(span.start, span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, name).unwrap_or(range);
        Self::document_symbol(
            name.to_string(),
            Some(detail.to_string()),
            SymbolKind::CONSTANT,
            range,
            selection_range,
            None,
        )
    }

    /// Builds a struct field outline child.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `field`: syntax-output field payload.
    ///
    /// Output:
    /// - Field document symbol with privacy reflected in the detail text.
    ///
    /// Transformation:
    /// - Projects the parser field span into an LSP property icon without
    ///   changing compiler semantics.
    pub(in super::super) fn struct_field_document_symbol(
        text: &str,
        field: &SyntaxStructFieldOutput,
    ) -> DocumentSymbol {
        let source_span = Span::new(field.span.start, field.span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, &field.name).unwrap_or(range);
        Self::document_symbol(
            field.name.clone(),
            Some(if field.is_private {
                "private field".to_string()
            } else {
                "field".to_string()
            }),
            SymbolKind::FIELD,
            range,
            selection_range,
            None,
        )
    }

    /// Builds a trait method outline child.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `method`: syntax-output trait method payload.
    ///
    /// Output:
    /// - Method document symbol with default-method status in detail.
    ///
    /// Transformation:
    /// - Keeps trait requirements visible below the trait symbol in editor
    ///   Outline without flattening them into module declarations.
    pub(in super::super) fn trait_method_document_symbol(
        text: &str,
        method: &SyntaxTraitMethodOutput,
    ) -> DocumentSymbol {
        let source_span = Span::new(method.span.start, method.span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, &method.name).unwrap_or(range);
        Self::document_symbol(
            method.name.clone(),
            Some(match (method.is_pure, method.default_body.is_some()) {
                (true, true) => "pure trait default method".to_string(),
                (true, false) => "pure trait method".to_string(),
                (false, true) => "trait default method".to_string(),
                (false, false) => "trait method".to_string(),
            }),
            SymbolKind::METHOD,
            range,
            selection_range,
            None,
        )
    }

    /// Builds an implementation method outline child.
    ///
    /// Inputs:
    /// - `text`: current document text used for range conversion.
    /// - `method`: syntax-output impl method payload.
    ///
    /// Output:
    /// - Method document symbol beneath the impl declaration.
    ///
    /// Transformation:
    /// - Represents implementation bodies as nested editor symbols while
    ///   preserving the explicit `impl Trait for Type` parent.
    pub(in super::super) fn impl_method_document_symbol(
        text: &str,
        method: &SyntaxImplMethodOutput,
    ) -> DocumentSymbol {
        let source_span = Span::new(method.span.start, method.span.end);
        let range = OpenDocument::range_from_span(text, &source_span);
        let selection_range =
            Self::symbol_selection_range(text, &source_span, &method.name).unwrap_or(range);
        Self::document_symbol(
            method.name.clone(),
            Some("impl method".to_string()),
            SymbolKind::METHOD,
            range,
            selection_range,
            None,
        )
    }

    /// Builds a name-only selection range inside a broader symbol span.
    ///
    /// Inputs:
    /// - `text`: full document text.
    /// - `span`: byte range for the enclosing module or declaration.
    /// - `name`: symbol name to locate inside that range.
    ///
    /// Output:
    /// - LSP range for the first matching symbol name, or `None` if the name
    ///   cannot be found inside the span.
    ///
    /// Transformation:
    /// - Searches only within the compiler-provided span and converts the
    ///   matched byte range back to UTF-16 LSP coordinates.
    pub(in super::super) fn symbol_selection_range(
        text: &str,
        span: &Span,
        name: &str,
    ) -> Option<Range> {
        let start = span.start.min(text.len());
        let end = span.end.min(text.len());
        if start >= end || name.is_empty() {
            return None;
        }
        let haystack = &text[start..end];
        let relative_start = haystack.find(name)?;
        let name_start = start + relative_start;
        let name_end = name_start + name.len();
        Some(OpenDocument::range_from_span(
            text,
            &Span::new(name_start, name_end),
        ))
    }

    /// Returns declaration symbol metadata.
    ///
    /// Inputs:
    /// - `payload`: syntax-output declaration payload.
    ///
    /// Output:
    /// - Symbol name, detail label, and LSP symbol kind for named declarations.
    ///
    /// Transformation:
    /// - Keeps editor symbol naming centralized so future declarations can be
    ///   added without changing the LSP request handler.
    pub(in super::super) fn declaration_symbol_parts(
        payload: &SyntaxDeclarationPayload,
    ) -> Option<(String, String, SymbolKind)> {
        match payload {
            SyntaxDeclarationPayload::Type {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "type");
                Some((name.clone(), detail, SymbolKind::STRUCT))
            }
            SyntaxDeclarationPayload::Struct {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "struct");
                Some((name.clone(), detail, SymbolKind::STRUCT))
            }
            SyntaxDeclarationPayload::Constructor {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "constructor");
                Some((name.clone(), detail, SymbolKind::CONSTRUCTOR))
            }
            SyntaxDeclarationPayload::Constant {
                name, is_public, ..
            } => Some((
                name.clone(),
                Self::visibility_detail(*is_public, "constant"),
                SymbolKind::CONSTANT,
            )),
            SyntaxDeclarationPayload::ConstFunction {
                name, is_public, ..
            } => Some((
                name.clone(),
                Self::visibility_detail(*is_public, "const function"),
                SymbolKind::FUNCTION,
            )),
            SyntaxDeclarationPayload::Function {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "function");
                Some((name.clone(), detail, SymbolKind::FUNCTION))
            }
            SyntaxDeclarationPayload::Method {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "receiver method");
                Some((name.clone(), detail, SymbolKind::METHOD))
            }
            SyntaxDeclarationPayload::Trait {
                name, is_public, ..
            } => {
                let detail = Self::visibility_detail(*is_public, "trait");
                Some((name.clone(), detail, SymbolKind::INTERFACE))
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                generic_params,
                for_type,
                is_negative,
                is_public,
                ..
            } => {
                let detail = Self::visibility_detail(
                    *is_public,
                    if *is_negative {
                        "negative impl"
                    } else {
                        "impl"
                    },
                );
                Some((
                    format!(
                        "{}{} for {}",
                        if *is_negative { "not " } else { "" },
                        crate::terlan_syntax::render_trait_impl_ref(
                            &trait_ref.text,
                            generic_params
                        ),
                        for_type.text
                    ),
                    detail,
                    SymbolKind::INTERFACE,
                ))
            }
            SyntaxDeclarationPayload::AnnotationSchema { path, .. } => {
                Some((path.join("."), "annotation".to_string(), SymbolKind::KEY))
            }
            SyntaxDeclarationPayload::Template { name, .. } => {
                Some((name.clone(), "template".to_string(), SymbolKind::FUNCTION))
            }
            SyntaxDeclarationPayload::Config { name, .. } => {
                Some((name.clone(), "config".to_string(), SymbolKind::OBJECT))
            }
            SyntaxDeclarationPayload::Raw { raw_kind, text } => {
                Self::raw_shape_symbol_parts(raw_kind, text)
            }
            SyntaxDeclarationPayload::Import { .. } | SyntaxDeclarationPayload::Export { .. } => {
                None
            }
        }
    }

    /// Returns symbol metadata for parse-preserved raw shape declarations.
    ///
    /// Inputs:
    /// - `raw_kind`: raw declaration kind emitted by syntax output.
    /// - `text`: original declaration text.
    ///
    /// Output:
    /// - Symbol name, detail label, and LSP symbol kind for reserved shape
    ///   declarations.
    ///
    /// Transformation:
    /// - Keeps shape declarations visible in editor outlines while semantic
    ///   expansion is still intentionally blocked by the compiler.
    pub(in super::super) fn raw_shape_symbol_parts(
        raw_kind: &str,
        text: &str,
    ) -> Option<(String, String, SymbolKind)> {
        if raw_kind != "shape" {
            return None;
        }

        let trimmed = text.trim_start();
        let (is_public, after_visibility) = if let Some(rest) = trimmed
            .strip_prefix("pub")
            .and_then(Self::trim_keyword_rest)
        {
            (true, rest)
        } else {
            (false, trimmed)
        };
        let after_shape = after_visibility
            .strip_prefix("shape")
            .and_then(Self::trim_keyword_rest)?;
        let name = after_shape
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect::<String>();
        if name.is_empty() {
            return None;
        }

        let detail = Self::visibility_detail(is_public, "shape");
        Some((name, detail, SymbolKind::STRUCT))
    }

    /// Trims whitespace after a recognized keyword token.
    ///
    /// Inputs:
    /// - `rest`: source text immediately after the keyword spelling.
    ///
    /// Output:
    /// - Remaining source after required whitespace.
    ///
    /// Transformation:
    /// - Prevents prefix matches such as `publisher` or `shapeName` from being
    ///   treated as keyword-bearing declarations.
    pub(in super::super) fn trim_keyword_rest(rest: &str) -> Option<&str> {
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !first.is_whitespace() {
            return None;
        }
        Some(chars.as_str().trim_start())
    }

    /// Formats visibility-aware symbol detail text.
    ///
    /// Inputs:
    /// - `is_public`: declaration visibility from syntax output.
    /// - `label`: human-readable symbol category.
    ///
    /// Output:
    /// - Stable detail string for editor outline rows.
    ///
    /// Transformation:
    /// - Encodes public/private distinction in standard LSP detail metadata
    ///   instead of inventing custom protocol fields.
    pub(in super::super) fn visibility_detail(is_public: bool, label: &str) -> String {
        if is_public {
            format!("pub {label}")
        } else {
            label.to_string()
        }
    }
}
