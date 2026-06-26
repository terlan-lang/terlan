use super::*;

/// Lowers a named call expression.
///
/// Inputs:
/// - `expr`: syntax-output `Call` node whose first child is the callee and
///   remaining children are source arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang call expression, constructor expression, trait dispatch expression,
///   or bridge expression selected by call routing.
///
/// Transformation:
/// - Splits the formal syntax call into callee, arguments, and optional remote
///   qualifier, then delegates to the shared call-router so direct calls and
///   pipe-forwarded calls use one backend dispatch path.
pub(super) fn lower_syntax_call_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let callee = expr.children.first()?;
    let args = &expr.children[1..];
    lower_syntax_call_parts(
        callee,
        args,
        &expr.arg_names,
        expr.remote.as_deref(),
        ctx,
        env,
    )
}

/// Lowers dedicated function-value invocation syntax.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` node created from `callee.(args)`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang `Apply` expression that invokes the lowered callee value.
///
/// Transformation:
/// - Lowers the callee as an ordinary value, including local function captures
///   such as `fun increment/1`, lowers each argument, and emits callable-value
///   application. This keeps `Expr.(...)` separate from named `Name(...)`
///   calls in the backend.
pub(super) fn lower_syntax_function_value_call_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let callee = expr.children.first()?;
    let args = expr.children[1..]
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    Some(ErlExpr::Apply {
        callee: Box::new(lower_syntax_expr_with_env(callee, ctx, env)?),
        args,
    })
}

/// Lowers default-aware call arguments in declaration order.
///
/// Inputs:
/// - `args`: source call arguments in written order.
/// - `arg_names`: optional source names parallel to `args`.
/// - `param_names`: declaration parameter names in backend argument order.
/// - `fixed_arity`: number of fixed declaration slots.
/// - `defaults`: default expressions parallel to fixed declaration slots.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang arguments in declaration order, including lowered defaults for
///   omitted slots.
///
/// Transformation:
/// - Keeps pure positional calls in source order and appends omitted trailing
///   defaults. For named calls, places supplied expressions into declaration
///   slots and lowers missing slots from `defaults`.
pub(super) fn lower_syntax_defaulted_call_args(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    param_names: &[String],
    fixed_arity: usize,
    defaults: &[Option<SyntaxExprOutput>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    if !arg_names.iter().any(Option::is_some) {
        let mut lowered = args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        for default in defaults.iter().skip(args.len()) {
            lowered.push(lower_syntax_expr_with_env(default.as_ref()?, ctx, env)?);
        }
        return Some(lowered);
    }

    let mut ordered = vec![None; fixed_arity];
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_ref) {
            Some(name) => {
                let param_index = param_names.iter().position(|param| param == name)?;
                if param_index < ordered.len() {
                    ordered[param_index] = Some(arg);
                }
            }
            None => {
                if index < ordered.len() {
                    ordered[index] = Some(arg);
                }
            }
        }
    }

    ordered
        .into_iter()
        .enumerate()
        .map(|(index, arg)| match arg {
            Some(arg) => lower_syntax_expr_with_env(arg, ctx, env),
            None => lower_syntax_expr_with_env(defaults[index].as_ref()?, ctx, env),
        })
        .collect()
}

