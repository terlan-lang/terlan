use super::*;

/// Collects callable signatures declared by syntax output.
///
/// Inputs:
/// - `module`: syntax-output module containing function and native config
///   declarations.
/// - `alias_names`: visible type names used while parsing annotations.
/// - `imported_type_names`, `imported_type_aliases`, and `local_aliases`:
///   imported/local type context used to expand aliases and qualify selected
///   imports.
///
/// Output:
/// - Function scheme candidate map keyed by source function name and arity.
///
/// Transformation:
/// - Lowers ordinary function annotations and native config signatures into
///   the shared `FunctionScheme` model consumed by declaration checking and
///   expression inference.
pub(super) fn collect_syntax_function_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> HashMap<(String, usize), Vec<FunctionScheme>> {
    let mut map = HashMap::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                generic_bounds,
                ..
            } => {
                let scheme = function_decl_to_scheme(
                    &params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    &return_type.text,
                    generic_bounds,
                    alias_names,
                    imported_type_names,
                    imported_type_aliases,
                    local_aliases,
                );
                map.entry((name.clone(), params.len()))
                    .or_insert_with(Vec::new)
                    .push(scheme);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for native_sig in extract_native_function_signatures(text) {
                    let arg_types = native_sig
                        .params
                        .iter()
                        .map(|(_, annotation)| annotation.clone())
                        .collect::<Vec<_>>();
                    let scheme = function_decl_to_scheme(
                        &arg_types,
                        &native_sig.return_type,
                        &Vec::new(),
                        alias_names,
                        imported_type_names,
                        imported_type_aliases,
                        local_aliases,
                    );
                    map.entry((native_sig.name, native_sig.arity))
                        .or_insert_with(Vec::new)
                        .push(scheme);
                }
            }
            _ => {}
        }
    }

    map
}

/// Lowers one callable type signature into a typechecker function scheme.
///
/// Inputs:
/// - `param_annotations`: source type annotations for callable parameters.
/// - `return_annotation`: source return type annotation.
/// - `generic_bounds`: source generic constraint strings.
/// - `alias_names`, `imported_type_names`, `imported_type_aliases`, and
///   `local_aliases`: visible type context used for parsing, alias expansion,
///   and imported-name qualification.
///
/// Output:
/// - Parsed function scheme containing parameter types, return type, and
///   instantiated generic bounds.
///
/// Transformation:
/// - Parses annotations, preserves imported nominal references, expands other
///   aliases, qualifies selected imported type names, and lowers constraints
///   into `FunctionBound` values.
pub(super) fn function_decl_to_scheme(
    param_annotations: &[String],
    return_annotation: &str,
    generic_bounds: &[String],
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> FunctionScheme {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;

    let params = param_annotations
        .iter()
        .map(|annotation| {
            let parsed = parse_type_expr(annotation, alias_names, &mut vars, &mut next_var)
                .unwrap_or(Type::Dynamic);
            let parsed = expand_imported_aliases_except_named(
                &parsed,
                imported_type_aliases,
                imported_type_names,
                local_aliases,
            );
            qualify_type_names(&parsed, imported_type_names)
        })
        .collect::<Vec<_>>();
    let ret = parse_type_expr(return_annotation, alias_names, &mut vars, &mut next_var)
        .unwrap_or(Type::Dynamic);
    let ret = expand_imported_aliases_except_named(
        &ret,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let ret = qualify_type_names(&ret, imported_type_names);

    let bounds = parse_generic_bounds(generic_bounds, &vars, alias_names)
        .into_iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name,
            trait_args: bound
                .trait_args
                .into_iter()
                .map(|arg| {
                    let arg = expand_imported_aliases_except_named(
                        &arg,
                        imported_type_aliases,
                        imported_type_names,
                        local_aliases,
                    );
                    qualify_type_names(&arg, imported_type_names)
                })
                .collect(),
        })
        .collect();

    FunctionScheme {
        params,
        ret,
        bounds,
    }
}

