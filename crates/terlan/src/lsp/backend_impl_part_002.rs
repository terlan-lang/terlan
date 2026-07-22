impl Backend {
    /// Builds field completion items from local struct declarations.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `type_name`: struct type name to inspect.
    ///
    /// Output:
    /// - Public field completion items for the local struct.
    ///
    /// Transformation:
    /// - Reads structured syntax-output fields and ignores private fields.
    fn local_struct_field_completion_items(
        module: &SyntaxModuleOutput,
        type_name: &str,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Struct { name, fields, .. } = &declaration.payload else {
                continue;
            };
            if name != type_name {
                continue;
            }
            for field in fields.iter().filter(|field| !field.is_private) {
                items.push(Self::field_completion_item(
                    field.name.clone(),
                    format!(
                        "field {type_name}.{}: {}",
                        field.name, field.annotation.text
                    ),
                ));
            }
        }
        items
    }

    /// Builds one field completion item.
    ///
    /// Inputs:
    /// - `label`: field name.
    /// - `detail`: visible field type summary.
    ///
    /// Output:
    /// - LSP completion item using the field icon kind.
    ///
    /// Transformation:
    /// - Keeps field suggestions distinct from values and functions in editor
    ///   ranking and display.
    fn field_completion_item(label: String, detail: String) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(detail),
            insert_text: Some(label),
            ..Default::default()
        }
    }

    /// Builds one method completion item.
    ///
    /// Inputs:
    /// - `label`: method name.
    /// - `detail`: visible method signature summary.
    /// - `docs`: documentation comments attached to the method declaration.
    ///
    /// Output:
    /// - LSP completion item using the method icon kind.
    ///
    /// Transformation:
    /// - Keeps dotted-call method suggestions distinct from fields, functions,
    ///   and variables in editor ranking and display.
    fn method_completion_item(label: String, detail: String, docs: &[String]) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(format!("{label}()")),
            ..Default::default()
        }
    }

    /// Returns public constructor signatures from an imported interface.
    ///
    /// Inputs:
    /// - `interface`: generated provider summary loaded for the current file.
    ///
    /// Output:
    /// - Public constructor signatures in deterministic order.
    ///
    /// Transformation:
    /// - Uses resolver-owned interface constructor metadata so completion obeys
    ///   public/private provider boundaries.
    fn imported_public_constructor_signatures(
        interface: &crate::terlan_hir::ModuleInterface,
    ) -> Vec<&ConstructorSignature> {
        let mut constructors = interface
            .constructors
            .values()
            .flat_map(|signatures| signatures.iter())
            .filter(|signature| signature.public)
            .collect::<Vec<_>>();
        constructors.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.params.len().cmp(&right.params.len()))
                .then_with(|| left.return_type.cmp(&right.return_type))
        });
        constructors
    }

    /// Builds local variable and parameter completion items.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `document`: current open document.
    /// - `byte_offset`: cursor byte offset.
    ///
    /// Output:
    /// - Completion items for active function parameters and simple prior
    ///   let-bindings.
    ///
    /// Transformation:
    /// - Uses declaration spans for parameter scope and source-order lexical
    ///   scanning only for plain `let name = literal` bindings before the
    ///   cursor.
    fn local_symbol_completion_items(
        module: &SyntaxModuleOutput,
        document: &OpenDocument,
        byte_offset: usize,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Function { params, .. } = &declaration.payload else {
                continue;
            };
            if byte_offset < declaration.span.start || byte_offset > declaration.span.end {
                continue;
            }
            for param in params {
                items.push(Self::variable_completion_item(
                    param.name.clone(),
                    format!("parameter {}", param.annotation.text),
                ));
            }
        }

        for (name, type_name) in
            Self::simple_let_bindings_before_offset(&document.text, byte_offset)
        {
            let detail = type_name
                .map(|type_name| format!("local {type_name}"))
                .unwrap_or_else(|| "local".to_string());
            items.push(Self::variable_completion_item(name, detail));
        }

        items
    }

    /// Builds one local variable completion item.
    ///
    /// Inputs:
    /// - `label`: local binding or parameter name.
    /// - `detail`: visible source of the local symbol.
    ///
    /// Output:
    /// - LSP completion item using a variable icon kind.
    ///
    /// Transformation:
    /// - Keeps local values distinct from callable completions so editors can
    ///   rank and display them without inventing Terlan-specific metadata.
    fn variable_completion_item(label: String, detail: String) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(detail),
            insert_text: Some(label),
            ..Default::default()
        }
    }

    /// Builds one type-like completion item.
    ///
    /// Inputs:
    /// - `label`: completed type, struct, or trait name.
    /// - `detail`: visible metadata summary.
    /// - `kind`: LSP item kind appropriate for the declaration.
    /// - `docs`: documentation lines for completion popovers.
    ///
    /// Output:
    /// - LSP completion item for type-level names.
    ///
    /// Transformation:
    /// - Uses syntax-output and interface metadata to expose the type namespace
    ///   without relying on source scraping or target-specific editor code.
    fn type_completion_item(
        label: String,
        detail: String,
        kind: CompletionItemKind,
        docs: &[String],
    ) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(kind),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(label),
            ..Default::default()
        }
    }

    /// Collects simple let-bindings visible before a cursor.
    ///
    /// Inputs:
    /// - `text`: source text.
    /// - `byte_offset`: cursor byte offset.
    ///
    /// Output:
    /// - Binding names with optional literal-inferred type names.
    ///
    /// Transformation:
    /// - Walks complete source lines before the cursor and recognizes plain
    ///   `let name = ...` bindings plus immediately continued `name = ...`
    ///   let-chain bindings, avoiding destructuring and incomplete current-line
    ///   edits.
    fn simple_let_bindings_before_offset(
        text: &str,
        byte_offset: usize,
    ) -> Vec<(String, Option<&'static str>)> {
        let mut bindings = Vec::new();
        let mut line_start = 0usize;
        let mut in_let_chain = false;
        for line in text.split_inclusive('\n') {
            let line_end = line_start + line.len();
            if line_end > byte_offset {
                break;
            }
            let line_without_newline = line.trim_end_matches(['\r', '\n']);
            if let Some((name, type_name)) = Self::simple_let_binding_for_line(line_without_newline)
            {
                bindings.push((name, type_name));
                in_let_chain = line_without_newline.trim_end().ends_with(';');
                line_start = line_end;
                continue;
            }
            if in_let_chain {
                if let Some((name, type_name)) =
                    Self::simple_assignment_binding_for_line(line_without_newline)
                {
                    bindings.push((name, type_name));
                    in_let_chain = line_without_newline.trim_end().ends_with(';');
                    line_start = line_end;
                    continue;
                }
            }
            in_let_chain = false;
            line_start = line_end;
        }
        bindings
    }

    /// Parses one simple let-binding line.
    ///
    /// Inputs:
    /// - `line`: one source line without trailing newline.
    ///
    /// Output:
    /// - Binding name and optional literal-inferred type name.
    ///
    /// Transformation:
    /// - Recognizes only `let name = expression` with an identifier binding and
    ///   delegates literal type classification to the inlay-hint helper.
    fn simple_let_binding_for_line(line: &str) -> Option<(String, Option<&'static str>)> {
        let trimmed = line.trim_start();
        let after_let = trimmed.strip_prefix("let ")?;
        Self::simple_assignment_binding_after_prefix(after_let)
    }

    /// Parses one continued let-chain assignment line.
    ///
    /// Inputs:
    /// - `line`: one source line without trailing newline.
    ///
    /// Output:
    /// - Binding name and optional literal-inferred type name.
    ///
    /// Transformation:
    /// - Recognizes only `name = expression` after a preceding semicolon-continued
    ///   let binding, keeping non-let expression lines out of completion scope.
    fn simple_assignment_binding_for_line(line: &str) -> Option<(String, Option<&'static str>)> {
        Self::simple_assignment_binding_after_prefix(line.trim_start())
    }

    /// Parses one simple assignment binding tail.
    ///
    /// Inputs:
    /// - `after_prefix`: source text beginning with a candidate identifier.
    ///
    /// Output:
    /// - Binding name and optional literal-inferred type name.
    ///
    /// Transformation:
    /// - Shares the conservative identifier/equal/literal parsing between
    ///   initial `let` bindings and continued let-chain bindings.
    fn simple_assignment_binding_after_prefix(
        after_prefix: &str,
    ) -> Option<(String, Option<&'static str>)> {
        let name_end = after_prefix
            .bytes()
            .position(|byte| !Self::is_identifier_byte(byte))?;
        let name = &after_prefix[..name_end];
        if name.is_empty() {
            return None;
        }
        let after_name = after_prefix[name_end..].trim_start();
        if !after_name.starts_with('=') {
            return None;
        }
        let value = after_name[1..]
            .trim_start()
            .trim_end_matches(';')
            .trim_end_matches('.')
            .trim();
        Some((name.to_string(), Self::literal_type_name(value)))
    }

    /// Returns public function signatures from an imported interface.
    ///
    /// Inputs:
    /// - `interface`: generated provider summary loaded for the current file.
    ///
    /// Output:
    /// - Public function signatures in deterministic order.
    ///
    /// Transformation:
    /// - Reads resolver-owned interface metadata, preserving visibility and
    ///   overload information instead of scraping provider source text.
    fn imported_public_function_signatures(
        interface: &crate::terlan_hir::ModuleInterface,
    ) -> Vec<&FunctionSignature> {
        let mut functions = interface
            .function_overloads
            .values()
            .flat_map(|signatures| signatures.iter())
            .filter(|signature| signature.public && !signature.receiver_method)
            .collect::<Vec<_>>();
        functions.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.params.len().cmp(&right.params.len()))
                .then_with(|| left.return_type.cmp(&right.return_type))
        });
        functions
    }

    /// Builds one function completion item.
    ///
    /// Inputs:
    /// - `label`: completed function name.
    /// - `detail`: visible signature summary.
    /// - `docs`: documentation lines for completion popovers.
    ///
    /// Output:
    /// - LSP completion item using a function icon kind.
    ///
    /// Transformation:
    /// - Converts local syntax-output or imported interface metadata into the
    ///   same editor completion representation.
    fn function_completion_item(label: String, detail: String, docs: &[String]) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(format!("{label}()")),
            ..Default::default()
        }
    }

    fn constant_completion_item(label: String, detail: String, docs: &[String]) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::CONSTANT),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(label),
            ..Default::default()
        }
    }

    /// Builds one constructor completion item.
    ///
    /// Inputs:
    /// - `label`: completed constructor name.
    /// - `detail`: visible constructor signature summary.
    /// - `docs`: documentation lines for completion popovers.
    ///
    /// Output:
    /// - LSP completion item using the constructor icon kind.
    ///
    /// Transformation:
    /// - Converts local syntax-output or imported interface constructor
    ///   metadata into the same editor completion representation.
    fn constructor_completion_item(
        label: String,
        detail: String,
        docs: &[String],
    ) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::CONSTRUCTOR),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(format!("{label}()")),
            ..Default::default()
        }
    }

    /// Builds one shape completion item.
    ///
    /// Inputs:
    /// - `label`: inserted/completed shape name.
    /// - `detail`: visible completion detail.
    /// - `docs`: documentation lines for hover/detail popovers.
    ///
    /// Output:
    /// - LSP completion item using a type-like icon kind.
    ///
    /// Transformation:
    /// - Converts compiler shape metadata into protocol-neutral editor
    ///   completion data.
    fn shape_completion_item(label: String, detail: String, docs: &[String]) -> CompletionItem {
        CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(detail),
            documentation: Self::completion_documentation(docs),
            insert_text: Some(label),
            ..Default::default()
        }
    }

    /// Converts documentation lines into completion documentation.
    ///
    /// Inputs:
    /// - `docs`: syntax-output or summary documentation lines.
    ///
    /// Output:
    /// - Markdown completion documentation, or `None` when undocumented.
    ///
    /// Transformation:
    /// - Reuses the LSP markdown channel instead of custom protocol fields.
    fn completion_documentation(docs: &[String]) -> Option<Documentation> {
        (!docs.is_empty()).then(|| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: docs.join("\n"),
            })
        })
    }

    /// Returns imported module names visible to completion.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    ///
    /// Output:
    /// - Imported module names in source order.
    ///
    /// Transformation:
    /// - Reads only import declarations; selected import item validation remains
    ///   owned by resolver/typecheck.
    fn completion_imported_modules(module: &SyntaxModuleOutput) -> Vec<String> {
        module
            .declarations
            .iter()
            .filter_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Import { module_name, .. } => Some(module_name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Returns imported module names after compiler-owned target inference.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    ///
    /// Output:
    /// - Source-order imported modules that may contribute completions.
    ///
    /// Transformation:
    /// - Reuses target-profile inference so mixed target-family evidence cannot
    ///   leak JS/VM/Wasm/native std completions into an incompatible editor
    ///   context. Non-target package imports remain visible so local workspace
    ///   completions do not disappear because of an unrelated profile conflict.
    fn target_compatible_completion_imported_modules(module: &SyntaxModuleOutput) -> Vec<String> {
        let imported_modules = Self::completion_imported_modules(module);
        let target_conflict = Self::completion_has_target_profile_conflict(&imported_modules);
        if !target_conflict {
            return imported_modules;
        }

        imported_modules
            .into_iter()
            .filter(|module_name| !Self::is_target_specific_std_import(module_name))
            .collect()
    }

    /// Returns whether imported std modules imply incompatible target families.
    ///
    /// Inputs:
    /// - `imported_modules`: source-order module imports.
    ///
    /// Output:
    /// - `true` when completions would mix mutually exclusive target-specific
    ///   std surfaces.
    ///
    /// Transformation:
    /// - Keeps LSP completion conservative for the same namespace families used
    ///   by target inference without linking the whole validation crate into the
    ///   standalone language-server binary.
    fn completion_has_target_profile_conflict(imported_modules: &[String]) -> bool {
        let mut family: Option<&'static str> = None;
        let mut js_browser = false;
        let mut js_worker = false;
        for module_name in imported_modules {
            let Some(next_family) = Self::completion_target_family(module_name) else {
                continue;
            };
            if let Some(current) = family {
                if current != next_family {
                    return true;
                }
            } else {
                family = Some(next_family);
            }
            js_browser |= module_name == "std.js.Dom" || module_name.starts_with("std.js.Dom.");
            js_worker |=
                module_name == "std.js.Worker" || module_name.starts_with("std.js.Worker.");
            if js_browser && js_worker {
                return true;
            }
        }
        false
    }

    /// Returns the coarse target family implied by a std import.
    ///
    /// Inputs:
    /// - `module_name`: fully qualified module import.
    ///
    /// Output:
    /// - Coarse target family id for target-specific std imports.
    ///
    /// Transformation:
    /// - Groups std namespaces only for editor suppression during conflicts;
    ///   backend-capability validation remains owned by target-profile checks.
    fn completion_target_family(module_name: &str) -> Option<&'static str> {
        if module_name == "std.js" || module_name.starts_with("std.js.") {
            Some("js")
        } else if module_name == "std.wasm" || module_name.starts_with("std.wasm.") {
            Some("wasm")
        } else if module_name == "std.vm"
            || module_name.starts_with("std.vm.")
            || module_name == "std.native"
            || module_name.starts_with("std.native.")
            || module_name == "std.db"
            || module_name.starts_with("std.db.")
        {
            Some("vm")
        } else {
            None
        }
    }

    /// Returns whether an import belongs to a target-owned std family.
    ///
    /// Inputs:
    /// - `module_name`: fully qualified module import.
    ///
    /// Output:
    /// - `true` for std families whose availability is selected by target
    ///   inference.
    ///
    /// Transformation:
    /// - Mirrors the inference namespace boundaries without duplicating the
    ///   profile-selection algorithm itself.
    fn is_target_specific_std_import(module_name: &str) -> bool {
        matches!(
            module_name,
            "std.js" | "std.wasm" | "std.vm" | "std.native" | "std.db"
        ) || module_name.starts_with("std.js.")
            || module_name.starts_with("std.wasm.")
            || module_name.starts_with("std.vm.")
            || module_name.starts_with("std.native.")
            || module_name.starts_with("std.db.")
    }

    /// Builds signature help for a local function call.
    ///
    /// Inputs:
    /// - `document`: current open-document snapshot.
    /// - `uri`: document URI used to discover generated interface summaries.
    /// - `position`: cursor position inside a call argument list.
    ///
    /// Output:
    /// - Signature help for a same-document function declaration or receiver
    ///   method.
    /// - `None` when the cursor is outside a supported call shape or parsing
    ///   fails.
    ///
    /// Transformation:
    /// - Finds the nearest open call parenthesis, resolves its callee against
    ///   syntax-output declarations or imported provider summaries, and
    ///   projects parameter metadata into standard LSP signature help.
    fn signature_help_for_position(
        document: &OpenDocument,
        uri: &Url,
        position: Position,
    ) -> Option<SignatureHelp> {
        let byte_offset = document.byte_offset_from_position(position)?;
        let (receiver, callee, active_parameter) =
            Self::call_context_at_byte_offset(&document.text, byte_offset)?;
        let module = parse_module_as_syntax_output(&document.text).ok()?;

        if let Some(receiver) = receiver {
            let receiver_type = Self::active_parameter_type(&module, byte_offset, &receiver)?;
            let type_name = Self::base_type_name(&receiver_type);
            if let Some(help) =
                Self::receiver_method_signature_help(&module, type_name, &callee, active_parameter)
            {
                return Some(help);
            }
            if let Some(help) = Self::imported_receiver_method_signature_help(
                uri,
                &module,
                type_name,
                &callee,
                active_parameter,
            ) {
                return Some(help);
            }
        }

        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Function {
                name,
                generic_params,
                params,
                return_type,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if name != &callee {
                continue;
            }

            return Some(Self::signature_help_from_parts(
                name,
                generic_params,
                params,
                &return_type.text,
                &declaration.docs,
                active_parameter,
                Self::declaration_has_marker_annotation(declaration, &["pure"]),
            ));
        }

        Self::imported_function_signature_help(uri, &module, &callee, active_parameter)
    }

    /// Builds signature help for imported top-level function calls.
    ///
    /// Inputs:
    /// - `uri`: current document URI used to locate provider summaries.
    /// - `module`: parsed source module containing imports.
    /// - `callee`: function name being called.
    /// - `active_parameter`: zero-based active argument index.
    ///
    /// Output:
    /// - Signature help for an imported public non-receiver function summary.
    ///
    /// Transformation:
    /// - Reads only imported, target-compatible module interfaces and projects
    ///   the public function overload with the smallest arity-compatible
    ///   signature into the same LSP shape as local functions.
    fn imported_function_signature_help(
        uri: &Url,
        module: &SyntaxModuleOutput,
        callee: &str,
        active_parameter: usize,
    ) -> Option<SignatureHelp> {
        let interfaces = OpenDocuments::interfaces_for_uri(uri);
        let mut signatures = Self::target_compatible_completion_imported_modules(module)
            .into_iter()
            .filter_map(|module_name| interfaces.get(&module_name))
            .flat_map(|interface| interface.function_overloads.values())
            .flat_map(|overloads| overloads.iter())
            .filter(|signature| {
                signature.public && !signature.receiver_method && signature.name == callee
            })
            .collect::<Vec<_>>();
        signatures.sort_by(|left, right| {
            left.params
                .len()
                .cmp(&right.params.len())
                .then_with(|| left.return_type.cmp(&right.return_type))
        });
        let signature = signatures.into_iter().next()?;
        Some(Self::signature_help_from_interface_parts(
            &signature.name,
            &signature.generic_params,
            &signature.params,
            &signature.return_type,
            &signature.docs,
            active_parameter,
            signature.pure,
        ))
    }

    /// Builds signature help for imported receiver method calls.
    ///
    /// Inputs:
    /// - `uri`: current document URI used to locate provider summaries.
    /// - `type_name`: receiver base type name.
    /// - `callee`: method name being called.
    /// - `active_parameter`: zero-based active dotted-call argument index.
    ///
    /// Output:
    /// - Signature help for an imported public receiver-method summary.
    ///
    /// Transformation:
    /// - Reads generated module interfaces and treats the first receiver-method
    ///   parameter as the implicit receiver, so dotted-call signature help only
    ///   shows explicit call arguments.
    fn imported_receiver_method_signature_help(
        uri: &Url,
        module: &SyntaxModuleOutput,
        type_name: &str,
        callee: &str,
        active_parameter: usize,
    ) -> Option<SignatureHelp> {
        let interfaces = OpenDocuments::interfaces_for_uri(uri);
        let mut signatures = Self::target_compatible_completion_imported_modules(module)
            .into_iter()
            .filter_map(|module_name| interfaces.get(&module_name))
            .flat_map(|interface| interface.function_overloads.values())
            .flat_map(|overloads| overloads.iter())
            .filter(|signature| {
                signature.public
                    && signature.receiver_method
                    && signature.name == callee
                    && signature
                        .params
                        .first()
                        .map(|param| Self::base_type_name(&param.annotation) == type_name)
                        .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        signatures.sort_by(|left, right| {
            left.params
                .len()
                .cmp(&right.params.len())
                .then_with(|| left.return_type.cmp(&right.return_type))
        });
        let signature = signatures.into_iter().next()?;
        Some(Self::signature_help_from_interface_parts(
            &signature.name,
            &signature.generic_params,
            &signature.params[1..],
            &signature.return_type,
            &signature.docs,
            active_parameter,
            signature.pure,
        ))
    }

    /// Builds signature help for a local receiver method call.
    ///
    /// Inputs:
    /// - `module`: parsed syntax-output module.
    /// - `type_name`: receiver base type name.
    /// - `callee`: method name being called.
    /// - `active_parameter`: zero-based active dotted-call argument index.
    ///
    /// Output:
    /// - Signature help for local receiver-method declarations or explicit
    ///   impl methods targeting the receiver type.
    ///
    /// Transformation:
    /// - Excludes the receiver itself from dotted-call parameter labels while
    ///   preserving typed/defaulted argument labels for editor display.
    fn receiver_method_signature_help(
        module: &SyntaxModuleOutput,
        type_name: &str,
        callee: &str,
        active_parameter: usize,
    ) -> Option<SignatureHelp> {
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Method {
                    receiver,
                    name,
                    generic_params,
                    params,
                    return_type,
                    ..
                } if name == callee
                    && Self::base_type_name(&receiver.annotation.text) == type_name =>
                {
                    return Some(Self::signature_help_from_parts(
                        name,
                        generic_params,
                        params,
                        &return_type.text,
                        &declaration.docs,
                        active_parameter,
                        Self::declaration_has_marker_annotation(declaration, &["pure"]),
                    ));
                }
                SyntaxDeclarationPayload::TraitImpl {
                    for_type, methods, ..
                } if Self::base_type_name(&for_type.text) == type_name => {
                    for method in methods.iter().filter(|method| method.name == callee) {
                        let Some(receiver) = method.params.first() else {
                            continue;
                        };
                        if Self::base_type_name(&receiver.annotation.text) != type_name {
                            continue;
                        }
                        return Some(Self::signature_help_from_parts(
                            &method.name,
                            &[],
                            &method.params[1..],
                            &method.return_type.text,
                            &[],
                            active_parameter,
                            false,
                        ));
                    }
                }
                _ => {}
            }
        }
        None
    }
}