/// Routes a named call to the appropriate backend lowering path.
///
/// Inputs:
/// - `callee`: syntax-output callee expression.
/// - `args`: source-visible call argument expressions.
/// - `remote`: optional explicit remote qualifier from syntax output.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Lowered Erlang expression for a local call, remote call, constructor call,
///   receiver call, trait dispatch, intrinsic call, or imported function call.
///
/// Transformation:
/// - Applies Terlan call precedence in one place: receiver-like calls first,
///   then constructors/intrinsics/traits/imports/generic dictionaries, then
///   ordinary Erlang call emission as the final syntax-bridge fallback.
fn lower_syntax_call_parts(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    remote: Option<&str>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if remote.is_none() {
        if let Some(expr) = lower_syntax_list_each_receiver_method_call(callee, args, ctx, env) {
            return Some(expr);
        }
        if let Some(expr) =
            lower_syntax_primitive_receiver_method_call(callee, args, arg_names, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) = lower_syntax_receiver_method_call(callee, args, arg_names, ctx, env) {
            return Some(expr);
        }
        if let Some((module, function)) = syntax_method_shaped_remote_call_parts(callee, ctx, env) {
            if let Some(expr) =
                lower_syntax_primitive_intrinsic_call(&module, &function, args, ctx, env)
            {
                return Some(expr);
            }
            if let Some(expr) =
                lower_syntax_native_vector_module_call(&module, &function, args, ctx, env)
            {
                return Some(expr);
            }
            return Some(ErlExpr::Call {
                module: Some(module),
                function,
                args: args
                    .iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            });
        }
    }

    let callee_name = syntax_expr_name(callee)?;

    if remote.is_none() {
        if let Some(expr) = lower_syntax_type_intrinsic_call(callee_name, args, env) {
            return Some(expr);
        }
        if args.len() == 1 && ctx.opaque_constructors.contains(callee_name) {
            return lower_syntax_expr_with_env(&args[0], ctx, env);
        }
        if env.current_constructor_target.as_deref() == Some(callee_name) {
            if let Some(expr) =
                lower_syntax_default_struct_constructor_call(callee_name, args, arg_names, ctx, env)
            {
                return Some(expr);
            }
        }
        if let Some(target) = ctx.constructor_target(callee_name, args.len()) {
            return lower_syntax_explicit_constructor_call(target, args, arg_names, ctx, env);
        }
        if let Some(target) = ctx.imported_constructor_target(callee_name, args.len()) {
            return lower_syntax_remote_constructor_call(target, args, ctx, env);
        }
        if let Some(target) = ctx.alias_constructor_call_target(callee_name, args.len()) {
            return lower_syntax_alias_constructor_expr(target, args, arg_names, ctx, env);
        }
    } else if let Some(remote) = remote {
        if let Some(expr) =
            lower_syntax_primitive_intrinsic_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) =
            lower_syntax_runtime_capability_call(remote, callee_name, args, arg_names, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) = lower_syntax_native_vector_module_call(
            &ctx.resolve_remote_module(remote),
            callee_name,
            args,
            ctx,
            env,
        ) {
            return Some(expr);
        }
        if let Some(target) = ctx.remote_constructor_target(remote, callee_name, args.len()) {
            return lower_syntax_remote_constructor_call(target, args, ctx, env);
        }
        if let Some(target) = ctx.remote_alias_constructor_target(remote, callee_name, args.len()) {
            return lower_syntax_alias_constructor_expr(target, args, arg_names, ctx, env);
        }
        if let Some(expr) =
            lower_syntax_local_trait_receiver_method_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) =
            lower_syntax_bound_trait_method_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        let (trait_remote, explicit_trait_type_arg) = split_explicit_trait_call_target(remote);
        if let Some((module_name, source_trait_name)) = ctx.imported_trait_alias(&trait_remote) {
            if let Some(type_arg) = explicit_trait_type_arg
                .clone()
                .or_else(|| {
                    args.first()
                        .and_then(|arg| infer_syntax_trait_dispatch_type(arg, ctx, env))
                })
                .map(|type_arg| qualify_imported_type_text(&type_arg, &ctx.imported_type_refs))
            {
                if let Some(expr) = lower_syntax_std_trait_intrinsic_call(
                    module_name,
                    source_trait_name,
                    callee_name,
                    &type_arg,
                    args,
                    ctx,
                    env,
                ) {
                    return Some(expr);
                }
                if let Some(wrapper_type_arg) =
                    ctx.imported_trait_conformance_wrapper_type(&trait_remote, &type_arg)
                {
                    if let Some(expr) = lower_syntax_std_trait_intrinsic_call(
                        module_name,
                        source_trait_name,
                        callee_name,
                        wrapper_type_arg,
                        args,
                        ctx,
                        env,
                    ) {
                        return Some(expr);
                    }
                    let mut lowered_args = Vec::with_capacity(args.len() + 1);
                    lowered_args.push(trait_dictionary_expr(source_trait_name, callee_name));
                    lowered_args.extend(
                        args.iter()
                            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                            .collect::<Option<Vec<_>>>()?,
                    );
                    return Some(ErlExpr::Call {
                        module: Some(ctx.resolve_remote_module(module_name)),
                        function: typed_trait_method_wrapper_name(
                            source_trait_name,
                            callee_name,
                            wrapper_type_arg,
                        ),
                        args: lowered_args,
                    });
                }
            }
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(trait_dictionary_expr(source_trait_name, callee_name));
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: Some(ctx.resolve_remote_module(module_name)),
                function: trait_method_wrapper_name(source_trait_name, callee_name),
                args: lowered_args,
            });
        }
        if let Some(wrapper) = ctx.trait_method_wrapper(remote, callee_name) {
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(trait_dictionary_expr(remote, callee_name));
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: None,
                function: wrapper.clone(),
                args: lowered_args,
            });
        }
        if let Some(type_arg) = args
            .first()
            .and_then(|arg| infer_syntax_trait_dispatch_type(arg, ctx, env))
        {
            if let Some(wrapper) = ctx.typed_trait_method_wrapper(remote, callee_name, &type_arg) {
                let mut lowered_args = Vec::with_capacity(args.len() + 1);
                lowered_args.push(trait_dictionary_expr(remote, callee_name));
                lowered_args.extend(
                    args.iter()
                        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                        .collect::<Option<Vec<_>>>()?,
                );
                return Some(ErlExpr::Call {
                    module: None,
                    function: wrapper.clone(),
                    args: lowered_args,
                });
            }
        }
    }

    if remote.is_none() {
        if let Some(expr) = lower_syntax_template_call(callee_name, args, arg_names, ctx, env) {
            return Some(expr);
        }
        if !ctx.constructors.contains_key(callee_name) {
            if let Some(expr) =
                lower_syntax_default_struct_constructor_call(callee_name, args, arg_names, ctx, env)
            {
                return Some(expr);
            }
        }
    }

    if is_upper_identifier(callee_name) {
        return None;
    }

    if remote.is_none() && env.value_locals.contains(callee_name) {
        return Some(ErlExpr::Call {
            module: None,
            function: sanitize_erlang_var(callee_name),
            args: args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        });
    }

    if remote.is_none() {
        if let Some(target) = ctx.generic_function_target(callee_name, args.len()) {
            let mut lowered_args = lower_syntax_generic_bound_dictionaries(target, args, ctx, env)?;
            lowered_args.extend(lower_syntax_named_call_args(
                args,
                arg_names,
                ctx.local_function_param_names(callee_name, args.len()),
                ctx,
                env,
            )?);
            return Some(ErlExpr::Call {
                module: None,
                function: callee_name.to_string(),
                args: lowered_args,
            });
        }
        if let Some(target) = ctx.imported_function_target(callee_name, args.len()) {
            if let Some(expr) = lower_syntax_primitive_intrinsic_call(
                &target.module,
                &target.function,
                args,
                ctx,
                env,
            ) {
                return Some(expr);
            }
            if let Some(expr) = lower_syntax_runtime_capability_call(
                &target.module,
                &target.function,
                args,
                arg_names,
                ctx,
                env,
            ) {
                return Some(expr);
            }
            if let Some(expr) = lower_syntax_native_vector_module_call(
                &target.module,
                &target.function,
                args,
                ctx,
                env,
            ) {
                return Some(expr);
            }
            return Some(ErlExpr::Call {
                module: Some(ctx.resolve_remote_module(&target.module)),
                function: target.function.clone(),
                args: lower_syntax_defaulted_imported_function_args(
                    args, arg_names, target, ctx, env,
                )?,
            });
        }
        if let Some(target) = ctx.local_function_target(callee_name, args.len()) {
            return Some(ErlExpr::Call {
                module: None,
                function: callee_name.to_string(),
                args: lower_syntax_defaulted_local_function_args(
                    args, arg_names, target, ctx, env,
                )?,
            });
        }
    }

    Some(ErlExpr::Call {
        module: remote.map(|module| ctx.resolve_remote_module(module)),
        function: callee_name.to_string(),
        args: lower_syntax_named_call_args(
            args,
            arg_names,
            if remote.is_none() {
                ctx.local_function_param_names(callee_name, args.len())
            } else {
                None
            },
            ctx,
            env,
        )?,
    })
}

