use super::*;

/// Binds explicit call type arguments to instantiated function type variables.
///
/// Inputs:
/// - `scheme`: already instantiated function scheme.
/// - `function_name`: optional diagnostic context.
/// - `type_args`: explicit call-site type arguments.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - `Ok(())` when explicit arguments match generic arity and parse.
/// - `Err(message)` when a call supplies the wrong number of type args or an
///   unparseable type argument.
///
/// Transformation:
/// - Collects type variables from parameters, return type, and bounds in first
///   occurrence order, parses explicit source type arguments with the current
///   module type context, and unifies each variable with its supplied type.
pub(super) fn bind_explicit_call_type_args(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if type_args.is_empty() {
        return Ok(());
    }

    let generic_vars = ordered_function_scheme_type_vars(scheme);
    if generic_vars.len() != type_args.len() {
        let name = function_name.unwrap_or("function");
        return Err(format!(
            "wrong type-argument arity for {}: expected {} type args, found {}",
            name,
            generic_vars.len(),
            type_args.len()
        ));
    }

    for (index, (var, type_arg)) in generic_vars.into_iter().zip(type_args.iter()).enumerate() {
        let supplied = parse_explicit_call_type_arg(type_arg, ctx)?;
        if let Some(generic_param) = scheme.generic_params.get(index) {
            validate_explicit_hkt_type_arg_variance(generic_param, &supplied, type_arg, ctx)?;
        }
        if let Err(message) = unify(&Type::Var(var), &supplied, subst) {
            return Err(message);
        }
    }

    Ok(())
}

/// Validates explicit HKT constructor arguments against source slot variance.
///
/// Inputs:
/// - `generic_param`: source generic parameter text such as `F[+_]`.
/// - `supplied`: parsed explicit type argument.
/// - `type_arg`: original syntax-output type argument for diagnostics.
/// - `ctx`: expression context containing visible type aliases.
///
/// Output:
/// - `Ok(())` when no variance requirement exists or the supplied constructor
///   satisfies every required slot.
/// - `Err(message)` when an explicit constructor argument violates an HKT slot
///   variance requirement.
///
/// Transformation:
/// - Reads `+_` and `-_` markers from the generic parameter, resolves the
///   supplied bare constructor's declared variance, and rejects invariant or
///   opposite-variance constructors before ordinary unification can hide the
///   mismatch.
fn validate_explicit_hkt_type_arg_variance(
    generic_param: &str,
    supplied: &Type,
    type_arg: &SyntaxTypeOutput,
    ctx: &ExprInferContext<'_>,
) -> Result<(), String> {
    let requirements = hkt_slot_variance_requirements(generic_param);
    if requirements.iter().all(Option::is_none) {
        return Ok(());
    }

    let Some((module, name)) = bare_constructor_type_arg(supplied) else {
        return Err(format!(
            "explicit type argument `{}` must be a bare type constructor for `{}`",
            type_arg.text, generic_param
        ));
    };
    let actual = explicit_constructor_variance(module, name, ctx.aliases, requirements.len());
    for (slot_index, requirement) in requirements.iter().enumerate() {
        let Some(required) = requirement else {
            continue;
        };
        let actual = actual
            .get(slot_index)
            .copied()
            .unwrap_or(Variance::Invariant);
        if actual != *required {
            return Err(format!(
                "explicit type argument `{}` for `{}` requires slot {} to be {}, found {} constructor",
                type_arg.text,
                generic_param,
                slot_index + 1,
                variance_display(*required),
                variance_display(actual)
            ));
        }
    }

    Ok(())
}

/// Extracts HKT slot variance requirements from a generic parameter.
///
/// Inputs:
/// - `generic_param`: source generic parameter text.
///
/// Output:
/// - One optional variance requirement per HKT slot.
///
/// Transformation:
/// - Treats `_` as unconstrained, `+_` as covariant, and `-_` as
///   contravariant while ignoring outer type-parameter variance.
fn hkt_slot_variance_requirements(generic_param: &str) -> Vec<Option<Variance>> {
    let Some((_, slots)) = generic_param.split_once('[') else {
        return Vec::new();
    };
    let Some((slots, _)) = slots.rsplit_once(']') else {
        return Vec::new();
    };
    slots
        .split(',')
        .map(|slot| match compact_spaces(slot).as_str() {
            "+_" => Some(Variance::Covariant),
            "-_" => Some(Variance::Contravariant),
            _ => None,
        })
        .collect()
}

