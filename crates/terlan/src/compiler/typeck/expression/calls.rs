use super::*;
mod function_value;
mod imported;
mod local;
mod macro_call;
mod pipe;
mod receiver;
mod remote;
mod template;

pub(super) use function_value::*;
use imported::*;
use local::*;
pub(super) use macro_call::*;
pub(super) use pipe::*;
use receiver::*;
pub(super) use remote::*;
use template::*;

/// Infers a named call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Infers argument types first, then routes the call through constructor,
///   local, remote, receiver, trait, intrinsic, and import dispatch.
pub(super) fn infer_syntax_call_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = infer_syntax_call_arg_types(expr, locals, ctx, subst, errors);
    infer_syntax_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Infers call argument types with available local-call context.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Argument types in source order.
///
/// Transformation:
/// - For ordinary local calls with a known exact signature, supplies each
///   argument's expected parameter type to contextual expressions such as
///   `Module.member` function values. All other calls use ordinary expression
///   inference.
fn infer_syntax_call_arg_types(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Vec<Type> {
    let expected_arg_types = exact_local_call_expected_arg_types(expr, ctx, subst);
    expr.children
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, arg)| {
            expected_arg_types
                .as_ref()
                .and_then(|expected| expected.get(index))
                .and_then(Option::as_ref)
                .and_then(|expected| {
                    infer_syntax_expr_with_expected(arg, expected, locals, ctx, subst, errors)
                })
                .unwrap_or_else(|| infer_syntax_expr(arg, locals, ctx, subst, errors))
        })
        .collect()
}

