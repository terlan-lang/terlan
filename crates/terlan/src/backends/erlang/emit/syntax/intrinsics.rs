use super::*;

mod http;

pub(super) use http::{
    is_http_response_mutating_receiver_method, lower_http_response_builder_call,
    lower_http_response_receiver_method_call, lower_http_router_builder_call,
    lower_syntax_primitive_receiver_method_call,
};

/// Lowers selected std trait conformances through primitive intrinsics.
///
/// Inputs:
/// - `module_name`: provider module that owns the imported trait.
/// - `trait_name`: source trait name from the provider module.
/// - `method`: trait method name being called.
/// - `type_arg`: concrete conformance type selected from interface metadata.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for the primitive intrinsic when the imported std trait
///   conformance is compiler-owned.
/// - `None` for ordinary imported trait calls that should still use provider
///   wrappers.
///
/// Transformation:
/// - Keeps released std summary builds executable by mapping selected
///   std-owned conformances onto the same closed primitive intrinsic registry
///   used by direct std calls.
pub(super) fn lower_syntax_std_trait_intrinsic_call(
    module_name: &str,
    trait_name: &str,
    method: &str,
    type_arg: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if let Some(expr) =
        lower_syntax_std_collection_trait_bridge(module_name, trait_name, method, args, ctx, env)
    {
        return Some(expr);
    }

    let intrinsic =
        std_trait_primitive_intrinsic(module_name, trait_name, method, type_arg, args.len())?;
    let lowered_args = args
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Resolves a selected std trait conformance to a primitive intrinsic.
///
/// Inputs:
/// - `module_name`: canonical trait provider module.
/// - `trait_name`: trait declared by the provider module.
/// - `method`: trait method being called.
/// - `type_arg`: normalized concrete conformance type.
/// - `arity`: source-visible argument count for the call.
///
/// Output:
/// - Core primitive intrinsic for supported std trait conformances.
/// - `None` for unsupported traits, methods, types, or arities.
///
/// Transformation:
/// - Encodes executable std-facing conformance bridges. The bridge is
///   intentionally closed so user traits and non-selected std traits cannot
///   accidentally bypass provider wrapper generation.
fn std_trait_primitive_intrinsic(
    module_name: &str,
    trait_name: &str,
    method: &str,
    type_arg: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    let type_head = receiver_type_head(type_arg);
    match (module_name, trait_name, method, type_head.as_str(), arity) {
        ("std.core.String", "Show", "to_string", "Bool", 1) => {
            Some(CorePrimitiveIntrinsic::BoolToString)
        }
        ("std.core.String", "Show", "to_string", "Int", 1) => {
            Some(CorePrimitiveIntrinsic::IntToString)
        }
        ("std.core.String", "Show", "to_string", "Float", 1) => {
            Some(CorePrimitiveIntrinsic::FloatToString)
        }
        ("std.core.String", "Show", "to_string", "String", 1) => {
            Some(CorePrimitiveIntrinsic::StringToString)
        }
        ("std.core.String", "Parse", "from_string", "Bool", 1) => {
            Some(CorePrimitiveIntrinsic::BoolFromString)
        }
        ("std.core.String", "Parse", "from_string", "Int", 1) => {
            Some(CorePrimitiveIntrinsic::IntFromString)
        }
        ("std.core.String", "Parse", "from_string", "Float", 1) => {
            Some(CorePrimitiveIntrinsic::FloatFromString)
        }
        ("std.core.String", "Parse", "from_string", "String", 1) => {
            Some(CorePrimitiveIntrinsic::StringFromString)
        }
        ("std.core.Equal", "Equal", "equal", "Bool", 2)
        | ("std.core.Equal", "Equal", "equal", "Int", 2)
        | ("std.core.Equal", "Equal", "equal", "Float", 2)
        | ("std.core.Equal", "Equal", "equal", "Unit", 2)
        | ("std.core.Equal", "Equal", "equal", "Comparison", 2) => {
            Some(CorePrimitiveIntrinsic::BoolEqual)
        }
        ("std.core.Equal", "Equal", "equal", "String", 2) => {
            Some(CorePrimitiveIntrinsic::StringEqual)
        }
        ("std.collections.Iterable", "Iterable", "iterator", "List", 1) => {
            Some(CorePrimitiveIntrinsic::ListIterator)
        }
        ("std.collections.Iterable", "Iterable", "iterator", "Map", 1) => {
            Some(CorePrimitiveIntrinsic::MapIterator)
        }
        ("std.collections.Iterable", "Iterable", "iterator", "Set", 1) => {
            Some(CorePrimitiveIntrinsic::SetIterator)
        }
        _ => None,
    }
}

/// Lowers selected primitive std calls through compiler-owned intrinsics.
///
/// Inputs:
/// - `module`: source module path or alias owning the primitive operation.
/// - `function`: source function name.
/// - `args`: source argument expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for the primitive intrinsic when the call is selected.
/// - `None` for non-primitive calls or unsupported arity.
///
/// Transformation:
/// - Resolves module aliases, lowers arguments once through the syntax bridge,
///   maps portable `std.core.*` primitive APIs to CoreIR intrinsic identities,
///   and delegates to the shared CoreIR primitive BEAM lowering.
pub(super) fn lower_syntax_primitive_intrinsic_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let resolved_module = ctx.resolve_remote_module(module);
    let intrinsic = primitive_function_intrinsic(resolved_module.as_str(), function, args.len())?;
    if matches!(
        intrinsic,
        CorePrimitiveIntrinsic::TypeOf | CorePrimitiveIntrinsic::IsType
    ) {
        return lower_syntax_type_intrinsic_call(function, args, env);
    }
    let lowered_args = args
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Resolves a primitive std function call to its compiler-owned intrinsic.
///
/// Inputs:
/// - `module`: canonical std module path.
/// - `function`: source function name.
/// - `arity`: number of source arguments.
///
/// Output:
/// - Core primitive intrinsic id for selected primitive operations.
///
/// Transformation:
/// - Mirrors the CoreIR primitive registry at the transitional syntax bridge
///   boundary so selected imports and fully qualified primitive calls do not
///   emit calls to non-existent backend std modules.
pub(super) fn primitive_function_intrinsic(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (module, function, arity) {
        ("std.core.Type", "type_of", 1) => Some(CorePrimitiveIntrinsic::TypeOf),
        ("std.core.Type", "is_type", 2) => Some(CorePrimitiveIntrinsic::IsType),
        ("std.core.Bool", "equal", 2) => Some(CorePrimitiveIntrinsic::BoolEqual),
        ("std.core.Bool", "compare", 2) => Some(CorePrimitiveIntrinsic::BoolCompare),
        ("std.core.Bool", "to_string", 1) => Some(CorePrimitiveIntrinsic::BoolToString),
        ("std.core.Bool", "from_string", 1) => Some(CorePrimitiveIntrinsic::BoolFromString),
        ("std.core.Atom", "to_string", 1) => Some(CorePrimitiveIntrinsic::AtomToString),
        ("std.core.Int", "to_string", 1) => Some(CorePrimitiveIntrinsic::IntToString),
        ("std.core.Int", "from_string", 1) => Some(CorePrimitiveIntrinsic::IntFromString),
        ("std.core.Float", "to_string", 1) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("std.core.Float", "from_string", 1) => Some(CorePrimitiveIntrinsic::FloatFromString),
        ("std.core.String", "equal", 2) => Some(CorePrimitiveIntrinsic::StringEqual),
        ("std.core.String", "compare", 2) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("std.core.String", "to_string", 1) => Some(CorePrimitiveIntrinsic::StringToString),
        ("std.core.String", "from_string", 1) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("std.core.String", "is_empty", 1) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("std.core.String", "append", 2) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("std.core.String", "concat", 1) => Some(CorePrimitiveIntrinsic::StringConcat),
        ("std.core.String", "contains", 2) => Some(CorePrimitiveIntrinsic::StringContains),
        ("std.core.String", "starts_with", 2) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("std.core.String", "ends_with", 2) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("std.core.String", "length", 1) => Some(CorePrimitiveIntrinsic::StringLength),
        ("std.core.String", "byte_size", 1) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("std.core.String", "lowercase", 1) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("std.core.String", "uppercase", 1) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("std.core.String", "trim", 1) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("std.core.String", "trim_start", 1) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("std.core.String", "trim_end", 1) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        ("std.core.String", "replace", 3) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("std.core.String", "split", 2) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("std.core.String", "split_once", 2) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        ("std.collections.List", "new", 0) => Some(CorePrimitiveIntrinsic::ListNew),
        ("std.collections.List", "is_empty", 1) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("std.collections.List", "length", 1) => Some(CorePrimitiveIntrinsic::ListLength),
        ("std.collections.List", "first", 1) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("std.collections.List", "iterator", 1) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("std.collections.List", "push", 2) => Some(CorePrimitiveIntrinsic::ListPush),
        ("std.collections.List", "clear", 1) => Some(CorePrimitiveIntrinsic::ListClear),
        ("std.collections.Iterator", "next", 1) => Some(CorePrimitiveIntrinsic::IteratorNext),
        ("std.collections.Map", "new", 0) => Some(CorePrimitiveIntrinsic::MapNew),
        ("std.collections.Map", "from_entries", 1) => Some(CorePrimitiveIntrinsic::MapFromEntries),
        ("std.collections.Map", "is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("std.collections.Map", "size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("std.collections.Map", "get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("std.collections.Map", "contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("std.collections.Map", "iterator", 1) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("std.collections.Map", "put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("std.collections.Map", "remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("std.collections.Map", "clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        ("std.core.Object", "new", 0) => Some(CorePrimitiveIntrinsic::MapNew),
        ("std.core.Object", "from_entries", 1) => Some(CorePrimitiveIntrinsic::MapFromEntries),
        ("std.core.Object", "is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("std.core.Object", "size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("std.core.Object", "get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("std.core.Object", "contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("std.core.Object", "put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("std.core.Object", "remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("std.core.Object", "clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        ("std.collections.Set", "new", 0) => Some(CorePrimitiveIntrinsic::SetNew),
        ("std.collections.Set", "from_list", 1) => Some(CorePrimitiveIntrinsic::SetFromList),
        ("std.collections.Set", "is_empty", 1) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("std.collections.Set", "size", 1) => Some(CorePrimitiveIntrinsic::SetSize),
        ("std.collections.Set", "contains", 2) => Some(CorePrimitiveIntrinsic::SetContains),
        ("std.collections.Set", "iterator", 1) => Some(CorePrimitiveIntrinsic::SetIterator),
        ("std.collections.Set", "add", 2) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("std.collections.Set", "remove", 2) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("std.collections.Set", "clear", 1) => Some(CorePrimitiveIntrinsic::SetClear),
        ("std.core.Task", "done", 1) => Some(CorePrimitiveIntrinsic::TaskDone),
        ("std.core.Task", "result", 1) => Some(CorePrimitiveIntrinsic::TaskResult),
        ("std.beam.Agent", "start", 1) => Some(CorePrimitiveIntrinsic::BeamAgentStart),
        ("std.beam.Agent", "get", 1) => Some(CorePrimitiveIntrinsic::BeamAgentGet),
        ("std.beam.Agent", "get_and_update", 2) => {
            Some(CorePrimitiveIntrinsic::BeamAgentGetAndUpdate)
        }
        ("std.beam.Agent", "update", 2) => Some(CorePrimitiveIntrinsic::BeamAgentUpdate),
        ("std.beam.Agent", "cast", 2) => Some(CorePrimitiveIntrinsic::BeamAgentCast),
        ("std.beam.Agent", "stop", 1) => Some(CorePrimitiveIntrinsic::BeamAgentStop),
        ("std.beam.GenServer", "start", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerStart),
        ("std.beam.GenServer", "call", 2) => Some(CorePrimitiveIntrinsic::BeamGenServerCall),
        ("std.beam.GenServer", "cast", 2) => Some(CorePrimitiveIntrinsic::BeamGenServerCast),
        ("std.beam.GenServer", "stop", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerStop),
        ("std.beam.NativeBridge", "start", 1) => {
            Some(CorePrimitiveIntrinsic::BeamNativeBridgeStart)
        }
        ("std.beam.NativeBridge", "call", 2) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeCall),
        ("std.beam.NativeBridge", "dispose", 1) => {
            Some(CorePrimitiveIntrinsic::BeamNativeBridgeDispose)
        }
        ("std.beam.NativeBridge", "stop", 1) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeStop),
        ("std.beam.Bytes", "from_list", 1) => Some(CorePrimitiveIntrinsic::BeamBytesFromList),
        ("std.beam.Bytes", "to_list", 1) => Some(CorePrimitiveIntrinsic::BeamBytesToList),
        ("std.beam.Bytes", "length", 1) => Some(CorePrimitiveIntrinsic::BeamBytesLength),
        ("std.beam.Bytes", "concat", 2) => Some(CorePrimitiveIntrinsic::BeamBytesConcat),
        ("std.beam.Timeout", "milliseconds", 1) => {
            Some(CorePrimitiveIntrinsic::BeamTimeoutMilliseconds)
        }
        ("std.beam.Timeout", "forever", 0) => Some(CorePrimitiveIntrinsic::BeamTimeoutForever),
        ("std.beam.Tcp", "connect", 3) => Some(CorePrimitiveIntrinsic::BeamTcpConnect),
        ("std.beam.Tcp", "send", 2) => Some(CorePrimitiveIntrinsic::BeamTcpSend),
        ("std.beam.Tcp", "receive", 3) => Some(CorePrimitiveIntrinsic::BeamTcpReceive),
        ("std.beam.Tcp", "close", 1) => Some(CorePrimitiveIntrinsic::BeamTcpClose),
        ("std.beam.Port", "open", 1) => Some(CorePrimitiveIntrinsic::BeamPortOpen),
        ("std.beam.Port", "write", 2) => Some(CorePrimitiveIntrinsic::BeamPortWrite),
        ("std.beam.Port", "read", 3) => Some(CorePrimitiveIntrinsic::BeamPortRead),
        ("std.beam.Port", "close", 1) => Some(CorePrimitiveIntrinsic::BeamPortClose),
        ("std.beam.Supervisor", "child_spec", 1) => {
            Some(CorePrimitiveIntrinsic::BeamSupervisorChildSpec)
        }
        ("std.beam.Supervisor", "start", 2) => Some(CorePrimitiveIntrinsic::BeamSupervisorStart),
        ("std.beam.Supervisor", "stop", 2) => Some(CorePrimitiveIntrinsic::BeamSupervisorStop),
        ("std.beam.Task", "start", 1) => Some(CorePrimitiveIntrinsic::BeamTaskStart),
        ("std.beam.Task", "result", 1) => Some(CorePrimitiveIntrinsic::BeamTaskResult),
        ("std.beam.Task", "cancel", 1) => Some(CorePrimitiveIntrinsic::BeamTaskCancel),
        _ => None,
    }
}

/// Lowers selected std runtime capability calls from the direct syntax emitter.
///
/// Inputs:
/// - `module`: source module path or alias at the call boundary.
/// - `function`: source function name.
/// - `args`: source argument expressions.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` for runtime capabilities supported by the direct Erlang
///   syntax bridge emitter.
/// - `None` for ordinary source calls or malformed arguments.
///
/// Transformation:
/// - Resolves source module aliases, lowers arguments through the normal syntax
///   expression path, and delegates to the same backend runtime capability
///   lowering used by CoreIR emission.
pub(super) fn lower_syntax_runtime_capability_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let resolved_module = ctx.resolve_remote_module(module);
    if let Some(expr) =
        lower_http_response_builder_call(&resolved_module, function, args, arg_names, ctx, env)
    {
        return Some(expr);
    }
    if let Some(expr) =
        lower_http_router_builder_call(&resolved_module, function, args, arg_names, ctx, env)
    {
        return Some(expr);
    }
    match (resolved_module.as_str(), function, args.len()) {
        ("std.io.Console", "println", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_console_println(lowered_args)
        }
        ("std.log", "debug", 1)
        | ("std.log", "info", 1)
        | ("std.log", "warn", 1)
        | ("std.log", "error", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_console_println(lowered_args)
        }
        ("std.io.File", "exists", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_exists(lowered_args)
        }
        ("std.io.File", "read_text", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_read_text(lowered_args)
        }
        ("std.io.File", "write_text", 2) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_write_text(lowered_args)
        }
        ("std.io.File", "append_text", 2) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_append_text(lowered_args)
        }
        ("std.io.File", "delete", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_delete(lowered_args)
        }
        _ => None,
    }
}

/// Orders primitive receiver method arguments by compiler-known parameter name.
///
/// Inputs:
/// - `method`: primitive receiver method name.
/// - `args`: non-receiver source argument expressions.
/// - `arg_names`: optional source names parallel to `args`.
///
/// Output:
/// - Argument references in primitive ABI order.
/// - `None` when named metadata targets an unsupported primitive signature.
///
/// Transformation:
/// - Keeps positional calls unchanged and moves named primitive arguments into
///   the order expected by the backend intrinsic lowerer.
fn ordered_primitive_receiver_method_args<'a>(
    method: &str,
    args: &'a [SyntaxExprOutput],
    arg_names: &[Option<String>],
) -> Option<Vec<&'a SyntaxExprOutput>> {
    if !arg_names.iter().any(Option::is_some) {
        return Some(args.iter().collect());
    }
    let param_names = primitive_receiver_method_param_names(method, args.len())?;
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

/// Returns parameter names for compiler-owned primitive receiver methods.
///
/// Inputs:
/// - `method`: primitive receiver method name.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - Source parameter names in backend intrinsic order.
///
/// Transformation:
/// - Mirrors the typechecker primitive scalar surface so syntax lowering can
///   reorder named arguments without inferring semantic declarations.
fn primitive_receiver_method_param_names(
    method: &str,
    arg_count: usize,
) -> Option<Vec<&'static str>> {
    match (method, arg_count) {
        ("equal", 1) | ("compare", 1) => Some(vec!["other"]),
        ("append", 1) => Some(vec!["suffix"]),
        ("contains", 1) => Some(vec!["pattern"]),
        ("starts_with", 1) => Some(vec!["prefix"]),
        ("ends_with", 1) => Some(vec!["suffix"]),
        ("replace", 2) => Some(vec!["pattern", "replacement"]),
        ("split", 1) | ("split_once", 1) => Some(vec!["separator"]),
        ("params", 1) | ("query", 1) | ("headers", 1) | ("cookies", 1) => Some(vec!["name"]),
        (_, 0) => Some(Vec::new()),
        _ => None,
    }
}

/// Resolves a primitive receiver method to its compiler-owned intrinsic.
///
/// Inputs:
/// - `receiver_type`: normalized source type inferred for the receiver.
/// - `method`: method name from the field-access callee.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - Core primitive intrinsic id for supported primitive receiver calls.
/// - `None` for unsupported receiver types, methods, or arities.
///
/// Transformation:
/// - Keeps primitive receiver dispatch closed and explicit so source method
///   syntax cannot accidentally call arbitrary backend modules.
pub(super) fn primitive_receiver_method_intrinsic(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> Option<CorePrimitiveIntrinsic> {
    if receiver_type_has_head(receiver_type, "std.core.Task.Task")
        && matches!((method, arg_count), ("result", 0))
    {
        return Some(CorePrimitiveIntrinsic::TaskResult);
    }

    if receiver_type_has_head(receiver_type, "std.beam.Task.Task") {
        return match (method, arg_count) {
            ("result", 0) => Some(CorePrimitiveIntrinsic::BeamTaskResult),
            ("cancel", 0) => Some(CorePrimitiveIntrinsic::BeamTaskCancel),
            _ => None,
        };
    }

    if receiver_type_has_head(receiver_type, "std.beam.GenServer.ServerRef") {
        return match (method, arg_count) {
            ("call", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerCall),
            ("cast", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerCast),
            ("stop", 0) => Some(CorePrimitiveIntrinsic::BeamGenServerStop),
            _ => None,
        };
    }

    if let Some(intrinsic) = collection_receiver_method_intrinsic(receiver_type, method, arg_count)
    {
        return Some(intrinsic);
    }

    match (receiver_type, method, arg_count) {
        ("Int", "to_string", 0) => Some(CorePrimitiveIntrinsic::IntToString),
        ("Float", "to_string", 0) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("String", "equal", 1) => Some(CorePrimitiveIntrinsic::StringEqual),
        ("String", "compare", 1) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("String", "to_string", 0) => Some(CorePrimitiveIntrinsic::StringToString),
        ("String", "from_string", 0) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("String", "is_empty", 0) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("String", "append", 1) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("String", "contains", 1) => Some(CorePrimitiveIntrinsic::StringContains),
        ("String", "starts_with", 1) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("String", "ends_with", 1) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("String", "replace", 2) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("String", "split", 1) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("String", "split_once", 1) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        ("String", "length", 0) => Some(CorePrimitiveIntrinsic::StringLength),
        ("String", "byte_size", 0) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("String", "lowercase", 0) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("String", "uppercase", 0) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("String", "trim", 0) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("String", "trim_start", 0) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("String", "trim_end", 0) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        _ => None,
    }
}

/// Resolves collection receiver methods to compiler-owned intrinsics.
///
/// Inputs:
/// - `receiver_type`: normalized source type inferred for the receiver.
/// - `method`: source receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Core primitive intrinsic id for supported collection receiver calls.
/// - `None` for unsupported collection types, methods, or arities.
///
/// Transformation:
/// - Extracts the nominal type head from generic or qualified collection type
///   text and maps portable receiver methods to backend-neutral collection
///   intrinsic IDs.
fn collection_receiver_method_intrinsic(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (
        receiver_type_head(receiver_type).as_str(),
        method,
        arg_count,
    ) {
        ("List", "is_empty", 0) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("List", "length", 0) => Some(CorePrimitiveIntrinsic::ListLength),
        ("List", "first", 0) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("List", "iterator", 0) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("List", "push", 1) => Some(CorePrimitiveIntrinsic::ListPush),
        ("List", "clear", 0) => Some(CorePrimitiveIntrinsic::ListClear),
        ("Map", "is_empty", 0) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("Map", "size", 0) => Some(CorePrimitiveIntrinsic::MapSize),
        ("Map", "get", 1) => Some(CorePrimitiveIntrinsic::MapGet),
        ("Map", "contains_key", 1) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("Map", "iterator", 0) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("Map", "put", 2) => Some(CorePrimitiveIntrinsic::MapPut),
        ("Map", "remove", 1) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("Map", "clear", 0) => Some(CorePrimitiveIntrinsic::MapClear),
        ("Object", "is_empty", 0) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("Object", "size", 0) => Some(CorePrimitiveIntrinsic::MapSize),
        ("Object", "get", 1) => Some(CorePrimitiveIntrinsic::MapGet),
        ("Object", "contains_key", 1) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("Object", "put", 2) => Some(CorePrimitiveIntrinsic::MapPut),
        ("Object", "remove", 1) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("Object", "clear", 0) => Some(CorePrimitiveIntrinsic::MapClear),
        ("Set", "is_empty", 0) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("Set", "size", 0) => Some(CorePrimitiveIntrinsic::SetSize),
        ("Set", "contains", 1) => Some(CorePrimitiveIntrinsic::SetContains),
        ("Set", "iterator", 0) => Some(CorePrimitiveIntrinsic::SetIterator),
        ("Set", "add", 1) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("Set", "remove", 1) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("Set", "clear", 0) => Some(CorePrimitiveIntrinsic::SetClear),
        ("Agent", "get", 0) => Some(CorePrimitiveIntrinsic::BeamAgentGet),
        ("Agent", "get_and_update", 1) => Some(CorePrimitiveIntrinsic::BeamAgentGetAndUpdate),
        ("Agent", "update", 1) => Some(CorePrimitiveIntrinsic::BeamAgentUpdate),
        ("Agent", "cast", 1) => Some(CorePrimitiveIntrinsic::BeamAgentCast),
        ("Agent", "stop", 0) => Some(CorePrimitiveIntrinsic::BeamAgentStop),
        ("ServerRef", "call", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerCall),
        ("ServerRef", "cast", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerCast),
        ("ServerRef", "stop", 0) => Some(CorePrimitiveIntrinsic::BeamGenServerStop),
        ("NativeBridge", "call", 1) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeCall),
        ("NativeBridge", "dispose", 0) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeDispose),
        ("NativeBridge", "stop", 0) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeStop),
        ("Bytes", "to_list", 0) => Some(CorePrimitiveIntrinsic::BeamBytesToList),
        ("Bytes", "length", 0) => Some(CorePrimitiveIntrinsic::BeamBytesLength),
        ("Bytes", "concat", 1) => Some(CorePrimitiveIntrinsic::BeamBytesConcat),
        ("TcpSocket", "send", 1) => Some(CorePrimitiveIntrinsic::BeamTcpSend),
        ("TcpSocket", "receive", 2) => Some(CorePrimitiveIntrinsic::BeamTcpReceive),
        ("TcpSocket", "close", 0) => Some(CorePrimitiveIntrinsic::BeamTcpClose),
        ("Port", "write", 1) => Some(CorePrimitiveIntrinsic::BeamPortWrite),
        ("Port", "read", 2) => Some(CorePrimitiveIntrinsic::BeamPortRead),
        ("Port", "close", 0) => Some(CorePrimitiveIntrinsic::BeamPortClose),
        ("Supervisor", "start", 1) => Some(CorePrimitiveIntrinsic::BeamSupervisorStart),
        ("Supervisor", "stop", 1) => Some(CorePrimitiveIntrinsic::BeamSupervisorStop),
        _ => None,
    }
}