/// Extracts a bare constructor from an explicit type argument.
///
/// Inputs:
/// - `supplied`: parsed explicit type argument.
///
/// Output:
/// - Module/name pair for bare named constructors, otherwise `None`.
///
/// Transformation:
/// - Keeps HKT constructor arguments distinct from applied concrete types such
///   as `Option[Int]`.
fn bare_constructor_type_arg(supplied: &Type) -> Option<(Option<&str>, &str)> {
    match supplied {
        Type::Named { module, name, args } if args.is_empty() => {
            Some((module.as_deref(), name.as_str()))
        }
        _ => None,
    }
}

/// Resolves declared variance for an explicit constructor argument.
///
/// Inputs:
/// - `module` and `name`: bare constructor identity.
/// - `aliases`: visible alias metadata.
/// - `fallback_len`: number of slots that need a conservative fallback.
///
/// Output:
/// - Constructor parameter variances, or invariant fallbacks when the
///   constructor has no visible metadata.
///
/// Transformation:
/// - Uses the same conservative rule as named-type subtyping: unknown generic
///   constructors are invariant, while selected built-in collection
///   constructors expose known covariance.
fn explicit_constructor_variance(
    module: Option<&str>,
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
    fallback_len: usize,
) -> Vec<Variance> {
    if let Some(module) = module {
        let qualified = format!("{}.{}", module, name);
        if let Some(alias) = aliases.get(&qualified) {
            return alias.param_variance.clone();
        }
    }
    if let Some(alias) = aliases.get(name) {
        return alias.param_variance.clone();
    }
    match name {
        "List" => vec![Variance::Covariant],
        "Map" => vec![Variance::Covariant, Variance::Covariant],
        "FixedArray" => vec![Variance::Covariant],
        _ => vec![Variance::Invariant; fallback_len],
    }
}

/// Renders variance for call-site diagnostics.
///
/// Inputs:
/// - `variance`: declared or inferred variance direction.
///
/// Output:
/// - Stable lower-case diagnostic text.
///
/// Transformation:
/// - Keeps user-facing diagnostics independent from Rust enum debug output.
fn variance_display(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "invariant",
        Variance::Covariant => "covariant",
        Variance::Contravariant => "contravariant",
    }
}

/// Parses one explicit call type argument into the typechecker model.
///
/// Inputs:
/// - `type_arg`: syntax-output type argument text.
/// - `ctx`: active expression context containing aliases and imported type
///   names.
///
/// Output:
/// - Parsed and qualified type.
///
/// Transformation:
/// - Reuses normal type-expression parsing, preserves bare generic alias
///   constructors for HKT arguments, expands local value-level aliases, and
///   qualifies selected imported type names so call-site generics obey the same
///   naming rules as annotations.
fn parse_explicit_call_type_arg(
    type_arg: &SyntaxTypeOutput,
    ctx: &ExprInferContext<'_>,
) -> Result<Type, String> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let parsed = parse_type_expr(&type_arg.text, ctx.alias_names, &mut vars, &mut next_var)
        .ok_or_else(|| format!("cannot parse call type argument `{}`", type_arg.text))?;
    if is_bare_generic_alias_constructor(&parsed, ctx.aliases) {
        return Ok(qualify_type_names(&parsed, ctx.imported_type_names));
    }
    let parsed = expand_type_aliases(&parsed, ctx.aliases);
    Ok(qualify_type_names(&parsed, ctx.imported_type_names))
}

/// Returns whether an explicit type argument names a generic alias constructor.
///
/// Inputs:
/// - `ty`: parsed explicit call type argument.
/// - `aliases`: visible local/imported type aliases.
///
/// Output:
/// - `true` when the argument is a bare `TypeName` whose alias has parameters.
///
/// Transformation:
/// - Keeps higher-kinded explicit call arguments such as `identity[Option, Int]`
///   as constructors instead of expanding `Option[T]` into its union body.
fn is_bare_generic_alias_constructor(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } if args.is_empty() => aliases
            .get(name)
            .is_some_and(|alias| !alias.params.is_empty()),
        Type::Named {
            module: Some(module),
            name,
            args,
        } if args.is_empty() => {
            let qualified = format!("{}.{}", module, name);
            aliases
                .get(&qualified)
                .is_some_and(|alias| !alias.params.is_empty())
        }
        _ => false,
    }
}