/// Builds positional expected argument types for an exact local function call.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `ctx`: expression inference context containing local function signatures.
/// - `subst`: active substitution table used when instantiating generic
///   function schemes.
///
/// Output:
/// - Expected source-argument types when the call head is a direct local
///   function and the supplied arity exactly matches a known declaration.
/// - `None` for remote calls, receiver calls, constructors, imports, or calls
///   whose local signature is not exact.
///
/// Transformation:
/// - Parses the local function scheme, instantiates generic variables, and
///   maps named arguments back to declaration slots without changing source
///   argument order.
fn exact_local_call_expected_arg_types(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Vec<Option<Type>>> {
    if expr.remote.is_some() || !syntax_callee_is_var(expr) {
        return None;
    }
    let function_name = syntax_callee_name(expr)?;
    let supplied_arity = expr.children.len().saturating_sub(1);
    let symbol = ctx
        .local_fns
        .get(&(function_name.to_string(), supplied_arity))?;
    let scheme = parse_symbol_scheme(symbol)?;
    let instantiated =
        instantiate_function_scheme_from(&scheme, next_function_type_var(&[], subst));
    let param_names = symbol
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<Vec<_>>();

    let mut next_positional = 0;
    Some(
        expr.arg_names
            .iter()
            .map(|arg_name| {
                let slot = if let Some(arg_name) = arg_name {
                    param_names.iter().position(|param| param == arg_name)
                } else {
                    while next_positional < param_names.len()
                        && expr
                            .arg_names
                            .iter()
                            .any(|name| name.as_deref() == Some(param_names[next_positional]))
                    {
                        next_positional += 1;
                    }
                    let slot = (next_positional < param_names.len()).then_some(next_positional);
                    next_positional += 1;
                    slot
                }?;
                instantiated.params.get(slot).cloned()
            })
            .collect(),
    )
}

/// Infers one expression with a contextual expected type when supported.
///
/// Inputs:
/// - `expr`: argument expression.
/// - `expected`: expected parameter type from the receiving call.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Contextually inferred type for supported forms.
/// - `None` when the expression has no contextual inference behavior.
///
/// Transformation:
/// - Currently uses function-value expectations to resolve overloaded
///   imported module-member references such as `Users.index`.
pub(crate) fn infer_syntax_expr_with_expected(
    expr: &SyntaxExprOutput,
    expected: &Type,
    _locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    infer_imported_module_member_function_value_with_expected(expr, expected, ctx, subst, errors)
}

/// Infers a call expression after argument types are known.
///
/// Inputs:
/// - `expr`: call expression.
/// - `arg_types`: previously inferred argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Applies call resolution without re-inferring arguments, allowing pipe and
///   function-value callers to share dispatch.
fn infer_syntax_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.remote.is_none() {
        if let Some(ty) =
            infer_syntax_primitive_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
        if let Some(ty) =
            infer_syntax_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let Some(function_name) = syntax_callee_name(expr) else {
        return Type::Dynamic;
    };

    if expr.remote.is_none() && syntax_callee_is_var(expr) {
        if let Some(template_result) =
            infer_syntax_template_call(function_name, expr, locals, ctx, subst, errors)
        {
            return template_result;
        }

        if ctx.current_constructor_target == Some(function_name) {
            if let Some(constructed) = infer_default_struct_constructor_call(
                function_name,
                &arg_types,
                &expr.arg_names,
                ctx,
                subst,
                errors,
            ) {
                return constructed;
            }
        }

        if let Some(constructed) = infer_constructor_call(
            function_name,
            &arg_types,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        ) {
            return constructed;
        }

        if let Some(imported) = ctx.constructor_aliases.get(function_name) {
            if let Some(interface) = ctx.interface_map.get(&imported.module) {
                if let Some(schemes) = parse_interface_constructor_schemes(
                    interface
                        .constructors
                        .get(&imported.name)
                        .map(Vec::as_slice),
                    interface,
                ) {
                    if let Some(constructed) = infer_constructor_schemes(
                        function_name,
                        &schemes,
                        &arg_types,
                        &expr.arg_names,
                        subst,
                        errors,
                    ) {
                        let interface_aliases = interface_type_aliases(interface);
                        return expand_type_aliases(&constructed, &interface_aliases);
                    }
                }
                if interface.opaque_types.contains(&imported.name) {
                    errors.push(format!(
                        "cannot construct opaque type {}.{} outside defining module",
                        imported.module, imported.name
                    ));
                    return Type::Dynamic;
                }
            }
        }

        if let Some(constructed) = infer_default_struct_constructor_call(
            function_name,
            &arg_types,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        ) {
            return constructed;
        }

        if let Some(constructed) =
            infer_opaque_constructor(function_name, &arg_types, ctx.aliases, errors)
        {
            return constructed;
        }

        if let Some(Type::Function { params, ret }) =
            locals.get(function_name).map(|ty| apply_subst(ty, subst))
        {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            return apply_subst(ret.as_ref(), subst);
        }

        if is_constructor_name(function_name) {
            let diagnostic_span = expr
                .children
                .first()
                .map(|callee| callee.span.into())
                .unwrap_or_else(|| expr.span.into());
            errors.push(spanned_expression_error(
                diagnostic_span,
                format!(
                    "unknown constructor {} / {}",
                    function_name,
                    arg_types.len()
                ),
            ));
            return Type::Dynamic;
        }
    }

    if let Some(module_name) = expr.remote.as_deref() {
        return infer_syntax_remote_call(
            module_name,
            function_name,
            arg_types,
            &expr.type_args,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        );
    }

    infer_syntax_local_call(
        function_name,
        arg_types,
        &expr.type_args,
        &expr.arg_names,
        ctx,
        subst,
        errors,
    )
}

/// Infers one receiver-method candidate with linked receiver generics.
///
/// Inputs:
/// - `candidate`: receiver dispatch candidate selected by method name/arity.
/// - `function_name`: optional diagnostic label for the method.
/// - `receiver_type`: inferred type of the receiver expression.
/// - `arg_types`: inferred non-receiver argument types after default
///   completion.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Candidate return type when the receiver and arguments satisfy the method
///   signature.
/// - Diagnostic text when this candidate does not accept the call.
///
/// Transformation:
/// - Builds a synthetic function scheme whose first parameter is the receiver
///   type, freshens that complete scheme once, then infers the call without a
///   second freshening step. This keeps receiver generics such as
///   `Map[K, V]` tied to method parameters and return types.
fn infer_receiver_method_candidate(
    candidate: &ReceiverMethodDispatchSignature,
    function_name: Option<&str>,
    receiver_type: &Type,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let candidate_receiver_type = qualify_imported_named_heads(&candidate.receiver_type, ctx);
    let receiver_type = qualify_imported_named_heads(receiver_type, ctx);
    let method_params = candidate
        .scheme
        .params
        .iter()
        .map(|param| qualify_imported_named_heads(param, ctx))
        .collect::<Vec<_>>();
    let method_return = qualify_imported_named_heads(&candidate.scheme.ret, ctx);
    let arg_types = arg_types
        .iter()
        .map(|arg| qualify_imported_named_heads(arg, ctx))
        .collect::<Vec<_>>();
    let mut params = Vec::with_capacity(arg_types.len() + 1);
    params.push(candidate_receiver_type);
    params.extend(method_params);
    let combined_scheme = FunctionScheme {
        params,
        ret: method_return,
        generic_params: candidate.scheme.generic_params.clone(),
        bounds: candidate.scheme.bounds.clone(),
    };
    let mut combined_args = Vec::with_capacity(arg_types.len() + 1);
    combined_args.push(receiver_type);
    combined_args.extend(arg_types);
    let instantiated = instantiate_function_scheme_from(
        &combined_scheme,
        next_function_type_var(&combined_args, subst),
    );

    infer_instantiated_function_with_bounds(
        &instantiated,
        function_name,
        &combined_args,
        ctx,
        subst,
    )
}

/// Extracts the source-visible callee name from a call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - Callee text when the call head is a variable or atom.
///
/// Transformation:
/// - Reads the first call child and returns its preserved text only for
///   name-like callee nodes.
pub(crate) fn syntax_callee_name(expr: &SyntaxExprOutput) -> Option<&str> {
    expr.children.first().and_then(|callee| match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref(),
        _ => None,
    })
}