/// Lowers compiler-provided struct field constructors to Erlang records.
///
/// Inputs:
/// - `callee_name`: local struct name used as the source call head.
/// - `args` and `arg_names`: call argument expressions and field labels.
/// - `ctx`, `env`: active module and local lowering state.
///
/// Output:
/// - Erlang record construction when `callee_name` names a local struct and all
///   supplied arguments are named field assignments.
///
/// Transformation:
/// - Converts canonical Terlan `User(name = value)` construction into the
///   backend record expression used by BEAM, leaving defaulted fields to the
///   generated record declaration.
fn lower_syntax_default_struct_constructor_call(
    callee_name: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let fields = ctx.struct_field_types.get(callee_name)?;
    if args.len() != arg_names.len() {
        return None;
    }

    let lowered_fields = args
        .iter()
        .zip(arg_names.iter())
        .map(|(arg, name)| {
            let name = name.as_ref()?;
            if !fields.contains_key(name) {
                return None;
            }
            Some(ErlMapField {
                key: name.clone(),
                value: lower_syntax_expr_with_env(arg, ctx, env)?,
                required: false,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    Some(ErlExpr::RecordConstruct {
        name: callee_name.to_string(),
        fields: lowered_fields,
    })
}

/// Lowers local function arguments with named and defaulted parameter support.
///
/// Inputs:
/// - `args`: source call arguments in written order.
/// - `arg_names`: optional source names parallel to `args`.
/// - `target`: selected local function declaration metadata.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang call arguments in full declaration order, including lowered default
///   expressions for omitted defaulted parameters.
///
/// Transformation:
/// - Places positional and named source arguments into declaration slots and
///   fills omitted slots with declaration defaults. Typechecking owns required
///   argument diagnostics, so missing non-default slots fail lowering by
///   returning `None`.
fn lower_syntax_defaulted_local_function_args(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    target: &SyntaxLocalFunctionTarget,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    lower_syntax_defaulted_call_args(
        args,
        arg_names,
        &target.param_names,
        target.fixed_arity,
        &target.defaults,
        ctx,
        env,
    )
}

/// Lowers imported function arguments with named/defaulted parameter support.
///
/// Inputs:
/// - `args`: source call arguments in written order.
/// - `arg_names`: optional source names parallel to `args`.
/// - `target`: selected imported function target.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang call arguments in provider declaration order, including lowered
///   interface default expressions for omitted defaulted parameters.
///
/// Transformation:
/// - Places positional and named source arguments into imported signature
///   slots and fills omitted slots from parsed `.typi` default expressions.
fn lower_syntax_defaulted_imported_function_args(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    target: &SyntaxImportedFunctionTarget,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    lower_syntax_defaulted_call_args(
        args,
        arg_names,
        &target.param_names,
        target.fixed_arity,
        &target.defaults,
        ctx,
        env,
    )
}

/// Lowers call arguments with optional named-argument reordering.
///
/// Inputs:
/// - `args`: source argument expressions in written order.
/// - `arg_names`: optional source names parallel to `args`.
/// - `param_names`: declaration parameter names in backend call order.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Lowered Erlang arguments in declaration order when named metadata is
///   available, otherwise in source order.
///
/// Transformation:
/// - Keeps positional calls unchanged, places named arguments into their
///   declaration slots, and recursively lowers the resulting expression order.
///   Typechecking validates names before emission, so missing slots fall back
///   to source order rather than inventing backend values.
pub(super) fn lower_syntax_named_call_args(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    param_names: Option<&Vec<String>>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let ordered = ordered_named_call_args(args, arg_names, param_names)
        .unwrap_or_else(|| args.iter().collect::<Vec<_>>());
    ordered
        .into_iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect()
}

/// Orders source call arguments according to declaration parameter names.
///
/// Inputs:
/// - `args`: source argument expressions.
/// - `arg_names`: optional argument names parallel to `args`.
/// - `param_names`: declaration parameter names.
///
/// Output:
/// - `Some` ordered expression references when named arguments can be mapped.
/// - `None` when the call is positional or lacks parameter metadata.
///
/// Transformation:
/// - Copies leading positional expressions to their matching indexes and moves
///   named expressions into the slot with the same declaration parameter name.
fn ordered_named_call_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &[Option<String>],
    param_names: Option<&Vec<String>>,
) -> Option<Vec<&'a SyntaxExprOutput>> {
    if !arg_names.iter().any(Option::is_some) {
        return None;
    }
    let param_names = param_names?;
    if param_names.len() != args.len() {
        return None;
    }

    let mut ordered = vec![None; args.len()];
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_ref) {
            Some(name) => {
                let param_index = param_names.iter().position(|param| param == name)?;
                if param_index < ordered.len() {
                    ordered[param_index] = Some(arg);
                }
            }
            None => {
                if index < ordered.len() {
                    ordered[index] = Some(arg);
                }
            }
        }
    }

    ordered.into_iter().collect()
}

