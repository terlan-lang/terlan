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
/// - Uses the declaration-ordered generic identities retained by the scheme,
///   parses explicit source type arguments with the current
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

    let generic_vars = scheme.generic_vars.clone();
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
            if generic_param.trim().starts_with("const ") {
                bind_explicit_const_arg(var, supplied, subst)?;
                continue;
            }
        }
        unify(&Type::Var(var), &supplied, subst)?;
    }

    Ok(())
}
fn bind_explicit_const_arg(
    var: TypeVarId,
    supplied: Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if !matches!(
        supplied,
        Type::LiteralInt(_) | Type::LiteralBool(_) | Type::LiteralAtom(_)
    ) {
        return Err("const generic argument must be an Int, Bool, or Atom literal".to_string());
    }
    if let Some(existing) = subst.get(&var) {
        if existing == &supplied {
            return Ok(());
        }
        return Err(format!(
            "const generic argument conflicts with existing `{}`",
            pretty_type(existing)
        ));
    }
    subst.insert(var, supplied);
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
        "Array" => vec![Variance::Covariant],
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
pub(super) fn parse_explicit_call_type_arg(
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
