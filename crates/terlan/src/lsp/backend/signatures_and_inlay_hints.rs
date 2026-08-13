use super::super::*;

type InlayHintSignature<'a> = (&'a str, String, Vec<(String, Option<String>)>);

impl Backend {
    /// Builds an LSP signature help object from syntax-output parameter parts.
    ///
    /// Inputs:
    /// - `name`: callable name.
    /// - `params`: visible call parameters.
    /// - `return_type`: callable return type text.
    /// - `docs`: callable documentation lines.
    /// - `active_parameter`: zero-based active argument index.
    ///
    /// Output:
    /// - Signature help with one active signature.
    ///
    /// Transformation:
    /// - Converts syntax-output parameter metadata into standard LSP signature
    ///   and parameter labels.
    pub(in super::super) fn signature_help_from_parts(
        name: &str,
        generic_params: &[String],
        params: &[SyntaxParamOutput],
        return_type: &str,
        docs: &[String],
        active_parameter: usize,
        is_pure: bool,
    ) -> SignatureHelp {
        let parameter_labels = params
            .iter()
            .map(Self::signature_parameter_label)
            .collect::<Vec<_>>();
        let label = Self::signature_label(
            name,
            generic_params,
            &parameter_labels,
            return_type,
            is_pure,
        );
        let parameters = parameter_labels
            .iter()
            .map(|label| ParameterInformation {
                label: ParameterLabel::Simple(label.clone()),
                documentation: None,
            })
            .collect::<Vec<_>>();
        let active_parameter = active_parameter.min(params.len().saturating_sub(1)) as u32;

        SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: Self::completion_documentation(docs),
                parameters: Some(parameters),
                active_parameter: Some(active_parameter),
            }],
            active_signature: Some(0),
            active_parameter: Some(active_parameter),
        }
    }

    /// Builds an LSP signature help object from imported interface parameters.
    ///
    /// Inputs:
    /// - `name`: callable name.
    /// - `params`: visible imported call parameters.
    /// - `return_type`: callable return type text.
    /// - `docs`: callable documentation lines.
    /// - `active_parameter`: zero-based active argument index.
    ///
    /// Output:
    /// - Signature help with one active signature.
    ///
    /// Transformation:
    /// - Converts HIR summary parameter metadata into the same LSP shape used
    ///   for local syntax-output parameters.
    pub(in super::super) fn signature_help_from_interface_parts(
        name: &str,
        generic_params: &[String],
        params: &[crate::terlan_hir::ParamSignature],
        return_type: &str,
        docs: &[String],
        active_parameter: usize,
        is_pure: bool,
    ) -> SignatureHelp {
        let parameter_labels = params
            .iter()
            .map(|param| {
                let mut label = String::new();
                if param.is_mutable {
                    label.push_str("mut ");
                }
                label.push_str(&param.name);
                label.push_str(": ");
                label.push_str(&param.annotation);
                if let Some(default_text) = &param.default_text {
                    label.push_str(" = ");
                    label.push_str(default_text);
                }
                label
            })
            .collect::<Vec<_>>();
        let label = Self::signature_label(
            name,
            generic_params,
            &parameter_labels,
            return_type,
            is_pure,
        );
        let parameters = parameter_labels
            .iter()
            .map(|label| ParameterInformation {
                label: ParameterLabel::Simple(label.clone()),
                documentation: None,
            })
            .collect::<Vec<_>>();
        let active_parameter = active_parameter.min(params.len().saturating_sub(1)) as u32;

        SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: Self::completion_documentation(docs),
                parameters: Some(parameters),
                active_parameter: Some(active_parameter),
            }],
            active_signature: Some(0),
            active_parameter: Some(active_parameter),
        }
    }

    /// Formats a callable signature label.
    ///
    /// Inputs:
    /// - `name`: callable name.
    /// - `generic_params`: source generic parameters.
    /// - `parameter_labels`: already formatted visible parameters.
    /// - `return_type`: rendered return type.
    ///
    /// Output:
    /// - Signature label with generic parameters preserved when present.
    ///
    /// Transformation:
    /// - Centralizes local and imported signature rendering so editor output
    ///   cannot drift between syntax-output and generated-summary paths.
    pub(in super::super) fn signature_label(
        name: &str,
        generic_params: &[String],
        parameter_labels: &[String],
        return_type: &str,
        is_pure: bool,
    ) -> String {
        let generics = if generic_params.is_empty() {
            String::new()
        } else {
            format!("[{}]", generic_params.join(", "))
        };
        format!(
            "{}{}{}({}): {}",
            Self::pure_display_prefix(is_pure),
            name,
            generics,
            parameter_labels.join(", "),
            return_type
        )
    }

    /// Returns the editor-display prefix for pure callables.
    ///
    /// Inputs:
    /// - `is_pure`: whether the callable carries compiler-owned `@pure`
    ///   metadata.
    ///
    /// Output:
    /// - `"pure "` when pure, otherwise empty text.
    ///
    /// Transformation:
    /// - Keeps completion details and signature labels compact while preserving
    ///   the same purity metadata surfaced in hover and generated docs.
    pub(in super::super) fn pure_display_prefix(is_pure: bool) -> &'static str {
        if is_pure {
            "pure "
        } else {
            ""
        }
    }

    /// Returns whether a syntax declaration carries one exact marker annotation.
    ///
    /// Inputs:
    /// - `declaration`: parsed declaration with structured annotations.
    /// - `path`: annotation path to match.
    ///
    /// Output:
    /// - `true` when the declaration has a marker annotation whose path exactly
    ///   matches `path`.
    ///
    /// Transformation:
    /// - Uses structured annotation metadata so editor surfaces do not depend on
    ///   source whitespace or comment layout.
    pub(in super::super) fn declaration_has_marker_annotation(
        declaration: &SyntaxDeclarationOutput,
        path: &[&str],
    ) -> bool {
        declaration.annotations.iter().any(|annotation| {
            annotation.path.len() == path.len()
                && annotation
                    .path
                    .iter()
                    .map(String::as_str)
                    .zip(path.iter().copied())
                    .all(|(actual, expected)| actual == expected)
        })
    }

    /// Formats one function parameter for signature help.
    ///
    /// Inputs:
    /// - `param`: syntax-output parameter metadata.
    ///
    /// Output:
    /// - Human-readable `name: Type` label with mutability, patterns, and
    ///   defaults preserved.
    ///
    /// Transformation:
    /// - Uses compiler syntax-output metadata instead of reparsing parameter
    ///   text, keeping editor signature help aligned with generated summaries.
    pub(in super::super) fn signature_parameter_label(param: &SyntaxParamOutput) -> String {
        let mut label = String::new();
        if param.is_mutable {
            label.push_str("mut ");
        }
        label.push_str(param.pattern_text.as_deref().unwrap_or(&param.name));
        label.push_str(": ");
        label.push_str(&param.annotation.text);
        if let Some(default_text) = &param.default_text {
            label.push_str(" = ");
            label.push_str(default_text);
        }
        label
    }

    /// Finds a call context around a byte offset.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `byte_offset`: current cursor byte offset.
    ///
    /// Output:
    /// - Optional receiver name, callee name, and active argument index.
    ///
    /// Transformation:
    /// - Performs a bounded lexical walk over ASCII punctuation to locate the
    ///   nearest enclosing call without accepting nested commas as active
    ///   parameter separators.
    pub(in super::super) fn call_context_at_byte_offset(
        text: &str,
        byte_offset: usize,
    ) -> Option<(Option<String>, String, usize)> {
        let bytes = text.as_bytes();
        let mut cursor = byte_offset.min(bytes.len());
        let mut depth = 0usize;
        while cursor > 0 {
            cursor -= 1;
            match bytes[cursor] {
                b')' | b']' | b'}' => depth += 1,
                b'(' if depth == 0 => {
                    let (callee, callee_start, _) =
                        Self::identifier_before_byte_offset(text, cursor)?;
                    let receiver = if callee_start > 0 && bytes.get(callee_start - 1) == Some(&b'.')
                    {
                        Self::identifier_before_byte_offset(text, callee_start - 1)
                            .map(|(receiver, _, _)| receiver)
                    } else {
                        None
                    };
                    let active_parameter =
                        Self::active_parameter_index(bytes, cursor + 1, byte_offset);
                    return Some((receiver, callee, active_parameter));
                }
                b'(' | b'[' | b'{' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        None
    }

    /// Finds the identifier immediately before a byte offset.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `byte_offset`: offset before which an identifier may appear.
    ///
    /// Output:
    /// - Identifier text and byte bounds.
    ///
    /// Transformation:
    /// - Skips whitespace and then reuses the same ASCII identifier rules as
    ///   definition navigation.
    pub(in super::super) fn identifier_before_byte_offset(
        text: &str,
        byte_offset: usize,
    ) -> Option<(String, usize, usize)> {
        let bytes = text.as_bytes();
        let mut cursor = byte_offset.min(bytes.len());
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        if cursor == 0 {
            return None;
        }
        Self::identifier_bounds_at_byte_offset(text, cursor - 1)
    }

    /// Counts the active top-level argument index inside a call.
    ///
    /// Inputs:
    /// - `bytes`: source bytes.
    /// - `start`: byte offset after the open parenthesis.
    /// - `end`: cursor byte offset.
    ///
    /// Output:
    /// - Zero-based active parameter index.
    ///
    /// Transformation:
    /// - Counts commas only at nesting depth zero so nested calls and literals
    ///   do not shift signature help.
    pub(in super::super) fn active_parameter_index(
        bytes: &[u8],
        start: usize,
        end: usize,
    ) -> usize {
        let mut active = 0usize;
        let mut depth = 0usize;
        for byte in bytes
            .iter()
            .take(end.min(bytes.len()))
            .skip(start.min(bytes.len()))
        {
            match *byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => active += 1,
                _ => {}
            }
        }
        active
    }

    /// Builds deterministic type inlay hints for simple inferred let bindings.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    /// - `uri`: document URI used to discover generated interface summaries.
    /// - `range`: requested LSP range.
    ///
    /// Output:
    /// - Type inlay hints for supported literal bindings and parameter-name
    ///   hints for simple local function calls.
    ///
    /// Transformation:
    /// - Keeps this inlay surface intentionally conservative: it emits hints
    ///   for plain `let name = literal` bindings, immediately continued
    ///   `name = literal` let-chain bindings, and same-line local or imported
    ///   function calls that can be matched without invoking broader type
    ///   inference.
    pub(in super::super) fn inlay_hints_for_range(
        document: &OpenDocument,
        uri: &Url,
        range: Range,
    ) -> Vec<InlayHint> {
        let Ok(module) = document.parse_syntax() else {
            return Vec::new();
        };
        let interfaces = OpenDocuments::interfaces_for_uri(uri);

        let mut hints = Vec::new();
        let mut line_start_byte = 0usize;
        let mut in_let_chain = false;
        for (line_index, line) in document.text.lines().enumerate() {
            let Some(line_number) = u32::try_from(line_index).ok() else {
                continue;
            };
            let let_binding = Self::simple_let_binding_for_line(line);
            let chain_binding = if let_binding.is_none() && in_let_chain {
                Self::simple_assignment_binding_for_line(line)
            } else {
                None
            };
            if line_number < range.start.line || line_number > range.end.line {
                if let_binding.is_some() || chain_binding.is_some() {
                    in_let_chain = line.trim_end().ends_with(';');
                } else {
                    in_let_chain = false;
                }
                line_start_byte += line.len() + 1;
                continue;
            }
            if let Some(hint) = Self::let_literal_inlay_hint_for_line(line, line_number) {
                hints.push(hint);
            }
            if let Some(hint) = Self::assignment_literal_inlay_hint_for_line(line, line_number) {
                if chain_binding.is_some() {
                    hints.push(hint);
                }
            }
            hints.extend(Self::parameter_inlay_hints_for_line(
                &module,
                &interfaces,
                line,
                line_number,
            ));
            hints.extend(Self::receiver_parameter_inlay_hints_for_line(
                &module,
                &interfaces,
                line,
                line_number,
                line_start_byte,
            ));
            if let_binding.is_some() || chain_binding.is_some() {
                in_let_chain = line.trim_end().ends_with(';');
            } else {
                in_let_chain = false;
            }
            line_start_byte += line.len() + 1;
        }
        hints
    }

    /// Builds parameter-name inlay hints for simple function calls.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `interfaces`: generated provider summaries visible to the document.
    /// - `line`: source line to inspect.
    /// - `line_number`: zero-based LSP line number.
    ///
    /// Output:
    /// - Parameter hints for same-line local and imported function calls.
    ///
    /// Transformation:
    /// - Matches function names followed by `(`, skips declaration-like lines,
    ///   and labels top-level argument starts with parameter names.
    pub(in super::super) fn parameter_inlay_hints_for_line(
        module: &SyntaxModuleOutput,
        interfaces: &std::collections::HashMap<String, crate::terlan_hir::ModuleInterface>,
        line: &str,
        line_number: u32,
    ) -> Vec<InlayHint> {
        let trimmed = line.trim_start();
        if trimmed.starts_with("pub ")
            || trimmed.starts_with("priv ")
            || trimmed.starts_with("constructor ")
            || trimmed.starts_with("import ")
        {
            return Vec::new();
        }

        let mut signatures: Vec<InlayHintSignature<'_>> = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Function { name, params, .. } = &declaration.payload
            else {
                continue;
            };
            if params.is_empty() {
                continue;
            }
            signatures.push((
                name.as_str(),
                name.clone(),
                params
                    .iter()
                    .map(|param| (param.name.clone(), param.default_text.clone()))
                    .collect::<Vec<_>>(),
            ));
        }
        for interface in interfaces.values() {
            for signature in Self::imported_public_function_signatures(interface) {
                if signature.params.is_empty() {
                    continue;
                }
                signatures.push((
                    signature.name.as_str(),
                    format!("{}.{}", interface.module, signature.name),
                    signature
                        .params
                        .iter()
                        .map(|param| (param.name.clone(), param.default_text.clone()))
                        .collect::<Vec<_>>(),
                ));
            }
        }

        let mut hints = Vec::new();
        for (name, tooltip_name, params) in signatures {
            hints.extend(Self::parameter_inlay_hints_for_signature(
                line,
                line_number,
                name,
                &tooltip_name,
                params.as_slice(),
            ));
        }
        hints
    }

    /// Builds parameter-name inlay hints for one function signature.
    ///
    /// Inputs:
    /// - `line`: source line to inspect.
    /// - `line_number`: zero-based LSP line number.
    /// - `name`: callable name.
    /// - `tooltip_name`: provenance-qualified callable name for tooltips.
    /// - `params`: callable parameter names and optional default values.
    ///
    /// Output:
    /// - Parameter inlay hints for matching same-line calls.
    ///
    /// Transformation:
    /// - Reuses the same simple call scanner for local and imported function
    ///   signatures.
    pub(in super::super) fn parameter_inlay_hints_for_signature(
        line: &str,
        line_number: u32,
        name: &str,
        tooltip_name: &str,
        params: &[(String, Option<String>)],
    ) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        let needle = format!("{name}(");
        for (start, _) in line.match_indices(&needle) {
            if start > 0 {
                let previous = line.as_bytes()[start - 1];
                if Self::is_identifier_byte(previous) || previous == b'.' {
                    continue;
                }
            }
            let open_paren = start + name.len();
            let Some(close_paren) = Self::same_line_call_close(line, open_paren) else {
                continue;
            };
            let argument_positions =
                Self::simple_call_argument_positions(line, open_paren, close_paren);
            for (index, character) in argument_positions.iter().copied().enumerate() {
                let Some((param, _)) = params.get(index) else {
                    continue;
                };
                let Some(character) = u32::try_from(character).ok() else {
                    continue;
                };
                hints.push(InlayHint {
                    position: Position::new(line_number, character),
                    label: InlayHintLabel::String(format!("{param}:")),
                    kind: Some(InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(format!(
                        "Parameter `{param}` for `{tooltip_name}`."
                    ))),
                    padding_left: Some(false),
                    padding_right: Some(true),
                    data: None,
                });
            }
            hints.extend(Self::defaulted_parameter_inlay_hints(
                line_number,
                close_paren,
                tooltip_name,
                params,
                argument_positions.len(),
            ));
        }
        hints
    }

    /// Builds a trailing inlay hint for omitted defaulted arguments.
    ///
    /// Inputs:
    /// - `line_number`: zero-based LSP line number.
    /// - `close_paren`: byte offset of the call's closing parenthesis.
    /// - `name`: callable name.
    /// - `params`: callable parameter names and optional default values.
    /// - `provided_count`: number of explicit arguments supplied by the call.
    ///
    /// Output:
    /// - One compact parameter inlay hint when every omitted parameter has a
    ///   default; otherwise no hint.
    ///
    /// Transformation:
    /// - Converts compiler summary default values into editor-visible call-site
    ///   context without rewriting the user's source.
    pub(in super::super) fn defaulted_parameter_inlay_hints(
        line_number: u32,
        close_paren: usize,
        name: &str,
        params: &[(String, Option<String>)],
        provided_count: usize,
    ) -> Vec<InlayHint> {
        if provided_count >= params.len() {
            return Vec::new();
        }
        let omitted = params[provided_count..]
            .iter()
            .map(|(param, default_text)| default_text.as_ref().map(|value| (param, value)))
            .collect::<Option<Vec<_>>>();
        let Some(omitted) = omitted else {
            return Vec::new();
        };
        if omitted.is_empty() {
            return Vec::new();
        }
        let Some(character) = u32::try_from(close_paren).ok() else {
            return Vec::new();
        };
        let label = omitted
            .iter()
            .map(|(param, value)| format!("{param} = {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        vec![InlayHint {
            position: Position::new(line_number, character),
            label: InlayHintLabel::String(label),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(format!(
                "Defaulted parameter for `{name}`."
            ))),
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        }]
    }

    /// Builds parameter-name inlay hints for receiver-method calls.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `interfaces`: generated provider summaries visible to the document.
    /// - `line`: source line to inspect.
    /// - `line_number`: zero-based LSP line number.
    /// - `line_start_byte`: byte offset of `line` in the full source text.
    ///
    /// Output:
    /// - Parameter inlay hints for matching local or imported receiver-method
    ///   calls on the line.
    ///
    /// Transformation:
    /// - Uses the receiver identifier and active function parameter type to
    ///   choose method metadata, then labels explicit dotted-call arguments.
    pub(in super::super) fn receiver_parameter_inlay_hints_for_line(
        module: &SyntaxModuleOutput,
        interfaces: &std::collections::HashMap<String, crate::terlan_hir::ModuleInterface>,
        line: &str,
        line_number: u32,
        line_start_byte: usize,
    ) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        for (dot_offset, _) in line.match_indices('.') {
            let Some((receiver, _, _)) = Self::identifier_before_byte_offset(line, dot_offset)
            else {
                continue;
            };
            let Some((method, _, method_end)) =
                Self::identifier_bounds_at_byte_offset(line, dot_offset + 1)
            else {
                continue;
            };
            if line.as_bytes().get(method_end) != Some(&b'(') {
                continue;
            }
            let source_offset = line_start_byte + dot_offset;
            let Some(receiver_type) = Self::active_parameter_type(module, source_offset, &receiver)
            else {
                continue;
            };
            let type_name = Self::base_type_name(&receiver_type);
            let Some(close_paren) = Self::same_line_call_close(line, method_end) else {
                continue;
            };
            for params in
                Self::receiver_method_parameter_names(module, interfaces, type_name, &method)
            {
                hints.extend(Self::parameter_inlay_hints_for_call_arguments(
                    line,
                    line_number,
                    method_end,
                    close_paren,
                    &method,
                    &params,
                ));
            }
        }
        hints
    }

    /// Collects parameter names for receiver-method inlay hints.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `interfaces`: generated provider summaries visible to the document.
    /// - `type_name`: receiver base type name.
    /// - `method`: receiver-method name.
    ///
    /// Output:
    /// - Parameter-name lists for matching local or imported receiver methods,
    ///   excluding the receiver itself.
    ///
    /// Transformation:
    /// - Merges local receiver declarations, explicit impl methods, and
    ///   imported public receiver-method summaries.
    pub(in super::super) fn receiver_method_parameter_names(
        module: &SyntaxModuleOutput,
        interfaces: &std::collections::HashMap<String, crate::terlan_hir::ModuleInterface>,
        type_name: &str,
        method: &str,
    ) -> Vec<Vec<String>> {
        let mut parameter_sets = Vec::new();
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Method {
                    receiver,
                    name,
                    params,
                    ..
                } if name == method
                    && Self::base_type_name(&receiver.annotation.text) == type_name =>
                {
                    parameter_sets.push(params.iter().map(|param| param.name.clone()).collect());
                }
                SyntaxDeclarationPayload::TraitImpl {
                    for_type, methods, ..
                } if Self::base_type_name(&for_type.text) == type_name => {
                    for impl_method in methods
                        .iter()
                        .filter(|impl_method| impl_method.name == method)
                    {
                        let Some(receiver) = impl_method.params.first() else {
                            continue;
                        };
                        if Self::base_type_name(&receiver.annotation.text) != type_name {
                            continue;
                        }
                        parameter_sets.push(
                            impl_method.params[1..]
                                .iter()
                                .map(|param| param.name.clone())
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        }
        for interface in interfaces.values() {
            for overloads in interface.function_overloads.values() {
                for signature in overloads.iter().filter(|signature| {
                    signature.public
                        && signature.receiver_method
                        && signature.name == method
                        && signature
                            .params
                            .first()
                            .map(|param| Self::base_type_name(&param.annotation) == type_name)
                            .unwrap_or(false)
                }) {
                    parameter_sets.push(
                        signature.params[1..]
                            .iter()
                            .map(|param| param.name.clone())
                            .collect(),
                    );
                }
            }
        }
        parameter_sets.sort();
        parameter_sets.dedup();
        parameter_sets
    }

    /// Builds parameter-name inlay hints for a known call argument span.
    ///
    /// Inputs:
    /// - `line`: source line to inspect.
    /// - `line_number`: zero-based LSP line number.
    /// - `open_paren`: byte offset of the opening parenthesis.
    /// - `close_paren`: byte offset of the matching closing parenthesis.
    /// - `name`: callable name for tooltips.
    /// - `params`: visible call parameter names.
    ///
    /// Output:
    /// - Parameter inlay hints for top-level arguments in the call.
    ///
    /// Transformation:
    /// - Projects argument start offsets into LSP parameter inlay hints.
    pub(in super::super) fn parameter_inlay_hints_for_call_arguments(
        line: &str,
        line_number: u32,
        open_paren: usize,
        close_paren: usize,
        name: &str,
        params: &[String],
    ) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        for (index, character) in
            Self::simple_call_argument_positions(line, open_paren, close_paren)
                .into_iter()
                .enumerate()
        {
            let Some(param) = params.get(index) else {
                continue;
            };
            let Some(character) = u32::try_from(character).ok() else {
                continue;
            };
            hints.push(InlayHint {
                position: Position::new(line_number, character),
                label: InlayHintLabel::String(format!("{param}:")),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(format!(
                    "Parameter `{param}` for `{name}`."
                ))),
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
        hints
    }

    /// Finds a same-line call's closing parenthesis.
    ///
    /// Inputs:
    /// - `line`: source line.
    /// - `open_paren`: byte offset of the opening parenthesis.
    ///
    /// Output:
    /// - Byte offset of the matching closing parenthesis.
    ///
    /// Transformation:
    /// - Tracks bracket nesting to avoid stopping at nested call delimiters.
    pub(in super::super) fn same_line_call_close(line: &str, open_paren: usize) -> Option<usize> {
        let bytes = line.as_bytes();
        if bytes.get(open_paren) != Some(&b'(') {
            return None;
        }
        let mut depth = 0usize;
        for (offset, byte) in bytes.iter().enumerate().skip(open_paren) {
            match *byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(offset);
                    }
                }
                _ => {}
            }
        }
        None
    }
}
