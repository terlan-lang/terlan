use super::*;

/// Validates local negative trait facts without exposing them as conformances.
pub(crate) fn check_syntax_negative_trait_impls(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
    visible_type_names: &HashSet<String>,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let imported_type_refs = imported_type_text_refs(&imported_type_names(resolved));
    let positive_impl_entries =
        collect_syntax_positive_trait_impl_keys(module, &imported_type_refs);
    let positive_impls = positive_impl_entries
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<HashSet<_>>();
    let (imported_positive_impls, imported_negative_impls) =
        collect_imported_trait_impl_polarities(module, resolved);
    let mut negative_impls = HashSet::new();
    let mut diagnostics = Vec::new();

    for (key, span) in positive_impl_entries {
        if imported_negative_impls.contains(&key) {
            diagnostics.push(Diagnostic {
                span,
                message: format!(
                    "conflicting positive and imported negative trait impls for `{}`",
                    key
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }
    }

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_negative: true,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: "unable to parse trait name in negative impl".to_string(),
                severity: DiagSeverity::Error,
            });
            continue;
        };
        let Some(signature) = trait_map.get(&target.name) else {
            diagnostics.push(Diagnostic {
                span: trait_ref.span.into(),
                message: format!("unknown trait `{}` in negative impl", target.name),
                severity: DiagSeverity::Error,
            });
            continue;
        };
        if signature.type_params.len() != 1 {
            diagnostics.push(Diagnostic {
                span: trait_ref.span.into(),
                message: format!(
                    "negative impl trait `{}` expects {} type parameter(s), found 1",
                    target.name,
                    signature.type_params.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let resolved_target =
            match resolve_negative_impl_target(&for_type.text, visible_type_names, resolved) {
                Ok(target) => target,
                Err(message) => {
                    diagnostics.push(Diagnostic {
                        span: for_type.span.into(),
                        message,
                        severity: DiagSeverity::Error,
                    });
                    continue;
                }
            };
        if resolved.imported_traits.contains_key(&target.name)
            && !negative_impl_target_is_local(&resolved_target, resolved)
        {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "negative impl orphan rule violation: trait `{}` and target `{}` are both non-local",
                    target.name,
                    normalize_trait_type_text(&for_type.text)
                ),
                severity: DiagSeverity::Error,
            });
        }

        let for_type =
            crate::terlan_hir::qualify_syntax_type_text(&for_type.text, &imported_type_refs);
        let target = ParsedTraitInstance {
            name: target.name,
            type_args: vec![normalize_trait_type_text(&for_type)],
        };
        let Some(key) = syntax_trait_impl_key(&target, &for_type) else {
            continue;
        };
        if positive_impls.contains(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "conflicting positive and negative trait impls for `{}`",
                    key
                ),
                severity: DiagSeverity::Error,
            });
        }
        if imported_positive_impls.contains(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "conflicting imported positive and negative trait impls for `{}`",
                    key
                ),
                severity: DiagSeverity::Error,
            });
        }
        if !negative_impls.insert(key.clone()) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!("duplicate negative trait impl for `{}`", key),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Collects visible negative trait facts for generic bound resolution.
pub(crate) fn collect_visible_negative_trait_impl_type_args(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    alias_names: &HashSet<String>,
) -> HashMap<String, Vec<Vec<Type>>> {
    let imported_type_refs = imported_type_text_refs(&imported_type_names(resolved));
    let mut facts = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_negative: true,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
            continue;
        };
        let target_text =
            crate::terlan_hir::qualify_syntax_type_text(&for_type.text, &imported_type_refs);
        if let Some(target_type) = parse_negative_trait_bound_type(&target_text, alias_names) {
            insert_negative_trait_bound_fact(&mut facts, target.name, target_type);
        }
    }

    let visible_interfaces = visible_imported_interfaces(module, resolved);
    for imported in resolved.imported_traits.values() {
        for interface in &visible_interfaces {
            for conformance in &interface.trait_conformances {
                if !conformance.public || !conformance.is_negative {
                    continue;
                }
                let Some(target) = parse_trait_instance_from_text(&conformance.trait_ref) else {
                    continue;
                };
                if !conformance_trait_matches_import(&target.name, imported) {
                    continue;
                }
                let target_text = qualify_interface_trait_type_text(
                    &conformance.for_type,
                    interface,
                    &resolved.interface_map,
                )
                .unwrap_or_else(|| conformance.for_type.clone());
                if let Some(target_type) =
                    parse_negative_trait_bound_type(&target_text, alias_names)
                {
                    insert_negative_trait_bound_fact(
                        &mut facts,
                        imported.local_name.clone(),
                        target_type,
                    );
                }
            }
        }
    }

    facts
}