/// Lowers declaration-site trait dispatch to a receiver-method call.
///
/// Inputs:
/// - `trait_name`: remote segment from a call such as `Show.to_string(value)`.
/// - `method`: trait method segment.
/// - `args`: source call arguments, where the first argument is the receiver
///   value required by the trait method.
/// - `ctx`, `env`: syntax lowering context and local type environment.
///
/// Output:
/// - `Some(ErlExpr::Call)` when a local trait declares the method and the first
///   argument's inferred type has a matching local receiver method.
/// - `None` when the call is not a supported declaration-site trait dispatch.
///
/// Transformation:
/// - Reuses the existing receiver-method backend ABI: the trait call's first
///   argument becomes the receiver argument to the generated Erlang function,
///   followed by any additional trait method arguments.
fn lower_syntax_local_trait_receiver_method_call(
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !ctx.has_local_trait_method(trait_name, method) {
        return None;
    }

    let receiver = args.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, ctx, env)?;
    let method_arity = args.len().checked_sub(1)?;
    ctx.receiver_method_target(&receiver_type, method, method_arity)?;

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Lowers a local receiver-method call.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: ordinary method arguments after the receiver.
/// - `ctx`: syntax lowering context containing local receiver-method identity.
/// - `env`: lexical environment used for conservative receiver type inference.
///
/// Output:
/// - `Some(ErlExpr::Call)` when the current module declares the selected
///   receiver method for the inferred receiver type.
/// - `None` when the callee is not a local receiver-method call.
///
/// Transformation:
/// - Rewrites `receiver.method(args...)` to the backend receiver-first calling
///   convention `method(receiver, args...)`, matching how method declarations
///   are lowered.
fn lower_syntax_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, ctx, env)?;
    if let Some(expr) =
        lower_syntax_native_vector_receiver_call(&receiver_type, method, receiver, args, ctx, env)
    {
        return Some(expr);
    }
    if let Some(expr) =
        lower_syntax_json_receiver_call(&receiver_type, method, receiver, args, arg_names, ctx, env)
    {
        return Some(expr);
    }
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, args.len())?;
    let _receiver_is_mutable = receiver_target.mutable;

    let mut lowered_args = Vec::with_capacity(args.len() + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
        args,
        arg_names,
        receiver_target,
        ctx,
        env,
    )?);

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: lowered_args,
    })
}

