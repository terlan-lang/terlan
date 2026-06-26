use super::*;

/// Lowers formal syntax-output for one module into an Erlang module model.
///
/// Inputs:
/// - `module`: formal syntax-output module to lower.
/// - `interfaces`: imported module interfaces keyed by Terlan module name.
/// - `file_imports`: raw imported file bytes available to expression lowering.
/// - `templates`: imported HTML templates available to template lowering.
/// - `markdown_imports`: imported markdown documents available to expression
///   lowering.
///
/// Output:
/// - Erlang module render model with exports, type exports, record forms, type
///   declarations, functions, and raw backend fragments.
/// - `None` when a declaration body cannot be lowered.
///
/// Transformation:
/// - Builds syntax lowering context, routes each top-level declaration into its
///   declaration-specific lowering helper, and assembles the final Erlang form
///   order expected by the backend renderer.
pub(in crate::emit) fn lower_syntax_module_output(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
    file_imports: &BTreeMap<String, Vec<u8>>,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
) -> Option<ErlModule> {
    let ctx = SyntaxLowerCtx::new(
        module,
        interfaces,
        file_imports,
        templates,
        markdown_imports,
    );
    let mut exports = BTreeSet::new();
    let mut type_exports = BTreeSet::new();
    let mut type_forms = Vec::new();
    let mut struct_forms = lower_imported_syntax_struct_record_decls(module, interfaces);
    let mut function_forms = Vec::new();

    for decl in &module.declarations {
        match &decl.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                ..
            } => {}
            SyntaxDeclarationPayload::Import {
                import_kind:
                    SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown,
                ..
            } => {}
            SyntaxDeclarationPayload::Template { .. } => {}
            SyntaxDeclarationPayload::AnnotationSchema { .. } => {}
            SyntaxDeclarationPayload::Trait { name, .. } => {
                function_forms.push(ErlForm::Raw(format!("%% trait {}.\n", name)));
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_public,
                methods,
            } => {
                if *is_public {
                    if let Some(trait_name) = syntax_type_head_name(&trait_ref.text) {
                        let type_arg = normalize_trait_type_text(&for_type.text);
                        let qualified_type_arg =
                            qualify_imported_type_text(&type_arg, &ctx.imported_type_refs);
                        for method in methods {
                            exports.insert(format!(
                                "{}/{}",
                                typed_trait_method_wrapper_name(
                                    &trait_name,
                                    &method.name,
                                    &type_arg
                                ),
                                method.params.len() + 1
                            ));
                            if qualified_type_arg != type_arg {
                                exports.insert(format!(
                                    "{}/{}",
                                    typed_trait_method_wrapper_name(
                                        &trait_name,
                                        &method.name,
                                        &qualified_type_arg
                                    ),
                                    method.params.len() + 1
                                ));
                            }
                        }
                    }
                }
                function_forms.extend(lower_syntax_trait_impl_decl(
                    decl, trait_ref, for_type, methods, &ctx,
                )?);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for signature in extract_native_function_signatures(text) {
                    exports.insert(format!("{}/{}", signature.name, signature.arity));
                }
                function_forms.push(ErlForm::Raw(lower_raw_decl_text(name, text)));
            }
            SyntaxDeclarationPayload::Config { .. } => {}
            SyntaxDeclarationPayload::Raw { raw_kind, .. } if raw_kind == "impl" => {}
            SyntaxDeclarationPayload::Raw { raw_kind, text } => {
                if raw_kind == "native" {
                    for signature in extract_native_function_signatures(text) {
                        exports.insert(format!("{}/{}", signature.name, signature.arity));
                    }
                }
                function_forms.push(ErlForm::Raw(lower_raw_decl_text(raw_kind, text)));
            }
            SyntaxDeclarationPayload::Export { items } => {
                collect_syntax_export_payloads(module.source_kind, items, &mut exports)
            }
            SyntaxDeclarationPayload::Constructor {
                name,
                is_public,
                clauses,
                ..
            } => {
                if *is_public {
                    for clause in clauses {
                        let fixed_arity = clause
                            .params
                            .iter()
                            .filter(|param| !param.is_varargs)
                            .count();
                        let varargs = clause.params.iter().any(|param| param.is_varargs);
                        let erlang_arity = if varargs {
                            fixed_arity + 1
                        } else {
                            fixed_arity
                        };
                        exports.insert(format!(
                            "{}/{}",
                            constructor_function_name(name, fixed_arity, varargs),
                            erlang_arity
                        ));
                    }
                }
                function_forms.extend(lower_syntax_constructor_decl(name, clauses, &ctx)?);
            }
            SyntaxDeclarationPayload::Type {
                name,
                params,
                is_public,
                is_opaque,
                variants,
                ..
            } => {
                if *is_public {
                    type_exports.insert(format!("{}/{}", map_type_name(name), params.len()));
                }
                type_forms.push(ErlForm::Type(lower_syntax_type_decl(
                    decl, name, params, *is_opaque, variants,
                )));
            }
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                fields,
                ..
            } => {
                if *is_public {
                    type_exports.insert(format!("{}/0", map_type_name(name)));
                }
                type_forms.push(ErlForm::Type(lower_syntax_struct_type_decl(name)));
                struct_forms.push(ErlForm::Record(lower_syntax_struct_decl(
                    decl, name, fields, &ctx,
                )?));
            }
            SyntaxDeclarationPayload::Function {
                name,
                params,
                generic_bounds,
                return_type,
                is_public,
                clauses,
                ..
            } => {
                if *is_public && !clauses.is_empty() {
                    exports.insert(format!("{}/{}", name, params.len() + generic_bounds.len()));
                }
                function_forms.extend(lower_syntax_function_decl(
                    decl,
                    name,
                    params,
                    generic_bounds,
                    return_type,
                    clauses,
                    &ctx,
                )?);
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                clauses,
                ..
            } => {
                let arity = params.len() + 1;
                if *is_public && !clauses.is_empty() {
                    exports.insert(format!("{}/{}", name, arity));
                }
                function_forms.extend(lower_syntax_method_decl(
                    decl,
                    receiver,
                    name,
                    params,
                    return_type,
                    clauses,
                    &ctx,
                )?);
            }
        }
    }
    let mut forms = Vec::new();
    if !exports.is_empty() {
        forms.push(ErlForm::Export(exports.into_iter().collect()));
    }
    if !type_exports.is_empty() {
        forms.push(ErlForm::ExportType(type_exports.into_iter().collect()));
    }
    forms.extend(struct_forms);
    forms.extend(type_forms);
    forms.extend(function_forms);

    Some(ErlModule {
        name: map_module_name(&module.module_name),
        docs: module.docs.clone(),
        forms,
    })
}

