//! Syntax-output Erlang lowering context construction.
//!
//! This module collects module-level compiler outputs into lookup tables used by the bridge.

use super::*;
use terlan_syntax::parse_expr_as_syntax_output;

impl SyntaxLowerCtx {
    /// Builds syntax lowering context from module-level compiler outputs.
    ///
    /// Inputs:
    /// - `module`: syntax-output module being lowered.
    /// - `interfaces`: resolved imported interfaces.
    /// - `file_imports`: file asset bytes keyed by source alias.
    /// - `templates`: parsed HTML templates keyed by template name.
    /// - `markdown_imports`: parsed markdown documents keyed by source alias.
    ///
    /// Output:
    /// - Fully populated lowering context for this module.
    ///
    /// Transformation:
    /// - Collects import aliases, constructor targets, alias constructors,
    ///   trait conformance wrappers, generic functions, templates, structs, and
    ///   receiver methods into backend lookup maps.
    pub(in crate::emit) fn new(
        module: &SyntaxModuleOutput,
        interfaces: &BTreeMap<String, ModuleInterface>,
        file_imports: &BTreeMap<String, Vec<u8>>,
        templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
        markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    ) -> Self {
        let module_aliases = module
            .declarations
            .iter()
            .flat_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    is_type: false,
                    ..
                } => {
                    let aliases = items
                        .iter()
                        .filter_map(|item| {
                            let target = format!("{}.{}", module_name, item.name);
                            if let Some(alias) = &item.as_alias {
                                return Some((alias.clone(), target));
                            }
                            is_upper_identifier(&item.name).then(|| (item.name.clone(), target))
                        })
                        .collect::<Vec<_>>();
                    aliases
                }
                _ => Vec::new(),
            })
            .collect();

        let imported_trait_aliases = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    ..
                } => Some((module_name, items, interfaces.get(module_name)?)),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .filter_map(|item| {
                        if !interface.traits.contains_key(&item.name) {
                            return None;
                        }
                        Some((
                            item.as_alias.clone().unwrap_or_else(|| item.name.clone()),
                            (module_name.clone(), item.name.clone()),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let imported_trait_conformances = collect_imported_trait_conformances(module, interfaces);
        let imported_type_refs = collect_imported_type_refs(module, interfaces);

        let imported_functions = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    is_type: false,
                    ..
                } => Some((module_name, items, interfaces.get(module_name)?)),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .flat_map(|item| {
                        let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
                        let overloads = interface
                            .function_overloads
                            .iter()
                            .filter(move |((name, _), _)| name == &item.name)
                            .flat_map(|(_, signatures)| signatures.iter())
                            .filter(|signature| signature.public)
                            .collect::<Vec<_>>();
                        if !overloads.is_empty() {
                            return overloads
                                .into_iter()
                                .map(|signature| {
                                    (
                                        local_name.clone(),
                                        SyntaxImportedFunctionTarget {
                                            module: module_name.clone(),
                                            function: item.name.clone(),
                                            fixed_arity: signature.params.len(),
                                            min_arity: signature
                                                .params
                                                .iter()
                                                .filter(|param| param.default_text.is_none())
                                                .count(),
                                            param_names: signature
                                                .params
                                                .iter()
                                                .map(|param| param.name.clone())
                                                .collect(),
                                            defaults: signature
                                                .params
                                                .iter()
                                                .map(|param| {
                                                    param.default_text.as_ref().and_then(|text| {
                                                        parse_expr_as_syntax_output(text).ok()
                                                    })
                                                })
                                                .collect(),
                                        },
                                    )
                                })
                                .collect::<Vec<_>>();
                        }
                        interface
                            .functions
                            .iter()
                            .filter(move |((name, _), signature)| {
                                name == &item.name && signature.public
                            })
                            .map(move |((_, _arity), signature)| {
                                (
                                    local_name.clone(),
                                    SyntaxImportedFunctionTarget {
                                        module: module_name.clone(),
                                        function: item.name.clone(),
                                        fixed_arity: signature.params.len(),
                                        min_arity: signature
                                            .params
                                            .iter()
                                            .filter(|param| param.default_text.is_none())
                                            .count(),
                                        param_names: signature
                                            .params
                                            .iter()
                                            .map(|param| param.name.clone())
                                            .collect(),
                                        defaults: signature
                                            .params
                                            .iter()
                                            .map(|param| {
                                                param.default_text.as_ref().and_then(|text| {
                                                    parse_expr_as_syntax_output(text).ok()
                                                })
                                            })
                                            .collect(),
                                    },
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .fold(
                BTreeMap::<String, Vec<SyntaxImportedFunctionTarget>>::new(),
                |mut functions, (local_name, target)| {
                    functions.entry(local_name).or_default().push(target);
                    functions
                },
            );
        let imported_module_member_functions =
            collect_imported_module_member_functions(&module_aliases, interfaces);

        let local_function_params = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Function { name, params, .. } = &decl.payload else {
                    return None;
                };
                Some((
                    (name.clone(), params.len()),
                    params.iter().map(|param| param.name.clone()).collect(),
                ))
            })
            .collect();

        let local_functions = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    return_type,
                    ..
                } = &decl.payload
                else {
                    return None;
                };
                Some((
                    name.clone(),
                    SyntaxLocalFunctionTarget {
                        fixed_arity: params.len(),
                        min_arity: params
                            .iter()
                            .filter(|param| param.default.is_none())
                            .count(),
                        param_names: params.iter().map(|param| param.name.clone()).collect(),
                        defaults: params.iter().map(|param| param.default.clone()).collect(),
                        return_type: return_type.text.clone(),
                    },
                ))
            })
            .fold(
                BTreeMap::<String, Vec<SyntaxLocalFunctionTarget>>::new(),
                |mut functions, (name, target)| {
                    functions.entry(name).or_default().push(target);
                    functions
                },
            );

        let constructors = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Constructor { name, clauses, .. } => Some((
                    name.clone(),
                    clauses
                        .iter()
                        .map(|clause| {
                            let fixed_arity = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs)
                                .count();
                            let min_arity = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs && param.default.is_none())
                                .count();
                            let defaults = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs)
                                .map(|param| param.default.clone())
                                .collect();
                            let param_names = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs)
                                .map(|param| param.name.clone())
                                .collect();
                            let varargs = clause.params.iter().any(|param| param.is_varargs);
                            SyntaxConstructorTarget {
                                function: constructor_function_name(name, fixed_arity, varargs),
                                param_names,
                                fixed_arity,
                                min_arity,
                                defaults,
                                varargs,
                            }
                        })
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();

        let constructor_patterns = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Constructor { name, clauses, .. } => Some((
                    name.clone(),
                    clauses
                        .iter()
                        .filter(|clause| !clause.params.iter().any(|param| param.is_varargs))
                        .map(|clause| SyntaxConstructorPatternTarget {
                            params: clause
                                .params
                                .iter()
                                .map(|param| param.name.clone())
                                .collect(),
                            body: clause.body.clone(),
                        })
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();

        let remote_constructor_targets = interfaces
            .iter()
            .flat_map(|(module_name, interface)| {
                interface
                    .constructors
                    .iter()
                    .flat_map(|(name, signatures)| {
                        let targets = signatures
                            .iter()
                            .filter(|signature| signature.public)
                            .map(|signature| {
                                syntax_remote_constructor_target_from_signature(
                                    module_name,
                                    name,
                                    signature,
                                )
                            })
                            .collect::<Vec<_>>();
                        if targets.is_empty() {
                            None
                        } else {
                            Some((format!("{}.{}", module_name, name), targets))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let imported_constructor_targets = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    ..
                } => Some((module_name, items, interfaces.get(module_name))),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .filter_map(|item| {
                        let primary_module = format!("{}.{}", module_name, item.name);
                        let (constructor_module, constructor_interface) = interfaces
                            .get(&primary_module)
                            .map(|primary_interface| (primary_module.as_str(), primary_interface))
                            .or_else(|| {
                                interface.map(|module_interface| {
                                    (module_name.as_str(), module_interface)
                                })
                            })?;
                        let signatures = constructor_interface.constructors.get(&item.name)?;
                        let targets = signatures
                            .iter()
                            .filter(|signature| signature.public)
                            .map(|signature| {
                                syntax_remote_constructor_target_from_signature(
                                    constructor_module,
                                    &item.name,
                                    signature,
                                )
                            })
                            .collect::<Vec<_>>();
                        if targets.is_empty() {
                            None
                        } else {
                            Some((
                                item.as_alias.clone().unwrap_or_else(|| item.name.clone()),
                                targets,
                            ))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let mut alias_constructor_targets = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Type {
                    name,
                    variants,
                    is_opaque: false,
                    ..
                } => parse_syntax_type_alias_constructor_target_texts(
                    &variants
                        .iter()
                        .map(|variant| variant.text.clone())
                        .collect::<Vec<_>>(),
                )
                .map(|target| (name.clone(), target)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        for decl in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                ..
            } = &decl.payload
            else {
                continue;
            };
            let Some(interface) = interfaces.get(module_name) else {
                continue;
            };
            for item in items {
                if !is_custom_type_name(&item.name)
                    || interface.constructors.contains_key(&item.name)
                {
                    continue;
                }
                let Some(type_body) = interface.type_bodies.get(&item.name) else {
                    continue;
                };
                let Some(target) = parse_syntax_type_alias_constructor_target_texts(type_body)
                else {
                    continue;
                };
                let alias = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
                alias_constructor_targets.insert(alias, target);
            }
        }

        let remote_alias_constructor_targets = interfaces
            .iter()
            .flat_map(|(module_name, interface)| {
                interface
                    .type_bodies
                    .iter()
                    .filter(|(name, _)| !interface.constructors.contains_key(*name))
                    .filter_map(|(name, variants)| {
                        parse_syntax_type_alias_constructor_target_texts(variants)
                            .map(|target| (format!("{}.{}", module_name, name), target))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let opaque_constructors = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Type {
                    name,
                    is_opaque: true,
                    ..
                } => Some(name.clone()),
                _ => None,
            })
            .collect();

        let templates = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Template { name, props, .. } = &decl.payload else {
                    return None;
                };
                let parsed = templates.get(name)?;
                Some((
                    name.clone(),
                    LowerTemplate {
                        nodes: parsed.nodes.clone(),
                        prop_order: props.iter().map(|prop| prop.name.clone()).collect(),
                        props: props
                            .iter()
                            .map(|prop| {
                                (
                                    prop.name.clone(),
                                    LowerTemplateProp {
                                        type_text: prop.annotation.text.clone(),
                                        default: prop.default.clone(),
                                    },
                                )
                            })
                            .collect(),
                    },
                ))
            })
            .collect();

        let struct_field_types = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Struct { name, fields, .. } = &decl.payload else {
                    return None;
                };
                Some((
                    name.clone(),
                    fields
                        .iter()
                        .map(|field| (field.name.clone(), field.annotation.text.clone()))
                        .collect::<BTreeMap<_, _>>(),
                ))
            })
            .collect();

        let local_trait_methods = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Trait { name, methods, .. } = &decl.payload else {
                    return None;
                };
                Some((
                    name.clone(),
                    methods
                        .iter()
                        .map(|method| method.name.clone())
                        .collect::<BTreeSet<_>>(),
                ))
            })
            .collect();

        let typed_trait_method_wrappers = collect_syntax_typed_trait_method_wrappers(module);
        let generic_functions = collect_syntax_generic_functions(module);
        let local_function_values = collect_syntax_local_function_values(module);

        let mut receiver_methods = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Method {
                    receiver,
                    name,
                    params,
                    ..
                } = &decl.payload
                else {
                    return None;
                };
                Some((
                    (name.clone(), params.len()),
                    normalize_trait_type_text(&receiver.annotation.text),
                    SyntaxReceiverMethodTarget {
                        mutable: receiver.is_mutable,
                        fixed_arity: params.len(),
                        min_arity: params
                            .iter()
                            .filter(|param| param.default.is_none())
                            .count(),
                        param_names: params.iter().map(|param| param.name.clone()).collect(),
                        defaults: params.iter().map(|param| param.default.clone()).collect(),
                    },
                ))
            })
            .fold(
                BTreeMap::<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>::new(),
                |mut methods, (key, receiver_type, target)| {
                    methods
                        .entry(key)
                        .or_default()
                        .insert(receiver_type, target);
                    methods
                },
            );
        extend_receiver_methods_with_local_struct_includes(module, &mut receiver_methods);

        Self {
            module_name: module.module_name.clone(),
            constructors,
            imported_constructor_targets,
            remote_constructor_targets,
            constructor_patterns,
            alias_constructor_targets,
            remote_alias_constructor_targets,
            local_functions,
            imported_functions,
            local_function_params,
            opaque_constructors,
            trait_method_wrappers: BTreeMap::new(),
            typed_trait_method_wrappers,
            generic_functions,
            local_function_values,
            imported_trait_aliases,
            imported_trait_conformances,
            imported_type_refs,
            local_trait_methods,
            receiver_methods,
            module_aliases,
            imported_module_member_functions,
            file_imports: file_imports.clone(),
            markdown_imports: markdown_imports.clone(),
            templates,
            struct_field_types,
        }
    }
}