/// Lowers `std.data.Json.Json` receiver methods to the SafeNative std module.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type text.
/// - `method`: source receiver method name.
/// - `receiver`: receiver expression.
/// - `args`: non-receiver method arguments.
/// - `arg_names`: optional names for non-receiver arguments.
/// - `ctx`, `env`: active syntax lowering state.
///
/// Output:
/// - `Some(ErlExpr::Call)` for the current JSON receiver API.
/// - `None` for non-JSON receivers, unknown methods, or malformed arguments.
///
/// Transformation:
/// - Rewrites `json.get("name")` to `std_data_json:get(Json, <<"name">>)`,
///   matching the generated SafeNative module ABI where receiver methods are
///   exported as receiver-first Erlang functions.
fn lower_syntax_json_receiver_call(
    receiver_type: &str,
    method: &str,
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if receiver_type_head(receiver_type) != "Json" {
        return None;
    }
    let param_names = json_receiver_method_param_names(method, args.len())?;
    let mut lowered_args = Vec::with_capacity(args.len() + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(lower_syntax_named_call_args(
        args,
        arg_names,
        Some(&param_names),
        ctx,
        env,
    )?);

    Some(ErlExpr::Call {
        module: Some("std_data_json".to_string()),
        function: method.to_string(),
        args: lowered_args,
    })
}

/// Returns the source parameter names for JSON receiver methods.
///
/// Inputs:
/// - `method`: JSON receiver method name.
/// - `arg_count`: non-receiver argument count.
///
/// Output:
/// - Parameter names in exported Erlang ABI order.
/// - `None` when the method or arity is not a JSON receiver API.
///
/// Transformation:
/// - Mirrors `std/data/json.terl` so named receiver-method calls preserve
///   source semantics during syntax-bridge emission.
fn json_receiver_method_param_names(method: &str, arg_count: usize) -> Option<Vec<String>> {
    let names = match (method, arg_count) {
        ("push", 1) => vec!["value"],
        ("put", 2) => vec!["key", "value"],
        ("get", 1) => vec!["key"],
        ("length", 0) => vec![],
        ("at", 1) => vec!["index"],
        ("as_string", 0) => vec![],
        ("as_int", 0) => vec![],
        ("as_float", 0) => vec![],
        ("as_bool", 0) => vec![],
        ("is_null", 0) => vec![],
        _ => return None,
    };
    Some(names.into_iter().map(str::to_string).collect())
}

/// Lowers receiver-method arguments with named and defaulted parameter support.
///
/// Inputs:
/// - `args`: source non-receiver arguments in written order.
/// - `arg_names`: optional source names parallel to `args`.
/// - `target`: selected receiver-method target metadata.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang non-receiver arguments in full declaration order, including
///   lowered default expressions for omitted defaulted parameters.
///
/// Transformation:
/// - Places positional and named source arguments into method declaration slots
///   and fills omitted slots with declaration defaults. The receiver itself is
///   handled by the caller because backend methods use receiver-first ABI.
pub(super) fn lower_syntax_defaulted_receiver_method_args(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    target: &SyntaxReceiverMethodTarget,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    lower_syntax_defaulted_call_args(
        args,
        arg_names,
        &target.param_names,
        target.fixed_arity,
        &target.defaults,
        ctx,
        env,
    )
}

/// Extracts module/function names from method-shaped remote-call syntax.
///
/// Inputs:
/// - `callee`: the syntax-output callee of a parsed call expression.
/// - `ctx`: syntax lowering context used to resolve imported module aliases.
/// - `env`: local value environment used to distinguish receiver methods from
///   module calls.
///
/// Output:
/// - `Some((module, function))` for two-part calls whose receiver is not a
///   local value binding, otherwise `None`.
///
/// Transformation:
/// - Recognizes the field-access tree produced by the canonical
///   `MethodCallSuffix` parser and reclassifies non-local receiver names as
///   Erlang remote calls for the syntax bridge. Local receiver names
///   stay outside this path and must be handled by later semantic method
///   resolution.
pub(super) fn syntax_method_shaped_remote_call_parts(
    callee: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<(String, String)> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let function = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let module = syntax_expr_name(receiver)?;
    if env.value_locals.contains(module) {
        None
    } else {
        Some((ctx.resolve_remote_module(module), function.to_string()))
    }
}

/// Lowers a pipe-forwarding binary expression.
///
/// Inputs:
/// - `left`: expression whose value flows into the right-hand call.
/// - `right`: pipe target expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Lowered call expression with `left` inserted as the first argument.
/// - Raw fallback expression when the right side is not a call.
///
/// Transformation:
/// - Preserves mutable receiver pipe forwarding as a special case, otherwise
///   rewrites `left |> f(args...)` to the same call router as `f(left, args...)`
///   so ordinary calls, trait calls, and imports share behavior.
pub(super) fn lower_syntax_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) {
        return Some(ErlExpr::Raw(format!(
            "{} |> {}",
            lower_syntax_expr_with_env(left, ctx, env)?.render(),
            lower_syntax_expr_with_env(right, ctx, env)?.render()
        )));
    }

    if let Some(expr) = lower_syntax_mutable_receiver_pipe_forward(left, right, ctx, env) {
        return Some(expr);
    }
    if let Some(expr) = lower_syntax_receiver_method_pipe_forward(left, right, ctx, env) {
        return Some(expr);
    }

    let callee = right.children.first()?;
    let mut args = Vec::with_capacity(right.children.len());
    args.push(left.clone());
    args.extend(right.children.iter().skip(1).cloned());
    lower_syntax_call_parts(callee, &args, &[], right.remote.as_deref(), ctx, env)
}

