use super::*;

/// Infers a local named call.
///
/// Inputs:
/// - `expr`: call expression without an explicit remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `type_args`: explicit call-site type arguments.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved local call return type.
///
/// Transformation:
/// - Checks constructors, local functions, imports, aliases, trait shorthands,
///   receiver forms, and intrinsics in source-call priority order.
pub(super) fn infer_syntax_local_call(
    function_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if is_removed_implicit_builtin_call(function_name, arg_types.len()) {
        errors.push(format!(
            "`{function_name}/{}` is not part of the implicit prelude; import or define it explicitly",
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if let Some(scheme) = builtin_call(function_name, arg_types.len()) {
        if let Err(message) =
            infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst)
        {
            errors.push(message);
        }
        return scheme.ret;
    }

    if let Some(ty) = infer_syntax_imported_function_call(
        function_name,
        arg_types,
        type_args,
        arg_names,
        ctx,
        subst,
        errors,
    ) {
        return ty;
    }

    let exact_local_symbol = ctx
        .local_fns
        .get(&(function_name.to_string(), arg_types.len()));
    if exact_local_symbol.is_none() {
        if let Some(symbol) =
            local_function_symbol_accepting_call(function_name, arg_types.len(), ctx)
        {
            let param_names = symbol
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>();
            if !validate_named_call_args(function_name, arg_names, &param_names, errors) {
                return Type::Dynamic;
            }
            if !validate_required_defaulted_local_call_args(
                function_name,
                arg_names,
                symbol,
                errors,
            ) {
                return Type::Dynamic;
            }
            if let Some(scheme) = parse_symbol_scheme(symbol) {
                let effective_arg_types = complete_defaulted_local_call_arg_types(
                    arg_types,
                    arg_names,
                    &param_names,
                    &scheme,
                );
                match infer_function_with_explicit_type_args(
                    &scheme,
                    Some(function_name),
                    &effective_arg_types,
                    type_args,
                    ctx,
                    subst,
                ) {
                    Ok(ty) => return ty,
                    Err(message) => {
                        errors.push(message);
                        return Type::Dynamic;
                    }
                }
            }
        }
    }

    let effective_arg_types = if let Some(symbol) = exact_local_symbol {
        let param_names = symbol
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        if !validate_named_call_args(function_name, arg_names, &param_names, errors) {
            return Type::Dynamic;
        }
        reorder_named_call_arg_types(arg_types, arg_names, &param_names)
    } else {
        arg_types.to_vec()
    };

    if let Some(schemes) = ctx
        .signatures
        .get(&(function_name.to_string(), effective_arg_types.len()))
    {
        match infer_function_scheme_overload_with_explicit_type_args(
            schemes,
            function_name,
            &effective_arg_types,
            type_args,
            ctx,
            subst,
        ) {
            Ok(ty) => return ty,
            Err(message) => {
                errors.push(message);
                return Type::Dynamic;
            }
        }
    }

    if let Some(symbol) = ctx
        .local_fns
        .get(&(function_name.to_string(), effective_arg_types.len()))
    {
        if let Some(scheme) = parse_symbol_scheme(symbol) {
            match infer_function_with_explicit_type_args(
                &scheme,
                Some(function_name),
                &effective_arg_types,
                type_args,
                ctx,
                subst,
            ) {
                Ok(ty) => return ty,
                Err(message) => {
                    errors.push(message);
                    return Type::Dynamic;
                }
            }
        }
    }

    Type::Dynamic
}

/// Selects a local function symbol that accepts a default-aware call arity.
///
/// Inputs:
/// - `function_name`: source call head.
/// - `supplied_arity`: number of arguments written at the call site.
/// - `ctx`: expression inference context containing resolved local functions.
///
/// Output:
/// - Matching local function symbol when exactly one declaration accepts the
///   supplied arity after considering trailing default parameters.
/// - `None` when no local function or multiple overloads match.
///
/// Transformation:
/// - Computes each function's required arity from parameters without defaults
///   and treats defaulted trailing parameters as optional for local calls.
fn local_function_symbol_accepting_call<'a>(
    function_name: &str,
    supplied_arity: usize,
    ctx: &'a ExprInferContext<'_>,
) -> Option<&'a FunctionSymbol> {
    let mut matches = ctx.local_fns.iter().filter_map(|((name, _arity), symbol)| {
        (name == function_name && local_function_symbol_accepts_arity(symbol, supplied_arity))
            .then_some(symbol)
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Checks whether one local function symbol accepts a supplied arity.
///
/// Inputs:
/// - `symbol`: resolved local function metadata.
/// - `supplied_arity`: number of call-site arguments.
///
/// Output:
/// - `true` when supplied arguments cover all required parameters and do not
///   exceed the full declaration arity.
///
/// Transformation:
/// - Counts parameters with no default as required and compares that arity
///   range with the call-site arity.
fn local_function_symbol_accepts_arity(symbol: &FunctionSymbol, supplied_arity: usize) -> bool {
    let required = symbol
        .params
        .iter()
        .filter(|param| param.default_text.is_none())
        .count();
    supplied_arity >= required && supplied_arity <= symbol.params.len()
}

/// Validates that default-aware local calls do not omit required parameters.
///
/// Inputs:
/// - `function_name`: source call head for diagnostics.
/// - `arg_names`: optional source names parallel to supplied arguments.
/// - `symbol`: selected local function symbol.
/// - `errors`: output diagnostics.
///
/// Output:
/// - `true` when every required parameter is supplied.
///
/// Transformation:
/// - Builds the set of supplied declaration slots from positional and named
///   arguments, then rejects any missing parameter without a default value.
fn validate_required_defaulted_local_call_args(
    function_name: &str,
    arg_names: &[Option<String>],
    symbol: &FunctionSymbol,
    errors: &mut Vec<String>,
) -> bool {
    let param_names = symbol
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<Vec<_>>();
    let supplied = supplied_named_parameter_slots(arg_names, &param_names);
    for (index, param) in symbol.params.iter().enumerate() {
        if param.default_text.is_none() && !supplied.contains(&index) {
            errors.push(format!(
                "missing required argument `{}` for call to `{}`",
                param.name, function_name
            ));
        }
    }

    errors.is_empty()
}

/// Completes local call argument types by inserting defaulted parameter types.
///
/// Inputs:
/// - `arg_types`: inferred source argument types.
/// - `arg_names`: optional source names parallel to `arg_types`.
/// - `param_names`: declaration parameter names.
/// - `scheme`: selected local function type scheme.
///
/// Output:
/// - Argument types in full declaration order, including the expected type for
///   omitted defaulted parameters.
///
/// Transformation:
/// - Places positional and named arguments into declaration slots and fills
///   omitted slots with the corresponding parameter type. Declaration default
///   expressions were already typechecked separately, so the parameter type is
///   the correct call-site type for omitted values.
fn complete_defaulted_local_call_arg_types(
    arg_types: &[Type],
    arg_names: &[Option<String>],
    param_names: &[&str],
    scheme: &FunctionScheme,
) -> Vec<Type> {
    complete_defaulted_call_arg_types(arg_types, arg_names, param_names, &scheme.params)
}
