use super::*;

/// Parses a resolved function symbol into a callable type scheme.
///
/// Inputs:
/// - `symbol`: public function symbol loaded from an interface or summary.
///
/// Outputs:
/// - Function scheme when all parameter and return annotations can be parsed.
/// - `None` when the return type annotation is not representable.
///
/// Transformation:
/// - Converts textual interface types into internal type variables and checked
///   parameter/return types without applying trait bounds.
pub(crate) fn parse_symbol_scheme(symbol: &FunctionSymbol) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let params = symbol
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &HashSet::new(), &mut vars, &mut next_var)
        })
        .collect::<Vec<_>>();
    let ret = parse_type_expr(
        &symbol.return_type,
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )?;

    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

/// Parses an interface function signature into a type scheme.
///
/// Inputs:
/// - `signature`: function signature from a module interface.
/// - `interface`: interface that owns local public/opaque type names.
/// - `global_aliases`: dependency aliases keyed by qualified name.
///
/// Outputs:
/// - Function scheme with local and global aliases expanded where possible.
/// - `None` when the return annotation cannot be parsed.
///
/// Transformation:
/// - Parses parameter and return type text, expands interface aliases, expands
///   unique global aliases, and qualifies local interface type names.
pub(crate) fn parse_interface_signature(
    signature: &FunctionSignature,
    interface: &ModuleInterface,
    global_aliases: &HashMap<String, TypeAlias>,
) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let params = signature
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
        })
        .map(|param| expand_type_aliases(&param, &interface_aliases))
        .map(|param| expand_interface_global_aliases(&param, global_aliases))
        .map(|param| qualify_type_names(&param, &qualified_names))
        .collect::<Vec<_>>();

    let ret = parse_type_expr(
        &signature.return_type,
        &alias_names,
        &mut vars,
        &mut next_var,
    )?;
    let ret = expand_type_aliases(&ret, &interface_aliases);
    let ret = expand_interface_global_aliases(&ret, global_aliases);
    let ret = qualify_type_names(&ret, &qualified_names);
    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