/// Lowers explicitly imported public struct types into local Erlang records.
///
/// Inputs:
/// - `module`: syntax-output consumer module containing import declarations.
/// - `interfaces`: provider interfaces keyed by Terlan module name.
///
/// Output:
/// - Erlang record forms for selected `import type provider.Struct` items.
///
/// Transformation:
/// - Reads only type imports, looks up matching public struct field metadata in
///   provider interfaces, and emits span-free record declarations needed by
///   BEAM record construction, access, and pattern matching in the consumer
///   module. Local struct names and repeated imports are skipped to avoid
///   duplicate record declarations.
fn lower_imported_syntax_struct_record_decls(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> Vec<ErlForm> {
    let local_structs = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut seen = BTreeSet::new();
    let mut records = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                is_type: true,
                ..
            } => Some((module_name, items)),
            _ => None,
        })
        .flat_map(|(module_name, items)| {
            let Some(interface) = interfaces.get(module_name) else {
                return Vec::new();
            };
            items
                .iter()
                .filter_map(|item| {
                    if local_structs.contains(&item.name) || !seen.insert(item.name.clone()) {
                        return None;
                    }
                    let fields = interface.struct_fields.get(&item.name)?;
                    Some(ErlForm::Record(ErlRecordDecl {
                        name: map_struct_name(&item.name),
                        fields: fields
                            .iter()
                            .map(|field| ErlRecordField {
                                name: field.name.clone(),
                                docs: Vec::new(),
                                default: None,
                            })
                            .collect(),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    if module_imports_value_module(module, "std.io.File") && !local_structs.contains("FileError") {
        push_std_io_file_error_record(&mut records, interfaces, &mut seen);
    }

    records
}

/// Adds the `std.io.File.FileError` record declaration required by file runtime
/// capabilities.
///
/// Inputs:
/// - `records`: target Erlang record declarations.
/// - `interfaces`: imported provider interfaces when available.
/// - `seen`: record names already emitted into this module.
///
/// Output:
/// - Mutates `records` at most once.
///
/// Transformation:
/// - Prefers provider interface fields and falls back to the runtime-owned
///   `FileError` ABI for value-only `std.io.File` imports, where no type
///   import forces the provider interface into the bridge.
fn push_std_io_file_error_record(
    records: &mut Vec<ErlForm>,
    interfaces: &BTreeMap<String, ModuleInterface>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert("FileError".to_string()) {
        return;
    }
    let fields = interfaces
        .get("std.io.File")
        .and_then(|interface| interface.struct_fields.get("FileError"))
        .map(|fields| {
            fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                "code".to_string(),
                "message".to_string(),
                "path".to_string(),
            ]
        });
    records.push(ErlForm::Record(ErlRecordDecl {
        name: map_struct_name("FileError"),
        fields: fields
            .into_iter()
            .map(|name| ErlRecordField {
                name,
                docs: Vec::new(),
                default: None,
            })
            .collect(),
    }));
}

/// Returns whether a module imports a provider for value-level use.
///
/// Inputs:
/// - `module`: syntax-output module to inspect.
/// - `provider`: fully qualified provider module path.
///
/// Output:
/// - `true` when the source has an executable module import for `provider`.
///
/// Transformation:
/// - Distinguishes value imports from type-only imports so runtime companion
///   record declarations are emitted only for modules that may execute the
///   provider capability.
fn module_imports_value_module(module: &SyntaxModuleOutput, provider: &str) -> bool {
    module.declarations.iter().any(|decl| match &decl.payload {
        SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            is_type: false,
            ..
        } => {
            if module_name == provider {
                return true;
            }
            let Some((parent, child)) = provider.rsplit_once('.') else {
                return false;
            };
            module_name == parent && items.iter().any(|item| item.name == child)
        }
        _ => false,
    })
}

/// Collects syntax-output export payloads for direct Erlang lowering.
///
/// Inputs:
/// - `source_kind`: whether the syntax output came from a Terlan source module
///   or an interface summary.
/// - `items`: export entries preserved in the syntax output.
/// - `exports`: mutable Erlang export set.
///
/// Output:
/// - No direct return value; `exports` is extended only for interface outputs.
///
/// Transformation:
/// - Interface export summaries are converted into Erlang `name/arity` export
///   entries.
/// - Module-mode export payloads are ignored because canonical Terlan source
///   uses declaration-site `pub`, and HIR owns the diagnostic for impossible
///   source-kind export payloads.
fn collect_syntax_export_payloads(
    source_kind: SyntaxSourceKind,
    items: &[terlan_syntax::SyntaxExportItem],
    exports: &mut BTreeSet<String>,
) {
    if source_kind == SyntaxSourceKind::Interface {
        for item in items {
            exports.insert(format!("{}/{}", item.name, item.arity));
        }
    }
}

/// Lowers syntax-output struct declarations to Erlang header record text.
///
/// Inputs:
/// - `module`: syntax-output module containing struct declarations.
///
/// Output:
/// - Rendered `.hrl` record declarations for local structs.
/// - `None` when a struct default expression cannot lower.
///
/// Transformation:
/// - Uses an empty lowering context and the ordinary struct declaration helper
///   to render record headers consumed by generated Erlang modules.
pub(in crate::emit) fn lower_syntax_struct_headers_to_hrl(
    module: &SyntaxModuleOutput,
) -> Option<String> {
    let ctx = SyntaxLowerCtx::empty();
    module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } => Some((decl, name, fields)),
            _ => None,
        })
        .map(|(decl, name, fields)| {
            Some(ErlForm::Record(lower_syntax_struct_decl(decl, name, fields, &ctx)?).render())
        })
        .collect::<Option<Vec<_>>>()
        .map(|forms| forms.join(""))
}

/// Lowers a Terlan type declaration into an Erlang type declaration.
///
/// Inputs:
/// - `decl`: source declaration metadata, including docs.
/// - `name`: Terlan type name.
/// - `params`: generic type parameter names.
/// - `is_opaque`: whether the source declaration hides its representation.
/// - `variants`: source type body variants, empty for representation-hidden
///   opaque declarations.
///
/// Output:
/// - Erlang type declaration preserving public type name, params, docs, and
///   opacity.
///
/// Transformation:
/// - Converts explicit variants into an Erlang union type. Empty opaque
///   declarations lower to `term()` so BEAM specs remain valid while the
///   source representation stays hidden.
fn lower_syntax_type_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    params: &[String],
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
) -> ErlTypeDecl {
    let rhs = if is_opaque && variants.is_empty() {
        ErlType::Raw("term()".to_string())
    } else {
        ErlType::Union(
            variants
                .iter()
                .map(|variant| lower_type_to_spec(&variant.text))
                .collect(),
        )
        .normalized()
    };

    ErlTypeDecl {
        opaque: is_opaque,
        docs: decl.docs.clone(),
        name: map_type_name(name),
        params: params.to_vec(),
        rhs,
    }
}

