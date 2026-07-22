impl Backend {
    /// Finds top-level argument starts inside a simple same-line call.
    ///
    /// Inputs:
    /// - `line`: source line.
    /// - `open_paren`: byte offset of the opening parenthesis.
    /// - `close_paren`: byte offset of the matching closing parenthesis.
    ///
    /// Output:
    /// - Byte offsets where top-level arguments begin.
    ///
    /// Transformation:
    /// - Treats the first non-whitespace byte after `(` or a top-level comma
    ///   as the argument start, ignoring nested comma separators.
    fn simple_call_argument_positions(
        line: &str,
        open_paren: usize,
        close_paren: usize,
    ) -> Vec<usize> {
        let bytes = line.as_bytes();
        let mut starts = Vec::new();
        let mut depth = 0usize;
        let mut expect_argument = true;
        for index in (open_paren + 1)..close_paren.min(bytes.len()) {
            let byte = bytes[index];
            match byte {
                b' ' | b'\t' if expect_argument => {}
                b',' if depth == 0 => expect_argument = true,
                b'(' | b'[' | b'{' => {
                    if expect_argument {
                        starts.push(index);
                        expect_argument = false;
                    }
                    depth += 1;
                }
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                _ if expect_argument => {
                    starts.push(index);
                    expect_argument = false;
                }
                _ => {}
            }
        }
        starts
    }

    /// Builds one inlay hint for a simple literal let-binding line.
    ///
    /// Inputs:
    /// - `line`: source line.
    /// - `line_number`: zero-based LSP line.
    ///
    /// Output:
    /// - Type inlay hint when the line has `let name = literal` shape.
    ///
    /// Transformation:
    /// - Identifies the binding name and literal category while avoiding
    ///   non-name patterns, explicit type annotations, and non-literal values.
    fn let_literal_inlay_hint_for_line(line: &str, line_number: u32) -> Option<InlayHint> {
        let leading_bytes = line.len() - line.trim_start().len();
        let (name, type_name) = Self::simple_let_binding_for_line(line)?;
        let type_name = type_name?;
        let character = u32::try_from(leading_bytes + "let ".len() + name.len()).ok()?;
        Some(Self::literal_binding_inlay_hint(
            line_number,
            character,
            type_name,
        ))
    }

    /// Builds one inlay hint for a simple continued let-chain binding line.
    ///
    /// Inputs:
    /// - `line`: source line.
    /// - `line_number`: zero-based LSP line.
    ///
    /// Output:
    /// - Type inlay hint when the line has `name = literal` shape.
    ///
    /// Transformation:
    /// - Identifies a continued binding name and literal category; callers
    ///   decide whether the line is actually inside a semicolon-continued let
    ///   chain.
    fn assignment_literal_inlay_hint_for_line(line: &str, line_number: u32) -> Option<InlayHint> {
        let leading_bytes = line.len() - line.trim_start().len();
        let (name, type_name) = Self::simple_assignment_binding_for_line(line)?;
        let type_name = type_name?;
        let character = u32::try_from(leading_bytes + name.len()).ok()?;
        Some(Self::literal_binding_inlay_hint(
            line_number,
            character,
            type_name,
        ))
    }

    /// Builds a literal binding type inlay hint.
    ///
    /// Inputs:
    /// - `line_number`: zero-based LSP line.
    /// - `character`: zero-based LSP character after the binding name.
    /// - `type_name`: inferred literal type name.
    ///
    /// Output:
    /// - Type inlay hint at the supplied binding-name boundary.
    ///
    /// Transformation:
    /// - Centralizes presentation for initial and continued literal bindings.
    fn literal_binding_inlay_hint(line_number: u32, character: u32, type_name: &str) -> InlayHint {
        InlayHint {
            position: Position::new(line_number, character),
            label: InlayHintLabel::String(format!(": {type_name}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(
                "Inferred from a simple literal binding.".to_string(),
            )),
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        }
    }

    /// Classifies simple literal text for inlay hints.
    ///
    /// Inputs:
    /// - `value`: source text to the right of a let-binding equals sign.
    ///
    /// Output:
    /// - Terlan type name for supported literals.
    ///
    /// Transformation:
    /// - Uses exact literal categories only; unknown or ambiguous expressions
    ///   intentionally produce no hint.
    fn literal_type_name(value: &str) -> Option<&'static str> {
        if value == "true" || value == "false" {
            return Some("Bool");
        }
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            return Some("String");
        }
        if value.parse::<i64>().is_ok() {
            return Some("Int");
        }
        None
    }

    /// Finds same-document reference locations for a source position.
    ///
    /// Inputs:
    /// - `uri`: document URI used in returned LSP locations.
    /// - `document`: current open-document snapshot.
    /// - `position`: cursor position from the editor.
    ///
    /// Output:
    /// - Locations for exact identifier-token matches in document order.
    /// - Empty vector when the cursor is not on an identifier.
    ///
    /// Transformation:
    /// - Extracts the identifier under the cursor and scans byte-wise while
    ///   requiring non-identifier boundaries on both sides, preventing partial
    ///   matches such as `id` inside `user_id`.
    fn reference_locations_for_position(
        uri: &Url,
        document: &OpenDocument,
        position: Position,
    ) -> Vec<Location> {
        let Some(byte_offset) = document.byte_offset_from_position(position) else {
            return Vec::new();
        };
        let Some((identifier, _identifier_start, _identifier_end)) =
            Self::identifier_bounds_at_byte_offset(&document.text, byte_offset)
        else {
            return Vec::new();
        };
        Self::reference_locations_for_identifier(uri, &document.text, &identifier)
    }

    /// Reports whether a reference location is a declaration-like occurrence.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    /// - `range`: candidate reference range.
    ///
    /// Output:
    /// - `true` for local binding and selected-import declaration positions.
    ///
    /// Transformation:
    /// - Uses the line prefix before the identifier as a conservative editor
    ///   navigation heuristic, so `includeDeclaration=false` removes local
    ///   declarations without deleting ordinary use-site references.
    fn is_reference_declaration_range(document: &OpenDocument, range: Range) -> bool {
        let Some(start) = document.byte_offset_from_position(range.start) else {
            return false;
        };
        let Some(end) = document.byte_offset_from_position(range.end) else {
            return false;
        };
        Self::is_declaration_identifier_span(&document.text, start, end)
    }

    /// Reports whether an identifier byte span is a declaration.
    ///
    /// Inputs:
    /// - `text`: source document text.
    /// - `identifier_start`: byte offset where an identifier starts.
    /// - `identifier_end`: byte offset where an identifier ends.
    ///
    /// Output:
    /// - `true` for local `let` bindings, selected import items, and callable
    ///   declaration heads.
    ///
    /// Transformation:
    /// - Looks only at the current line around the identifier to avoid broad
    ///   workspace or typechecker dependence in the first references provider.
    fn is_declaration_identifier_span(
        text: &str,
        identifier_start: usize,
        identifier_end: usize,
    ) -> bool {
        let line_start = text[..identifier_start]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let line_end = text[identifier_end..]
            .find('\n')
            .map_or(text.len(), |offset| identifier_end + offset);
        let prefix = text[line_start..identifier_start].trim_start().trim_end();
        if prefix == "let" || prefix.starts_with("import ") {
            return true;
        }

        let suffix = text[identifier_end..line_end].trim_start();
        let starts_declaration_line = prefix.is_empty() || prefix == "pub";
        starts_declaration_line && suffix.starts_with('(') && suffix.contains("):")
    }

    /// Finds exact same-document identifier-token references.
    ///
    /// Inputs:
    /// - `uri`: document URI used in returned LSP locations.
    /// - `text`: source document text.
    /// - `identifier`: identifier token to search.
    ///
    /// Output:
    /// - Locations for exact identifier-token matches in document order.
    ///
    /// Transformation:
    /// - Uses the syntax lexer so comments, strings, binaries, punctuation, and
    ///   keywords are not reported as references.
    fn reference_locations_for_identifier(
        uri: &Url,
        text: &str,
        identifier: &str,
    ) -> Vec<Location> {
        if identifier.is_empty() {
            return Vec::new();
        }

        lex(text)
            .unwrap_or_default()
            .into_iter()
            .filter(|token| {
                matches!(token.kind, TokenKind::Atom | TokenKind::Var) && token.text == identifier
            })
            .map(|token| {
                Location::new(
                    uri.clone(),
                    OpenDocument::range_from_span(text, &Span::new(token.start, token.end)),
                )
            })
            .collect()
    }

    /// Finds definition locations for a source position.
    ///
    /// Inputs:
    /// - `uri`: document URI used in returned LSP locations.
    /// - `document`: current open-document snapshot.
    /// - `position`: cursor position from the editor.
    ///
    /// Output:
    /// - One location for a matching declaration symbol.
    /// - Empty vector when the cursor is not on an identifier, parsing fails,
    ///   or the identifier has no visible declaration match.
    ///
    /// Transformation:
    /// - Extracts the identifier under the cursor, reuses compiler-backed
    ///   document symbols for same-file targets, and falls back to selected
    ///   imported public symbols when a provider interface/source file is
    ///   available beside the current document.
    fn definition_locations_for_position(
        uri: &Url,
        document: &OpenDocument,
        position: Position,
    ) -> Vec<Location> {
        let Some(byte_offset) = document.byte_offset_from_position(position) else {
            return Vec::new();
        };
        let Some((identifier, identifier_start, _identifier_end)) =
            Self::identifier_bounds_at_byte_offset(&document.text, byte_offset)
        else {
            return Vec::new();
        };
        let symbols = Self::document_symbols_for_text(&document.text);
        if Self::is_receiver_member_reference(&document.text, identifier_start) {
            if let Some(range) = Self::find_receiver_method_selection_range(&symbols, &identifier) {
                return vec![Location::new(uri.clone(), range)];
            }
            if let Ok(module) = parse_module_as_syntax_output(&document.text) {
                if let Some(location) = Self::imported_receiver_field_definition_location(
                    uri,
                    &module,
                    &document.text,
                    identifier_start,
                    &identifier,
                ) {
                    return vec![location];
                }
            }
        }
        if let Some(range) = Self::find_symbol_selection_range(&symbols, &identifier) {
            return vec![Location::new(uri.clone(), range)];
        }

        let Ok(module) = parse_module_as_syntax_output(&document.text) else {
            return Vec::new();
        };
        Self::imported_definition_location(uri, &module, &identifier)
            .into_iter()
            .collect()
    }

    /// Resolves an imported receiver field such as `user.name`.
    ///
    /// Inputs:
    /// - `uri`: current source document URI.
    /// - `module`: parsed current source module.
    /// - `text`: current source text.
    /// - `field_start`: byte offset where the field identifier starts.
    /// - `field_name`: receiver field identifier under the cursor.
    ///
    /// Output:
    /// - Provider struct-field location when the receiver has an imported type
    ///   annotation and the provider interface marks the field public.
    ///
    /// Transformation:
    /// - Keeps imported field navigation conservative by requiring an explicit
    ///   receiver parameter annotation and selected import before reading the
    ///   provider source/interface symbol tree.
    fn imported_receiver_field_definition_location(
        uri: &Url,
        module: &SyntaxModuleOutput,
        text: &str,
        field_start: usize,
        field_name: &str,
    ) -> Option<Location> {
        let receiver_name = Self::receiver_identifier_before_member(text, field_start)?;
        let type_name = Self::parameter_type_for_name(module, &receiver_name)?;
        let (provider_module, provider_type) = Self::selected_import_for_type(module, &type_name)?;
        let interfaces = OpenDocuments::interfaces_for_uri(uri);
        let fields = interfaces
            .get(&provider_module)?
            .struct_fields
            .get(&provider_type)?;
        if !fields
            .iter()
            .any(|field| field.name == field_name && !field.is_private)
        {
            return None;
        }
        let current_path = uri.to_file_path().ok()?;
        let current_dir = current_path.parent()?;
        Self::provider_struct_field_definition_location(
            current_dir,
            &provider_module,
            &provider_type,
            field_name,
        )
    }

    /// Finds the receiver identifier immediately before a dotted member.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `member_start`: byte offset where the member identifier starts.
    ///
    /// Output:
    /// - Receiver identifier for `receiver.member`.
    ///
    /// Transformation:
    /// - Performs a small lexical walk over ASCII identifiers, matching the
    ///   existing LSP identifier policy without invoking typechecking.
    fn receiver_identifier_before_member(text: &str, member_start: usize) -> Option<String> {
        let bytes = text.as_bytes();
        let mut cursor = member_start;
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        if cursor == 0 || bytes[cursor - 1] != b'.' {
            return None;
        }
        cursor -= 1;
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        let end = cursor;
        while cursor > 0 && Self::is_identifier_byte(bytes[cursor - 1]) {
            cursor -= 1;
        }
        (cursor < end).then(|| text[cursor..end].to_string())
    }

    /// Finds a top-level parameter type annotation by parameter name.
    ///
    /// Inputs:
    /// - `module`: parsed source module.
    /// - `name`: parameter or receiver name.
    ///
    /// Output:
    /// - Annotation text for the first matching function/method parameter.
    ///
    /// Transformation:
    /// - Uses syntax-output parameter metadata, which already preserves source
    ///   type spelling for editor features.
    fn parameter_type_for_name(module: &SyntaxModuleOutput, name: &str) -> Option<String> {
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Function { params, .. } => {
                    if let Some(param) = params.iter().find(|param| param.name == name) {
                        return Some(Self::base_type_name(&param.annotation.text).to_string());
                    }
                }
                SyntaxDeclarationPayload::Method {
                    receiver, params, ..
                } => {
                    if receiver.name == name {
                        return Some(Self::base_type_name(&receiver.annotation.text).to_string());
                    }
                    if let Some(param) = params.iter().find(|param| param.name == name) {
                        return Some(Self::base_type_name(&param.annotation.text).to_string());
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Resolves a local type name to the selected provider import that exposes it.
    ///
    /// Inputs:
    /// - `module`: parsed source module.
    /// - `type_name`: local type name from an annotation.
    ///
    /// Output:
    /// - Provider module and provider-side type name.
    ///
    /// Transformation:
    /// - Honors aliases and wildcard selected imports while rejecting ambiguous
    ///   candidates, matching normal imported definition behavior.
    fn selected_import_for_type(
        module: &SyntaxModuleOutput,
        type_name: &str,
    ) -> Option<(String, String)> {
        let mut candidates = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                module_name,
                items,
                is_selected,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !is_selected {
                continue;
            }
            for item in items {
                if item.name == "*" && item.as_alias.is_none() {
                    candidates.push((module_name.clone(), type_name.to_string()));
                    continue;
                }
                if item.as_alias.as_deref().unwrap_or(&item.name) == type_name {
                    candidates.push((module_name.clone(), item.name.clone()));
                }
            }
        }
        (candidates.len() == 1).then(|| candidates.remove(0))
    }

    /// Resolves a selected imported symbol to its provider declaration.
    ///
    /// Inputs:
    /// - `uri`: current source document URI, used to discover sibling
    ///   interface/source files.
    /// - `module`: parsed current document.
    /// - `identifier`: symbol under the editor cursor.
    ///
    /// Output:
    /// - Provider location when the import is selected, the interface confirms
    ///   the public symbol, and a provider file can be parsed.
    /// - `None` for private, ambiguous, missing, or unavailable provider paths.
    ///
    /// Transformation:
    /// - Keeps cross-file navigation tied to generated interface visibility
    ///   rather than guessing from arbitrary source files.
    fn imported_definition_location(
        uri: &Url,
        module: &SyntaxModuleOutput,
        identifier: &str,
    ) -> Option<Location> {
        let interfaces = OpenDocuments::interfaces_for_uri(uri);
        let current_path = uri.to_file_path().ok()?;
        let current_dir = current_path.parent()?;

        let mut resolved_locations = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                module_name,
                items,
                is_selected,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !is_selected {
                continue;
            }
            let Some(imported_name) = items.iter().find_map(|item| {
                if item.as_alias.as_deref().unwrap_or(&item.name) == identifier {
                    return Some(item.name.as_str());
                }
                (item.name == "*" && item.as_alias.is_none()).then_some(identifier)
            }) else {
                continue;
            };
            let interface_exports = interfaces.get(module_name).is_some_and(|interface| {
                Self::interface_exports_identifier(interface, imported_name)
            });
            if !interface_exports
                && !Self::provider_exports_identifier(current_dir, module_name, imported_name)
            {
                continue;
            }
            if let Some(location) =
                Self::provider_definition_location(current_dir, module_name, imported_name)
            {
                resolved_locations.push(location);
            }
        }

        (resolved_locations.len() == 1).then(|| resolved_locations.remove(0))
    }

    /// Reports whether an imported symbol is public in a provider interface.
    ///
    /// Inputs:
    /// - `interface`: loaded provider interface.
    /// - `name`: imported source symbol name before local aliasing.
    ///
    /// Output:
    /// - `true` when the public interface exports a function, type, shape,
    ///   trait, or constructor with the requested name.
    ///
    /// Transformation:
    /// - Uses resolver-owned interface metadata as the visibility gate for LSP
    ///   cross-file definition links.
    fn interface_exports_identifier(
        interface: &crate::terlan_hir::ModuleInterface,
        name: &str,
    ) -> bool {
        interface.public_types.contains(name)
            || interface.shapes.contains_key(name)
            || interface.traits.contains_key(name)
            || interface.constructors.contains_key(name)
            || interface
                .functions
                .keys()
                .any(|(function_name, _arity)| function_name == name)
            || interface
                .function_overloads
                .keys()
                .any(|(function_name, _arity)| function_name == name)
    }

    /// Reports whether provider artifacts visibly export an identifier.
    ///
    /// Inputs:
    /// - `current_dir`: directory containing the consumer document.
    /// - `module_name`: imported provider module.
    /// - `identifier`: imported source symbol name before local aliasing.
    ///
    /// Output:
    /// - `true` when a provider source/summary has a public declaration or an
    ///   interface-only export summary for the identifier.
    ///
    /// Transformation:
    /// - Supplements generated HIR interfaces for editor navigation, allowing
    ///   wrapper summaries that only re-export selected imports while still
    ///   refusing private source declarations.
    fn provider_exports_identifier(
        current_dir: &Path,
        module_name: &str,
        identifier: &str,
    ) -> bool {
        Self::provider_definition_candidates(current_dir, module_name)
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| {
                parse_module_as_syntax_output(&text)
                    .or_else(|_| {
                        crate::terlan_syntax::parse_interface_module_as_syntax_output(&text)
                    })
                    .ok()
            })
            .any(|module| {
                module.declarations.iter().any(|declaration| {
                    Self::public_declaration_exports_identifier(declaration, identifier)
                })
            })
    }

    /// Reports whether one provider declaration exposes an identifier publicly.
    ///
    /// Inputs:
    /// - `declaration`: parsed provider declaration.
    /// - `identifier`: name requested by editor navigation.
    ///
    /// Output:
    /// - `true` for public declarations and interface export summaries matching
    ///   `identifier`.
    ///
    /// Transformation:
    /// - Encodes only editor visibility, not typechecking semantics.
    fn public_declaration_exports_identifier(
        declaration: &crate::terlan_syntax::SyntaxDeclarationOutput,
        identifier: &str,
    ) -> bool {
        match &declaration.payload {
            SyntaxDeclarationPayload::Export { items } => {
                items.iter().any(|item| item.name == identifier)
            }
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                is_opaque,
                ..
            } => (*is_public || *is_opaque) && name == identifier,
            SyntaxDeclarationPayload::Struct {
                name, is_public, ..
            }
            | SyntaxDeclarationPayload::Trait {
                name, is_public, ..
            }
            | SyntaxDeclarationPayload::Function {
                name, is_public, ..
            }
            | SyntaxDeclarationPayload::Constructor {
                name, is_public, ..
            } => *is_public && name == identifier,
            _ => false,
        }
    }

    /// Finds the source/interface file and declaration range for a provider.
    ///
    /// Inputs:
    /// - `current_dir`: directory containing the consumer document.
    /// - `module_name`: imported module name.
    /// - `identifier`: provider symbol name.
    ///
    /// Output:
    /// - LSP location inside the provider file when it can be parsed and the
    ///   symbol exists.
    ///
    /// Transformation:
    /// - Checks source-like and generated-summary file names in deterministic
    ///   order, then reuses the same document-symbol projector used for local
    ///   definitions.
    fn provider_definition_location(
        current_dir: &Path,
        module_name: &str,
        identifier: &str,
    ) -> Option<Location> {
        Self::provider_definition_location_inner(
            current_dir,
            module_name,
            identifier,
            &mut HashSet::new(),
        )
    }

    /// Finds a public struct field inside a provider source/interface file.
    ///
    /// Inputs:
    /// - `current_dir`: directory containing the consumer document.
    /// - `module_name`: imported provider module.
    /// - `struct_name`: provider struct name.
    /// - `field_name`: public field requested by receiver navigation.
    ///
    /// Output:
    /// - LSP location for the provider field declaration.
    ///
    /// Transformation:
    /// - Parses provider candidates into document symbols and only returns a
    ///   field child under the matching struct symbol, avoiding accidental
    ///   jumps to same-named fields on unrelated structs.
    fn provider_struct_field_definition_location(
        current_dir: &Path,
        module_name: &str,
        struct_name: &str,
        field_name: &str,
    ) -> Option<Location> {
        for path in Self::provider_definition_candidates(current_dir, module_name) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let symbols = Self::provider_document_symbols_for_text(&text);
            if let Some(range) = Self::find_child_symbol_selection_range(
                &symbols,
                struct_name,
                field_name,
                SymbolKind::FIELD,
            ) {
                let uri = Url::from_file_path(path).ok()?;
                return Some(Location::new(uri, range));
            }
        }
        None
    }

    /// Finds a provider declaration range, following explicit re-export imports.
    ///
    /// Inputs:
    /// - `current_dir`: directory containing the original consumer document.
    /// - `module_name`: provider module to inspect.
    /// - `identifier`: visible symbol requested by the consumer or wrapper.
    /// - `visited`: recursion guard for cyclic summary imports.
    ///
    /// Output:
    /// - LSP location inside the first provider that declares the symbol.
    ///
    /// Transformation:
    /// - Checks provider files as before, then follows selected imports in
    ///   provider summaries such as `import base.{add}. export add/2.` so
    ///   editor navigation lands on the original public declaration.
    fn provider_definition_location_inner(
        current_dir: &Path,
        module_name: &str,
        identifier: &str,
        visited: &mut HashSet<String>,
    ) -> Option<Location> {
        let visit_key = format!("{module_name}.{identifier}");
        if !visited.insert(visit_key) {
            return None;
        }

        for path in Self::provider_definition_candidates(current_dir, module_name) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let symbols = Self::provider_document_symbols_for_text(&text);
            if let Some(range) = Self::find_symbol_selection_range(&symbols, identifier) {
                let uri = Url::from_file_path(path).ok()?;
                return Some(Location::new(uri, range));
            }
            if let Some((source_module, source_name)) =
                Self::provider_reexport_source(&text, identifier)
            {
                if let Some(location) = Self::provider_definition_location_inner(
                    current_dir,
                    &source_module,
                    &source_name,
                    visited,
                ) {
                    return Some(location);
                }
            }
        }
        None
    }

    /// Resolves a provider-summary re-export to its selected import source.
    ///
    /// Inputs:
    /// - `text`: provider source or interface summary.
    /// - `identifier`: symbol exported by the provider but not declared there.
    ///
    /// Output:
    /// - `(module, source_name)` for an explicit selected import whose local
    ///   name matches `identifier`.
    ///
    /// Transformation:
    /// - Parses source and interface forms, ignores wildcard imports, and
    ///   preserves aliases so `import base.{add as plus}. export plus/2.`
    ///   navigates to `base.add`.
    fn provider_reexport_source(text: &str, identifier: &str) -> Option<(String, String)> {
        let module = parse_module_as_syntax_output(text)
            .or_else(|_| crate::terlan_syntax::parse_interface_module_as_syntax_output(text))
            .ok()?;
        module.declarations.iter().find_map(|declaration| {
            let SyntaxDeclarationPayload::Import {
                module_name,
                items,
                is_selected,
                ..
            } = &declaration.payload
            else {
                return None;
            };
            if !is_selected {
                return None;
            }
            items.iter().find_map(|item| {
                if item.name == "*" {
                    return None;
                }
                let local_name = item.as_alias.as_deref().unwrap_or(&item.name);
                (local_name == identifier).then(|| (module_name.clone(), item.name.clone()))
            })
        })
    }

    /// Builds document symbols for source or interface provider text.
    ///
    /// Inputs:
    /// - `text`: provider source, `.terli`, or `.typi` content.
    ///
    /// Output:
    /// - LSP symbols for parseable provider declarations.
    ///
    /// Transformation:
    /// - Tries normal source parsing first, then interface-summary parsing so
    ///   bodyless public declarations can still serve editor navigation.
    fn provider_document_symbols_for_text(text: &str) -> Vec<DocumentSymbol> {
        parse_module_as_syntax_output(text)
            .or_else(|_| crate::terlan_syntax::parse_interface_module_as_syntax_output(text))
            .map(|module| vec![Self::module_document_symbol(text, &module)])
            .unwrap_or_default()
    }
}