/// Parses one canonical negative target into the ordinary type model.
fn parse_negative_trait_bound_type(text: &str, alias_names: &HashSet<String>) -> Option<Type> {
    let mut vars = HashMap::new();
    let mut next_var = 0;
    parse_type_expr(text, alias_names, &mut vars, &mut next_var)
}

/// Inserts one unary denial while preserving deterministic deduplication.
fn insert_negative_trait_bound_fact(
    facts: &mut HashMap<String, Vec<Vec<Type>>>,
    trait_name: String,
    target: Type,
) {
    let candidates = facts.entry(trait_name).or_default();
    let fact = vec![target];
    if !candidates.contains(&fact) {
        candidates.push(fact);
    }
}

/// Collects visible imported conformance keys, separated by polarity.
fn collect_imported_trait_impl_polarities(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> (HashSet<String>, HashSet<String>) {
    let mut positive = HashSet::new();
    let mut negative = HashSet::new();
    let visible_interfaces = visible_imported_interfaces(module, resolved);

    for imported in resolved.imported_traits.values() {
        for interface in &visible_interfaces {
            for conformance in &interface.trait_conformances {
                if !conformance.public {
                    continue;
                }
                let Some(key) = imported_trait_impl_key(conformance, imported, interface, resolved)
                else {
                    continue;
                };
                if conformance.is_negative {
                    negative.insert(key);
                } else {
                    positive.insert(key);
                }
            }
        }
    }

    (positive, negative)
}

/// Rewrites one provider conformance into the consumer's canonical namespace.
fn imported_trait_impl_key(
    conformance: &TraitConformanceSignature,
    imported: &ImportedItem,
    interface: &ModuleInterface,
    resolved: &ResolvedModule,
) -> Option<String> {
    let mut target = parse_trait_instance_from_text(&conformance.trait_ref)?;
    if !conformance_trait_matches_import(&target.name, imported) {
        return None;
    }
    target.name = imported.local_name.clone();
    target.type_args =
        qualify_interface_trait_type_args(&target.type_args, interface, &resolved.interface_map);
    let for_type = qualify_interface_trait_type_text(
        &conformance.for_type,
        interface,
        &resolved.interface_map,
    )
    .unwrap_or_else(|| normalize_trait_type_text(&conformance.for_type));
    if conformance.is_negative {
        target.type_args = vec![for_type.clone()];
    }
    syntax_trait_impl_key(&target, &for_type)
}

/// Returns interfaces named directly by source imports in declaration order.
fn visible_imported_interfaces<'a>(
    module: &SyntaxModuleOutput,
    resolved: &'a ResolvedModule,
) -> Vec<&'a ModuleInterface> {
    let mut seen = HashSet::new();
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import { module_name, .. }
                if seen.insert(module_name.clone()) =>
            {
                resolved.interface_map.get(module_name)
            }
            _ => None,
        })
        .collect()
}

/// Matches provider-qualified or local trait names to one imported trait.
fn conformance_trait_matches_import(trait_name: &str, imported: &ImportedItem) -> bool {
    trait_name == imported.source_name
        || trait_name
            .strip_prefix(&imported.source_module)
            .and_then(|suffix| suffix.strip_prefix('.'))
            == Some(imported.source_name.as_str())
}

/// Resolves a negative impl target and rejects unresolved nested names.
fn resolve_negative_impl_target(
    text: &str,
    visible_type_names: &HashSet<String>,
    resolved: &ResolvedModule,
) -> Result<Type, String> {
    let mut vars = HashMap::new();
    let mut next_var = 0;
    let Some(target) = parse_type_expr(text, visible_type_names, &mut vars, &mut next_var) else {
        return Err(format!("invalid type `{}` in negative impl target", text));
    };
    let Some(unknown) =
        first_unresolved_negative_target(&target, visible_type_names, resolved, &vars)
    else {
        return Ok(target);
    };
    Err(format!(
        "unknown type `{}` in negative impl target `{}`",
        unknown,
        normalize_trait_type_text(text)
    ))
}

/// Returns whether the outer target constructor is owned by this module.
fn negative_impl_target_is_local(target: &Type, resolved: &ResolvedModule) -> bool {
    let Type::Named { module, name, .. } = target else {
        return false;
    };
    match module {
        None => resolved.local_type_names.contains_key(name),
        Some(module) if module == &resolved.name => resolved.local_type_names.contains_key(name),
        Some(_) => false,
    }
}