/// Parses source generic constraints into function bounds.
///
/// Inputs:
/// - `generic_bounds`: raw source bound strings from a callable declaration.
/// - `vars`: generic type variables visible in that declaration.
/// - `alias_names`: visible type names used while parsing bound arguments.
///
/// Output:
/// - Function-bound list that can be checked during callable body inference.
///
/// Transformation:
/// - Supports both `T: Trait` and direct trait-reference forms, splits
///   top-level `+` constraints, and fills omitted trait arguments with the
///   constrained type variable.
pub(super) fn parse_generic_bounds(
    generic_bounds: &[String],
    vars: &HashMap<String, TypeVarId>,
    alias_names: &HashSet<String>,
) -> Vec<FunctionBound> {
    let mut bounds = Vec::new();
    for bound in generic_bounds {
        if let Some((left, right)) = bound.split_once(':') {
            let type_var = normalize_type_param_name(left.trim());
            let Some(&type_var_id) = vars.get(&type_var) else {
                continue;
            };

            for trait_expr in split_top_level_plus(right.trim()) {
                let Some(mut parsed_bound) =
                    parse_function_bound_trait_ref(&trait_expr, vars, alias_names)
                else {
                    continue;
                };
                if parsed_bound.trait_args.is_empty() {
                    parsed_bound.trait_args.push(Type::Var(type_var_id));
                }
                bounds.push(parsed_bound);
            }
            continue;
        }

        if let Some(parsed_bound) = parse_function_bound_trait_ref(bound, vars, alias_names) {
            bounds.push(parsed_bound);
        }
    }

    bounds
}

/// Converts one trait-reference constraint into a function bound.
///
/// Inputs:
/// - `trait_ref`: trait reference text such as `Eq[A]` or `Show[String]`.
/// - `vars`: generic type variables visible in the callable signature.
/// - `alias_names`: type aliases visible to type-expression parsing.
///
/// Output:
/// - `Some(FunctionBound)` when the trait reference parses and every type
///   argument can be converted to a typechecker `Type`.
/// - `None` when the reference is malformed or contains unsupported type
///   syntax.
///
/// Transformation:
/// - Parses the trait reference, preserves the trait name, and lowers type
///   arguments into semantic type payloads without attaching the bound to a
///   specific parameter name. This is the canonical `[Eq[A]]` constraint-list
///   path.
fn parse_function_bound_trait_ref(
    trait_ref: &str,
    vars: &HashMap<String, TypeVarId>,
    alias_names: &HashSet<String>,
) -> Option<FunctionBound> {
    let trait_instance = parse_trait_instance_from_text(trait_ref)?;
    let mut bound_arg_vars = vars.clone();
    let mut next_bound_arg_var = bound_arg_vars.len();
    let mut args = Vec::new();

    for raw_arg in &trait_instance.type_args {
        args.push(parse_type_expr(
            raw_arg,
            alias_names,
            &mut bound_arg_vars,
            &mut next_bound_arg_var,
        )?);
    }

    Some(FunctionBound {
        trait_name: trait_instance.name,
        trait_args: args,
    })
}