/// Lowers a Terlan struct declaration into an Erlang record declaration.
///
/// Inputs:
/// - `decl`: source declaration metadata.
/// - `name`: source struct/type name.
/// - `fields`: struct fields with docs and optional defaults.
/// - `ctx`: active syntax lowering context for default expressions.
///
/// Output:
/// - Erlang record declaration.
/// - `None` when a field default expression cannot lower.
///
/// Transformation:
/// - Maps the Terlan type name to the backend record name and lowers each field
///   default through syntax expression lowering while preserving field docs.
fn lower_syntax_struct_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    fields: &[SyntaxStructFieldOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlRecordDecl> {
    let _ = decl;
    Some(ErlRecordDecl {
        name: map_struct_name(name),
        fields: fields
            .iter()
            .map(|field| {
                Some(ErlRecordField {
                    name: field.name.clone(),
                    docs: field.docs.clone(),
                    default: match field.default.as_ref() {
                        Some(default) => Some(lower_syntax_expr(default, ctx)?),
                        None => None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Lowers a struct declaration's nominal type alias.
///
/// Inputs:
/// - `name`: source struct/type name.
///
/// Output:
/// - Erlang type declaration that aliases the generated record shape.
///
/// Transformation:
/// - Maps the source type name and struct name to backend names and emits a
///   zero-parameter type whose right side is the generated record expression.
fn lower_syntax_struct_type_decl(name: &str) -> ErlTypeDecl {
    ErlTypeDecl {
        opaque: false,
        docs: Vec::new(),
        name: map_type_name(name),
        params: Vec::new(),
        rhs: ErlType::Raw(format!("#{}{{}}", map_struct_name(name))),
    }
}

/// Lowers a syntax-output function declaration into Erlang forms.
///
/// Inputs:
/// - `decl`: source declaration metadata, docs, annotations, and span.
/// - `name`: source function name.
/// - `params`: source function parameters.
/// - `generic_bounds`: source generic trait bounds.
/// - `return_type`: source return type annotation.
/// - `clauses`: parsed function clauses.
/// - `ctx`: active syntax lowering context.
///
/// Output:
/// - Erlang spec/function forms for the function.
/// - `None` when a clause pattern, guard, body, or intrinsic replacement
///   cannot lower.
///
/// Transformation:
/// - Prepends hidden generic-bound dictionaries to the backend ABI when
///   required, emits specs for functions with parameters, and lowers each
///   function clause through the shared expression and pattern lowerers.
fn lower_syntax_function_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    generic_bounds: &[String],
    return_type: &SyntaxTypeOutput,
    clauses: &[SyntaxFunctionClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let mut forms = Vec::new();
    let env = lower_syntax_function_env(params, ctx, generic_bounds);
    let hidden_bound_params = generic_bound_param_names(generic_bounds);

    if !params.is_empty() {
        forms.push(ErlForm::Spec(ErlSpec {
            docs: decl.docs.clone(),
            name: name.to_string(),
            args: hidden_bound_params
                .iter()
                .map(|_| ErlType::Raw("map()".to_string()))
                .chain(
                    params
                        .iter()
                        .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx)),
                )
                .collect(),
            ret: lower_syntax_type_to_spec(&return_type.text, ctx),
        }));
    }

    forms.push(ErlForm::Function(ErlFunction {
        docs: if params.is_empty() {
            decl.docs.clone()
        } else {
            Vec::new()
        },
        name: name.to_string(),
        clauses: clauses
            .iter()
            .map(|clause| {
                let body = lower_intrinsic_annotation_body(decl, params, &ctx.module_name)
                    .or_else(|| lower_syntax_expr_with_env(&clause.body, ctx, &env))?;
                let mut patterns = hidden_bound_params
                    .iter()
                    .map(|name| ErlPattern::Var(name.clone()))
                    .collect::<Vec<_>>();
                patterns.extend(
                    clause
                        .patterns
                        .iter()
                        .map(|pattern| lower_syntax_pattern(pattern, ctx))
                        .collect::<Option<Vec<_>>>()?,
                );
                Some(ErlFunctionClause {
                    patterns,
                    guard: match clause.guard.as_ref() {
                        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                        None => None,
                    },
                    body,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    }));

    Some(forms)
}

/// Lowers an explicit trait impl declaration into typed wrapper functions.
///
/// Inputs:
/// - `decl`: source declaration metadata, docs, annotations, and span.
/// - `trait_ref`: implemented trait type expression.
/// - `for_type`: concrete type expression after `for`.
/// - `methods`: implementation methods declared inside the impl block.
/// - `ctx`: syntax lowering context for method bodies.
///
/// Output:
/// - Erlang spec/function forms for each impl method wrapper.
/// - `None` when the trait head cannot be identified or a method body cannot
///   lower through the syntax-output path.
///
/// Transformation:
/// - Emits private typed dictionary-wrapper functions with a hidden first
///   dictionary argument followed by the source method arguments. Trait call
///   lowering can then dispatch `Trait.method(value)` to the wrapper selected
///   by the inferred first argument type.
fn lower_syntax_trait_impl_decl(
    decl: &SyntaxDeclarationOutput,
    trait_ref: &SyntaxTypeOutput,
    for_type: &SyntaxTypeOutput,
    methods: &[SyntaxImplMethodOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let trait_name = syntax_type_head_name(&trait_ref.text)?;
    let type_arg = normalize_trait_type_text(&for_type.text);
    let qualified_type_arg = qualify_imported_type_text(&type_arg, &ctx.imported_type_refs);

    let mut lowered = methods
        .iter()
        .map(|method| lower_syntax_trait_impl_method(decl, &trait_name, &type_arg, method, ctx))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if qualified_type_arg != type_arg {
        lowered.extend(
            methods
                .iter()
                .map(|method| {
                    lower_syntax_trait_impl_method(
                        decl,
                        &trait_name,
                        &qualified_type_arg,
                        method,
                        ctx,
                    )
                })
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .flatten(),
        );
    }
    Some(lowered)
}

/// Lowers one explicit trait impl method into a typed wrapper function.
///
/// Inputs:
/// - `decl`: parent impl declaration metadata.
/// - `trait_name`: source-visible trait head.
/// - `type_arg`: normalized concrete implementation type.
/// - `method`: implementation method signature and clauses.
/// - `ctx`: syntax lowering context for method expressions and patterns.
///
/// Output:
/// - Erlang spec and function forms for the generated wrapper.
///
/// Transformation:
/// - Prepends a hidden trait dictionary argument to the Erlang ABI while
///   preserving the source method clauses, guards, and bodies. The dictionary
///   argument is currently metadata-only for local explicit impl dispatch.
fn lower_syntax_trait_impl_method(
    decl: &SyntaxDeclarationOutput,
    trait_name: &str,
    type_arg: &str,
    method: &SyntaxImplMethodOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let wrapper_name = typed_trait_method_wrapper_name(trait_name, &method.name, type_arg);
    let env = lower_syntax_function_env(&method.params, ctx, &[]);

    Some(vec![
        ErlForm::Spec(ErlSpec {
            docs: decl.docs.clone(),
            name: wrapper_name.clone(),
            args: std::iter::once(ErlType::Raw("map()".to_string()))
                .chain(
                    method
                        .params
                        .iter()
                        .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx)),
                )
                .collect(),
            ret: lower_syntax_type_to_spec(&method.return_type.text, ctx),
        }),
        ErlForm::Function(ErlFunction {
            docs: Vec::new(),
            name: wrapper_name,
            clauses: method
                .clauses
                .iter()
                .map(|clause| {
                    let mut patterns = Vec::with_capacity(clause.patterns.len() + 1);
                    patterns.push(ErlPattern::Var("_TraitDict".to_string()));
                    patterns.extend(
                        clause
                            .patterns
                            .iter()
                            .map(|pattern| lower_syntax_pattern(pattern, ctx))
                            .collect::<Option<Vec<_>>>()?,
                    );
                    Some(ErlFunctionClause {
                        patterns,
                        guard: match clause.guard.as_ref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        }),
    ])
}

/// Lowers a syntax-output receiver method into an Erlang function.
///
/// Inputs:
/// - `decl`: source declaration metadata, docs, annotations, and span.
/// - `receiver`: receiver parameter declared before the method name.
/// - `name`: method name to expose as a backend function.
/// - `params`: ordinary method parameters.
/// - `return_type`: method return type annotation.
/// - `clauses`: parsed method clauses; currently one clause is produced by the
///   parser.
/// - `ctx`: syntax lowering context for expressions and imports.
///
/// Output:
/// - Erlang spec/function forms, or `None` when a method body cannot lower.
///
/// Transformation:
/// - Rewrites `(receiver: Type) name(args...)` into the backend calling
///   convention `name(receiver, args...)`, using the same intrinsic annotation
///   path as ordinary functions.
fn lower_syntax_method_decl(
    decl: &SyntaxDeclarationOutput,
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    clauses: &[SyntaxFunctionClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let all_params = syntax_method_params(receiver, params);
    let env = lower_syntax_function_env(&all_params, ctx, &[]);
    let mut forms = Vec::new();

    forms.push(ErlForm::Spec(ErlSpec {
        docs: decl.docs.clone(),
        name: name.to_string(),
        args: all_params
            .iter()
            .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx))
            .collect(),
        ret: lower_syntax_method_return_type_to_spec(receiver, return_type, ctx),
    }));

    forms.push(ErlForm::Function(ErlFunction {
        docs: Vec::new(),
        name: name.to_string(),
        clauses: clauses
            .iter()
            .map(|clause| {
                let body = lower_intrinsic_annotation_body_for_names(
                    decl,
                    all_params.iter().map(|param| param.name.as_str()),
                    &ctx.module_name,
                )
                .or_else(|| lower_syntax_expr_with_env(&clause.body, ctx, &env))?;
                Some(ErlFunctionClause {
                    patterns: all_params
                        .iter()
                        .map(|param| ErlPattern::Var(sanitize_erlang_var(&param.name)))
                        .collect(),
                    guard: match clause.guard.as_ref() {
                        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                        None => None,
                    },
                    body,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    }));

    Some(forms)
}

/// Lowers a receiver-method return annotation into the backend helper spec.
///
/// Inputs:
/// - `receiver`: source receiver parameter, including the contextual `mut`
///   marker.
/// - `return_type`: source-visible method return type annotation.
///
/// Output:
/// - Erlang type expression for the generated receiver-first helper function.
///
/// Transformation:
/// - Implements the first P0.2c command-style mutable receiver ABI slice:
///   a source method declared as `(mut receiver: T) method(...): Unit` exposes
///   `Unit` to Terlan callers, but the backend helper returns `T` so sequence
///   and pipe lowering can rebind the updated receiver.
fn lower_syntax_method_return_type_to_spec(
    receiver: &SyntaxParamOutput,
    return_type: &SyntaxTypeOutput,
    ctx: &SyntaxLowerCtx,
) -> ErlType {
    if receiver.is_mutable && compact_type_application(&compact_spaces(&return_type.text)) == "Unit"
    {
        lower_syntax_type_to_spec(&receiver.annotation.text, ctx)
    } else {
        lower_syntax_type_to_spec(&return_type.text, ctx)
    }
}

/// Builds the receiver-first parameter list for backend method lowering.
///
/// Inputs:
/// - `receiver`: receiver parameter from a method declaration.
/// - `params`: ordinary method parameters.
///
/// Output:
/// - Owned parameter list with `receiver` first.
///
/// Transformation:
/// - Clones syntax-output parameters into the function-like order required by
///   the current backend calling convention.
fn syntax_method_params(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
) -> Vec<SyntaxParamOutput> {
    std::iter::once(receiver.clone())
        .chain(params.iter().cloned())
        .collect()
}