/// Checks whether a call expression's callee is a variable node.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - `true` when the first child is a variable callee.
///
/// Transformation:
/// - Examines only the call head shape without resolving the identifier.
pub(super) fn syntax_callee_is_var(expr: &SyntaxExprOutput) -> bool {
    matches!(
        expr.children.first().map(|callee| callee.kind),
        Some(SyntaxExprKind::Var)
    )
}

/// Infers the compiler-provided field-assignment constructor for a struct.
///
/// Inputs:
/// - `function_name`: source call head, expected to name a visible struct.
/// - `arg_types`: inferred argument types in source order.
/// - `arg_names`: field names supplied by named call arguments.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type::Named)` when `function_name` resolves to a visible struct.
/// - `None` when the call head is not a struct name.
///
/// Transformation:
/// - Treats `User(name = value)` as the default struct constructor, requiring
///   explicit field assignments, rejecting unknown/duplicate/missing fields,
///   enforcing field visibility, and unifying each supplied value with the
///   declared field type.
fn infer_default_struct_constructor_call(
    function_name: &str,
    arg_types: &[Type],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let field_types = ctx.struct_fields.get(function_name)?;
    if ctx.constructors.contains_key(function_name)
        && ctx.current_constructor_target != Some(function_name)
    {
        errors.push(format!(
            "implicit struct constructor `{}` is disabled because explicit constructors are declared",
            function_name
        ));
        return Some(Type::Dynamic);
    }
    let mut supplied = HashSet::new();

    for (index, actual) in arg_types.iter().enumerate() {
        let Some(source_field_name) = arg_names.get(index).and_then(Option::as_deref) else {
            errors.push(format!(
                "struct constructor `{}` requires named field arguments",
                function_name
            ));
            continue;
        };
        let (field_name, requested_private) = split_private_field_spelling(source_field_name);
        if !supplied.insert(field_name.to_string()) {
            errors.push(format!(
                "duplicate field `{}` in struct constructor `{}`",
                field_name, function_name
            ));
            continue;
        }

        let Some(expected) = field_types.get(field_name) else {
            errors.push(format!(
                "unknown field `{}` on struct `{}`",
                field_name, function_name
            ));
            continue;
        };
        if let Some(message) = struct_field_visibility_error(
            function_name,
            field_name,
            requested_private,
            ctx.struct_field_visibility,
            ctx.imported_type_names,
        ) {
            errors.push(message);
        }

        let expected_expanded = expand_type_aliases(expected, ctx.aliases);
        let actual_expanded = expand_type_aliases(actual, ctx.aliases);
        if let Err(message) = unify(&expected_expanded, &actual_expanded, subst) {
            errors.push(format!(
                "field `{}` on struct `{}` {}",
                field_name, function_name, message
            ));
        }
    }

    for field_name in field_types.keys() {
        if !supplied.contains(field_name) {
            errors.push(format!(
                "missing field `{}` in struct constructor `{}`",
                field_name, function_name
            ));
        }
    }

    Some(Type::Named {
        module: None,
        name: function_name.to_string(),
        args: Vec::new(),
    })
}