/// Finds the first unresolved name in a parsed negative impl target.
fn first_unresolved_negative_target(
    target: &Type,
    visible_type_names: &HashSet<String>,
    resolved: &ResolvedModule,
    vars: &HashMap<String, TypeVarId>,
) -> Option<String> {
    match target {
        Type::Var(id) => vars
            .iter()
            .find_map(|(name, candidate)| (*candidate == *id).then(|| name.clone()))
            .or_else(|| Some(format!("T{}", id))),
        Type::Apply { constructor, .. } => vars
            .iter()
            .find_map(|(name, candidate)| (*candidate == *constructor).then(|| name.clone()))
            .or_else(|| Some(format!("T{}", constructor))),
        Type::Placeholder => Some("_".to_string()),
        Type::Named { module, name, args } => {
            let is_visible = match module {
                None => visible_type_names.contains(name),
                Some(module) if module == &resolved.name => {
                    resolved.local_type_names.contains_key(name)
                }
                Some(module) => resolved.imported_types.values().any(|imported| {
                    imported.source_module == *module && imported.source_name == *name
                }),
            };
            if !is_visible {
                return Some(match module {
                    Some(module) => format!("{}.{}", module, name),
                    None => name.clone(),
                });
            }
            args.iter().find_map(|arg| {
                first_unresolved_negative_target(arg, visible_type_names, resolved, vars)
            })
        }
        Type::Existential { .. } => Some("exists".to_string()),
        Type::List(inner) => {
            first_unresolved_negative_target(inner, visible_type_names, resolved, vars)
        }
        Type::FixedArray { size, elem } => {
            if let crate::terlan_typeck::types::FixedArraySize::Param(id) = size {
                return vars
                    .iter()
                    .find_map(|(name, candidate)| (*candidate == *id).then(|| name.clone()))
                    .or_else(|| Some(format!("T{id}")));
            }
            first_unresolved_negative_target(elem, visible_type_names, resolved, vars)
        }
        Type::Tuple(items) | Type::Union(items) => items.iter().find_map(|item| {
            first_unresolved_negative_target(item, visible_type_names, resolved, vars)
        }),
        Type::Map(fields) => fields.iter().find_map(|field| {
            first_unresolved_negative_target(&field.value, visible_type_names, resolved, vars)
        }),
        Type::Function { params, ret } => params
            .iter()
            .chain(std::iter::once(ret.as_ref()))
            .find_map(|item| {
                first_unresolved_negative_target(item, visible_type_names, resolved, vars)
            }),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::LiteralBool(_) => None,
    }
}

/// Converts positive syntax-output impl declarations into checker summaries.
pub(crate) fn syntax_trait_impl_to_parsed(
    declaration: &SyntaxDeclarationOutput,
    imported_type_refs: &HashMap<String, String>,
) -> Option<ParsedTraitImpl> {
    let SyntaxDeclarationPayload::TraitImpl {
        trait_ref,
        generic_params,
        for_type,
        is_negative,
        methods,
        ..
    } = &declaration.payload
    else {
        return None;
    };

    (!*is_negative).then_some(())?;
    let trait_ref =
        crate::terlan_hir::qualify_syntax_type_text(&trait_ref.text, imported_type_refs);
    let for_type = crate::terlan_hir::qualify_syntax_type_text(&for_type.text, imported_type_refs);
    let target = parse_trait_instance_from_text(&trait_ref)?;
    Some(ParsedTraitImpl {
        target,
        generic_params: generic_params.clone(),
        for_type: Some(normalize_trait_type_text(&for_type)),
        methods: methods
            .iter()
            .map(|method| syntax_impl_method_signature(method, imported_type_refs))
            .collect(),
    })
}

/// Drops impl method bodies into the signature shape used by conformance checks.
fn syntax_impl_method_signature(
    method: &SyntaxImplMethodOutput,
    imported_type_refs: &HashMap<String, String>,
) -> ParsedMethodSignature {
    ParsedMethodSignature {
        name: method.name.clone(),
        params: method
            .params
            .iter()
            .map(|param| {
                normalize_trait_type_text(&crate::terlan_hir::qualify_syntax_type_text(
                    &param.annotation.text,
                    imported_type_refs,
                ))
            })
            .collect(),
        mutable_params: method.params.iter().map(|param| param.is_mutable).collect(),
        return_type: normalize_trait_type_text(&crate::terlan_hir::qualify_syntax_type_text(
            &method.return_type.text,
            imported_type_refs,
        )),
        span: method.span.into(),
    }
}