/// Collects function-scheme type variables in deterministic first-use order.
///
/// Inputs:
/// - `scheme`: instantiated function scheme.
///
/// Output:
/// - Type variable identifiers in the order explicit call type arguments bind
///   to them.
///
/// Transformation:
/// - Traverses parameters, return type, and bounds recursively while preserving
///   first occurrence order and removing duplicates.
fn ordered_function_scheme_type_vars(scheme: &FunctionScheme) -> Vec<TypeVarId> {
    let mut vars = Vec::new();
    for param in &scheme.params {
        collect_type_vars_in_order(param, &mut vars);
    }
    collect_type_vars_in_order(&scheme.ret, &mut vars);
    for bound in &scheme.bounds {
        for arg in &bound.trait_args {
            collect_type_vars_in_order(arg, &mut vars);
        }
    }
    vars
}

/// Collects type variables from one type in first-use order.
///
/// Inputs:
/// - `ty`: type to inspect.
/// - `vars`: accumulator preserving existing order.
///
/// Output:
/// - No direct return value; `vars` is extended in place.
///
/// Transformation:
/// - Recursively walks structural type forms and appends each unseen
///   `Type::Var` identifier exactly once.
fn collect_type_vars_in_order(ty: &Type, vars: &mut Vec<TypeVarId>) {
    match ty {
        Type::Var(id) => {
            if !vars.contains(id) {
                vars.push(*id);
            }
        }
        Type::Apply { constructor, args } => {
            if !vars.contains(constructor) {
                vars.push(*constructor);
            }
            for arg in args {
                collect_type_vars_in_order(arg, vars);
            }
        }
        Type::Existential { params, body } => {
            collect_type_vars_in_order_excluding(body, vars, params);
        }
        Type::List(inner) => collect_type_vars_in_order(inner, vars),
        Type::Tuple(items) | Type::Union(items) => {
            for item in items {
                collect_type_vars_in_order(item, vars);
            }
        }
        Type::Map(fields) => {
            for field in fields {
                collect_type_vars_in_order(&field.value, vars);
            }
        }
        Type::FixedArray { elem, .. } => collect_type_vars_in_order(elem, vars),
        Type::Named { args, .. } => {
            for arg in args {
                collect_type_vars_in_order(arg, vars);
            }
        }
        Type::Function { params, ret } => {
            for param in params {
                collect_type_vars_in_order(param, vars);
            }
            collect_type_vars_in_order(ret, vars);
        }
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => {}
    }
}

/// Collects free type variables while excluding existential binders.
///
/// Inputs:
/// - `ty`: type tree to inspect.
/// - `vars`: accumulator preserving first-use order.
/// - `excluded`: locally bound type-variable ids to ignore.
///
/// Output:
/// - No direct return value; free variables are appended to `vars`.
///
/// Transformation:
/// - Walks the same structures as `collect_type_vars_in_order`, extending the
///   exclusion set through nested existential scopes.
fn collect_type_vars_in_order_excluding(
    ty: &Type,
    vars: &mut Vec<TypeVarId>,
    excluded: &[TypeVarId],
) {
    match ty {
        Type::Var(id) => {
            if !excluded.contains(id) && !vars.contains(id) {
                vars.push(*id);
            }
        }
        Type::Apply { constructor, args } => {
            if !excluded.contains(constructor) && !vars.contains(constructor) {
                vars.push(*constructor);
            }
            for arg in args {
                collect_type_vars_in_order_excluding(arg, vars, excluded);
            }
        }
        Type::Existential { params, body } => {
            let mut nested_excluded = excluded.to_vec();
            nested_excluded.extend(params);
            collect_type_vars_in_order_excluding(body, vars, &nested_excluded);
        }
        Type::List(inner) => collect_type_vars_in_order_excluding(inner, vars, excluded),
        Type::Tuple(items) | Type::Union(items) => {
            for item in items {
                collect_type_vars_in_order_excluding(item, vars, excluded);
            }
        }
        Type::Map(fields) => {
            for field in fields {
                collect_type_vars_in_order_excluding(&field.value, vars, excluded);
            }
        }
        Type::FixedArray { elem, .. } => {
            collect_type_vars_in_order_excluding(elem, vars, excluded);
        }
        Type::Named { args, .. } => {
            for arg in args {
                collect_type_vars_in_order_excluding(arg, vars, excluded);
            }
        }
        Type::Function { params, ret } => {
            for param in params {
                collect_type_vars_in_order_excluding(param, vars, excluded);
            }
            collect_type_vars_in_order_excluding(ret, vars, excluded);
        }
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => {}
    }
}