/// Lowers immutable receiver-method pipe forwarding.
///
/// Inputs:
/// - `left`: source pipe receiver expression.
/// - `right`: source call expression on the right side of `|>`.
/// - `ctx`, `env`: syntax lowering context and lexical type environment.
///
/// Output:
/// - `Some(ErlExpr::Call)` when `right` names a declared immutable receiver
///   method for the inferred receiver type.
/// - `None` for mutable receiver methods and non-receiver pipe targets.
///
/// Transformation:
/// - Rewrites `receiver |> method(args...)` into the receiver-first backend
///   call shape and fills omitted receiver-method default arguments.
fn lower_syntax_receiver_method_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) || right.remote.is_some() {
        return None;
    }

    let callee = right.children.first()?;
    let method = syntax_expr_name(callee)?;
    let arity = right.children.len().checked_sub(1)?;
    let receiver_type = infer_syntax_pipe_receiver_type(left, ctx, env)?;
    let args = &right.children[1..];
    if let Some(expr) =
        lower_syntax_native_vector_receiver_call(&receiver_type, method, left, args, ctx, env)
    {
        return Some(expr);
    }
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity)?;
    if receiver_target.mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(receiver_target.fixed_arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(left, ctx, env)?);
    lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
        args,
        &right.arg_names,
        receiver_target,
        ctx,
        env,
    )?);

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: lowered_args,
    })
}