/// Expands aliases visible through the global interface map.
///
/// Inputs:
/// - `ty`: type parsed from a public interface signature.
/// - `global_aliases`: fully qualified aliases loaded from dependency
///   interfaces.
///
/// Output:
/// - Type with fully qualified aliases expanded, plus unqualified aliases
///   expanded only when their short name is unique globally.
///
/// Transformation:
/// - Preserves local/interface parsing while allowing checked std summaries
///   such as `Option[T]` to resolve to the single loaded
///   `std.core.Option.Option[T]` alias without requiring every summary to spell
///   fully qualified type names.
fn expand_interface_global_aliases(ty: &Type, global_aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = unique_global_alias(name, global_aliases) {
                if alias.params.len() != args.len() {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                            .collect(),
                    };
                }
                let args = args
                    .iter()
                    .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                if alias.is_opaque {
                    return substitute_type_vars(&alias.body, &mapping);
                }
                return expand_interface_global_aliases(
                    &substitute_type_vars(&alias.body, &mapping),
                    global_aliases,
                );
            }
            expand_type_aliases(ty, global_aliases)
        }
        Type::Named { .. } => expand_type_aliases(ty, global_aliases),
        Type::List(inner) => Type::List(Box::new(expand_interface_global_aliases(
            inner,
            global_aliases,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_interface_global_aliases(&field.value, global_aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_interface_global_aliases(param, global_aliases))
                .collect(),
            ret: Box::new(expand_interface_global_aliases(ret, global_aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_interface_global_aliases(elem, global_aliases)),
        },
        other => other.clone(),
    }
}

/// Finds a globally unique alias by short type name.
///
/// Inputs:
/// - `name`: unqualified type name from an interface signature.
/// - `global_aliases`: fully qualified aliases keyed as `module.Type`.
///
/// Output:
/// - The alias when exactly one global alias has the requested final segment.
///
/// Transformation:
/// - Scans fully qualified alias keys by their final dotted segment and rejects
///   ambiguous short names so interface summaries do not accidentally bind to
///   the wrong module.
fn unique_global_alias<'a>(
    name: &str,
    global_aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a TypeAlias> {
    let mut matches = global_aliases.iter().filter_map(|(qualified, alias)| {
        qualified
            .rsplit_once('.')
            .and_then(|(_, short)| (short == name).then_some(alias))
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

/// Parses public constructor signatures from an imported interface.
///
/// Inputs:
/// - `signatures`: optional constructor signature list.
/// - `interface`: owning module interface.
///
/// Outputs:
/// - Constructor schemes when signatures exist.
/// - `None` when the interface has no constructor signatures.
///
/// Transformation:
/// - Parses fixed and vararg constructor parameter types, expands local
///   aliases, qualifies local type names, and defaults unparseable annotations
///   to `Dynamic`.
pub(crate) fn parse_interface_constructor_schemes(
    signatures: Option<&[ConstructorSignature]>,
    interface: &ModuleInterface,
) -> Option<Vec<ConstructorScheme>> {
    let signatures = signatures?;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let schemes = signatures
        .iter()
        .filter(|signature| signature.public && signature.min_arity == signature.params.len())
        .map(|signature| {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;

            let fixed_params = signature
                .params
                .iter()
                .map(|param| {
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic)
                })
                .map(|param| expand_type_aliases(&param, &interface_aliases))
                .map(|param| qualify_type_names(&param, &qualified_names))
                .collect::<Vec<_>>();

            let vararg = signature.vararg.as_ref().map(|param| {
                let parsed =
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic);
                let parsed = expand_type_aliases(&parsed, &interface_aliases);
                qualify_type_names(&parsed, &qualified_names)
            });

            let ret = parse_type_expr(
                &signature.return_type,
                &alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let ret = expand_type_aliases(&ret, &interface_aliases);
            let ret = qualify_type_names(&ret, &qualified_names);

            ConstructorScheme {
                min_arity: signature.min_arity,
                fixed_params,
                vararg,
                ret,
            }
        })
        .collect::<Vec<_>>();

    Some(schemes)
}

/// Returns type names visible inside one module interface.
///
/// Inputs:
/// - `interface`: module interface with public and opaque type declarations.
///
/// Outputs:
/// - Set containing interface type names plus primitive type names.
///
/// Transformation:
/// - Merges public, opaque, and primitive names so interface type parsing can
///   recognize local type references.
pub(crate) fn interface_type_names(interface: &ModuleInterface) -> HashSet<String> {
    let mut names: HashSet<String> = interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .cloned()
        .collect();
    names.extend(primitive_type_names());
    names
}

/// Builds non-opaque type aliases declared by an interface.
///
/// Inputs:
/// - `interface`: module interface containing type bodies and type parameters.
///
/// Outputs:
/// - Alias map keyed by local type name.
///
/// Transformation:
/// - Parses each non-opaque type body, normalizes unions, records type
///   parameters, and skips opaque types so their representation stays hidden.
pub(crate) fn interface_type_aliases(interface: &ModuleInterface) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    let alias_names = interface_type_names(interface);

    for (name, variants) in &interface.type_bodies {
        if interface.opaque_types.contains(name) {
            continue;
        }

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut params = Vec::new();
        for param in interface.type_params.get(name).into_iter().flatten() {
            vars.insert(normalize_type_param_name(param), next_var);
            params.push(next_var);
            next_var += 1;
        }

        let body = normalize_union(
            variants
                .iter()
                .filter_map(|variant| {
                    parse_type_expr(variant, &alias_names, &mut vars, &mut next_var)
                })
                .collect(),
        );
        aliases.insert(
            name.clone(),
            TypeAlias {
                params,
                body,
                is_opaque: false,
            },
        );
    }

    aliases
}

/// Returns qualified names for interface-owned public and opaque types.
///
/// Inputs:
/// - `interface`: module interface with module name and type declarations.
///
/// Outputs:
/// - Map from local type name to qualified module/type identity.
///
/// Transformation:
/// - Attaches the interface module path to every exported type name so imported
///   signatures can resolve local references deterministically.
pub(crate) fn interface_qualified_type_names(
    interface: &ModuleInterface,
) -> HashMap<String, QualifiedTypeName> {
    interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .map(|name| {
            (
                name.clone(),
                QualifiedTypeName {
                    module: interface.module.clone(),
                    name: name.clone(),
                },
            )
        })
        .collect()
}

/// Qualifies local named types using an interface-owned type map.
///
/// Inputs:
/// - `ty`: parsed type to rewrite.
/// - `qualified_names`: local names mapped to qualified type identities.
///
/// Outputs:
/// - Type with matching unqualified names rewritten to qualified names.
///
/// Transformation:
/// - Recursively walks compound types and rewrites only names present in the
///   interface qualification map.
pub(crate) fn qualify_type_names(
    ty: &Type,
    qualified_names: &HashMap<String, QualifiedTypeName>,
) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            let args = args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect();
            if let Some(qualified) = qualified_names.get(name) {
                Type::Named {
                    module: Some(qualified.module.clone()),
                    name: qualified.name.clone(),
                    args,
                }
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args,
                }
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect(),
        },
        Type::List(inner) => Type::List(Box::new(qualify_type_names(inner, qualified_names))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: qualify_type_names(&field.value, qualified_names),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| qualify_type_names(param, qualified_names))
                .collect(),
            ret: Box::new(qualify_type_names(ret, qualified_names)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(qualify_type_names(elem, qualified_names)),
        },
        other => other.clone(),
    }
}