/// Expands imported aliases while preserving explicitly imported named types.
///
/// Inputs:
/// - `ty`: parsed type expression to normalize.
/// - `imported_aliases`: aliases exported by provider interfaces.
/// - `imported_names`: selected imported type names that should stay nominal.
/// - `local_aliases`: local aliases that can be expanded inside imported
///   nominal type arguments.
///
/// Output:
/// - Normalized type expression suitable for signature comparison.
///
/// Transformation:
/// - Recursively expands aliases in compound types, but leaves selected
///   imported named types as named references so opaque/imported identity is
///   not erased during type checking.
pub(super) fn expand_imported_aliases_except_named(
    ty: &Type,
    imported_aliases: &HashMap<String, TypeAlias>,
    imported_names: &HashMap<String, QualifiedTypeName>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> Type {
    match ty {
        Type::Named { name, args, .. } if imported_names.contains_key(name) => {
            let args = args
                .iter()
                .map(|arg| {
                    let arg = expand_type_aliases(arg, local_aliases);
                    expand_imported_aliases_except_named(
                        &arg,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect();
            match ty {
                Type::Named { module, name, .. } => Type::Named {
                    module: module.clone(),
                    name: name.clone(),
                    args,
                },
                _ => ty.clone(),
            }
        }
        Type::List(inner) => Type::List(Box::new(expand_imported_aliases_except_named(
            inner,
            imported_aliases,
            imported_names,
            local_aliases,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| {
                    expand_imported_aliases_except_named(
                        item,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| {
                    expand_imported_aliases_except_named(
                        item,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_imported_aliases_except_named(
                        &field.value,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    ),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| {
                    expand_imported_aliases_except_named(
                        param,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
            ret: Box::new(expand_imported_aliases_except_named(
                ret,
                imported_aliases,
                imported_names,
                local_aliases,
            )),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_imported_aliases_except_named(
                elem,
                imported_aliases,
                imported_names,
                local_aliases,
            )),
        },
        other => expand_type_aliases(other, imported_aliases),
    }
}

/// Collects constructor signatures declared by syntax output.
///
/// Inputs:
/// - `module`: syntax-output module containing constructor declarations.
/// - `alias_names`: visible type names used while parsing annotations.
/// - `imported_type_names`, `imported_type_aliases`, and `aliases`: visible
///   type context used for alias expansion and imported-name qualification.
///
/// Output:
/// - Constructor scheme map keyed by constructor name.
///
/// Transformation:
/// - Parses each constructor clause into fixed parameters, optional vararg
///   parameter, minimum arity, and qualified return type.
pub(super) fn collect_syntax_constructor_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, Vec<ConstructorScheme>> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            params,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        for param in params {
            vars.insert(normalize_type_param_name(param), next_var);
            next_var += 1;
        }

        let mut schemes = Vec::new();
        for clause in clauses {
            let mut fixed_params = Vec::new();
            let mut vararg = None;

            for param in &clause.params {
                let parsed = parse_type_expr(
                    &param.annotation.text,
                    alias_names,
                    &mut vars,
                    &mut next_var,
                )
                .unwrap_or(Type::Dynamic);
                let parsed = expand_type_aliases(&parsed, aliases);
                let parsed = qualify_type_names(&parsed, imported_type_names);
                if param.is_varargs {
                    vararg = Some(parsed);
                } else {
                    fixed_params.push(parsed);
                }
            }

            let ret = parse_type_expr(
                &clause.return_type.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let ret = expand_type_aliases(&ret, aliases);
            let ret = expand_type_aliases(&ret, imported_type_aliases);
            let ret = qualify_type_names(&ret, imported_type_names);

            schemes.push(ConstructorScheme {
                fixed_params,
                min_arity: clause
                    .params
                    .iter()
                    .filter(|param| !param.is_varargs && param.default.is_none())
                    .count(),
                vararg,
                ret,
            });
        }

        out.insert(name.clone(), schemes);
    }

    out
}

/// Returns primitive source type names for type-expression parsing.
///
/// Inputs:
/// - No runtime inputs.
///
/// Output:
/// - Set of built-in source type names that should parse as concrete types
///   instead of generic variables.
///
/// Transformation:
/// - Materializes the primitive type namespace used by trait conformance,
///   function signature, and receiver-method parsing so std summaries such as
///   `Show[Int]` remain concrete when imported by user modules.
pub(super) fn primitive_type_names() -> HashSet<String> {
    ["Bool", "Float", "Int", "String", "Unit"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Collects local struct field types for expression checking.
///
/// Inputs:
/// - `module`: syntax-output module containing struct declarations.
/// - `alias_names`: visible type names used while parsing field annotations.
///
/// Output:
/// - Map from struct name to field-name/type mappings.
///
/// Transformation:
/// - Parses each declared struct field annotation into the typechecker model
///   and stores only type information needed by record construction and field
///   access checks.
pub(super) fn collect_syntax_struct_fields(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
) -> HashMap<String, HashMap<String, Type>> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { name, fields, .. } = &declaration.payload else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut field_types = HashMap::new();

        for field in fields {
            let ty = parse_type_expr(
                &field.annotation.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            field_types.insert(field.name.clone(), ty);
        }

        out.insert(name.clone(), field_types);
    }

    out
}

/// Collects local source type names declared by a module.
///
/// Inputs:
/// - `module`: syntax-output module containing top-level declarations.
///
/// Output:
/// - Set of local type and struct names.
///
/// Transformation:
/// - Scans type and struct declarations only, producing the name surface used
///   by type parsing, receiver ownership checks, and imported alias merging.
pub(super) fn collect_syntax_type_names(module: &SyntaxModuleOutput) -> HashSet<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// Collects template property schemes declared by syntax output.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `alias_names`: visible type names used while parsing prop annotations.
///
/// Output:
/// - Map from template name to parsed property type scheme.
///
/// Transformation:
/// - Parses each template prop annotation into the typechecker model and drops
///   non-type template metadata that expression checking does not need.
pub(super) fn collect_syntax_template_schemes(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
) -> HashMap<String, TemplateScheme> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Template { name, props, .. } = &declaration.payload else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let prop_types = props
            .iter()
            .map(|prop| {
                let ty =
                    parse_type_expr(&prop.annotation.text, alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic);
                (prop.name.clone(), ty)
            })
            .collect::<HashMap<_, _>>();

        out.insert(name.clone(), TemplateScheme { props: prop_types });
    }

    out
}