/// Lowers mutable receiver-method pipe forwarding.
///
/// Inputs:
/// - `left`: source pipe receiver expression.
/// - `right`: source call expression on the right side of `|>`.
/// - `ctx`, `env`: syntax lowering context and lexical type environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when `right` names a declared mutable receiver method
///   for the inferred receiver type or a compiler-known direct HTTP response
///   helper.
/// - `None` for non-call right sides, remote calls, non-method calls,
///   immutable receiver methods, or expressions whose receiver type cannot be
///   inferred from the syntax lowering environment.
///
/// Transformation:
/// - Rewrites `receiver |> mut_method(args...)` into a backend-local binding
///   whose value is the hidden updated receiver returned by the lowered mutable
///   method function. The binding result becomes the pipe expression value so
///   later pipe steps receive the updated receiver. Direct HTTP response
///   helpers use the same pipe result shape because the BEAM handler bridge
///   represents response edits as updated response tuples.
fn lower_syntax_mutable_receiver_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) || right.remote.is_some() {
        return None;
    }

    let callee = right.children.first()?;
    let method = syntax_expr_name(callee)?;
    let arity = right.children.len().checked_sub(1)?;
    let receiver_type = infer_syntax_pipe_receiver_type(left, ctx, env)?;
    let args = &right.children[1..];
    if is_native_vector_mutating_receiver_method(&receiver_type, method, arity) {
        let updated_receiver = "_TerlanMutReceiver".to_string();
        return Some(ErlExpr::Let {
            bindings: vec![ErlLetBinding {
                pattern: ErlPattern::Var(updated_receiver.clone()),
                value: lower_syntax_native_vector_receiver_call(
                    &receiver_type,
                    method,
                    left,
                    args,
                    ctx,
                    env,
                )?,
            }],
            body: Box::new(ErlExpr::Var(updated_receiver)),
        });
    }
    if let Some(value) = lower_http_response_receiver_method_call(
        &receiver_type,
        method,
        left,
        args,
        &right.arg_names,
        ctx,
        env,
    ) {
        let updated_receiver = "_TerlanMutReceiver".to_string();
        return Some(ErlExpr::Let {
            bindings: vec![ErlLetBinding {
                pattern: ErlPattern::Var(updated_receiver.clone()),
                value,
            }],
            body: Box::new(ErlExpr::Var(updated_receiver)),
        });
    }

    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity)?;
    if !receiver_target.mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(receiver_target.fixed_arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(left, ctx, env)?);
    lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
        args,
        &right.arg_names,
        receiver_target,
        ctx,
        env,
    )?);

    let updated_receiver = "_TerlanMutReceiver".to_string();
    Some(ErlExpr::Let {
        bindings: vec![ErlLetBinding {
            pattern: ErlPattern::Var(updated_receiver.clone()),
            value: ErlExpr::Call {
                module: None,
                function: method.to_string(),
                args: lowered_args,
            },
        }],
        body: Box::new(ErlExpr::Var(updated_receiver)),
    })
}

/// Infers the receiver type that should flow through a pipe chain.
///
/// Inputs:
/// - `expr`: source expression used as a pipe receiver.
/// - `env`: lexical lowering environment containing known value types.
///
/// Output:
/// - Normalized receiver type text when the receiver can be inferred.
/// - `None` when the expression shape has no receiver-type evidence.
///
/// Transformation:
/// - Reads ordinary expression receiver types through the existing trait
///   dispatch inference helper. For nested pipe expressions, follows the left
///   side of the pipe because mutable receiver pipe lowering preserves the
///   original receiver type across each mutating step.
fn infer_syntax_pipe_receiver_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    if matches!(expr.kind, SyntaxExprKind::BinaryOp) && expr.operator.as_deref() == Some("|>") {
        return infer_syntax_pipe_receiver_type(expr.children.first()?, ctx, env);
    }

    infer_syntax_trait_dispatch_type(expr, ctx, env)
}
