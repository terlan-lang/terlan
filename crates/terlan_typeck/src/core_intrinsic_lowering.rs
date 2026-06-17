use super::core_expr_lowering::core_expr_from_syntax;
use super::*;

/// Converts a syntax-output call into a compiler-owned intrinsic call when selected.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` for currently selected intrinsic-backed
///   `std.core` operations with matching call shape and arity.
/// - `None` for non-intrinsic calls, unsupported operations, malformed call
///   shapes, or unsupported argument expressions.
///
/// Transformation:
/// - Accepts both module-shaped primitive calls such as
///   `std.core.String.contains(value, pattern)` and receiver-shaped primitive
///   calls such as `value.contains(pattern)`, then replaces either spelling
///   with the same backend-neutral intrinsic identity.
pub(crate) fn core_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    core_remote_intrinsic_call_expr_from_syntax(expr)
        .or_else(|| core_receiver_intrinsic_call_expr_from_syntax(expr))
        .or_else(|| core_local_intrinsic_call_expr_from_syntax(expr))
}

/// Converts a local syntax-output call into a compiler-owned intrinsic call.
///
/// Inputs:
/// - `expr`: syntax-output call expression without a remote module path.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` for selected compiler-backed local functions
///   such as `type_of(value)` and `is_type(value, Type)`.
/// - `None` for remote calls, non-name callees, unsupported local functions,
///   arity mismatch, or unsupported argument expressions.
///
/// Transformation:
/// - Replaces implicit prelude calls with stable CoreIR intrinsic identities so
///   target-neutral compiler features do not look like unresolved user
///   functions downstream.
fn core_local_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    let function = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.as_deref()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;
    core_intrinsic_expr_from_parts("std.core.Type", function, args, expr.span.into())
}

/// Converts a remote syntax-output call into a compiler-owned intrinsic call.
///
/// Inputs:
/// - `expr`: syntax-output call expression with a remote module path.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` for selected `std.core` primitive operations.
/// - `None` for local calls, malformed callees, unsupported operations,
///   mismatched arity, or unsupported argument expressions.
///
/// Transformation:
/// - Replaces a source-level `std.core.*` API function call with a stable
///   CoreIR intrinsic id while preserving argument order, return type, effects,
///   and source span.
fn core_remote_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) {
        return None;
    }

    let module = expr.remote.as_deref()?;
    let (callee, args) = expr.children.split_first()?;
    let function = match core_expr_from_syntax(callee)? {
        CoreExpr::Atom(function) | CoreExpr::Var(function) => function,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;
    core_intrinsic_expr_from_parts(module, function.as_str(), args, expr.span.into())
}

/// Converts a mutable receiver-method call into effectful CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output call expression that may have a field-access callee.
/// - `receiver_methods`: declared local receiver-method dispatch signatures.
///
/// Output:
/// - `Some(CoreExpr::MutableReceiverCall)` when the call is shaped as
///   `receiver.method(args...)` and all declared candidates for the method/arity
///   are mutable receiver methods.
/// - `None` for non-receiver calls, unknown methods, mixed mutable/immutable
///   overload sets, or children outside the current typed Core subset.
///
/// Transformation:
/// - Preserves the receiver expression separately from non-receiver arguments
///   and attaches the stable receiver-mutation effect label so later lowering
///   can choose target-specific rebinding or in-place mutation semantics.
pub(crate) fn core_mutable_receiver_call_expr_from_syntax(
    expr: &SyntaxExprOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }

    let method = callee.text.as_deref()?;
    if !receiver_method_set_is_exclusively_mutable(receiver_methods, method, args.len()) {
        return None;
    }

    let receiver = callee.children.first()?;
    Some(CoreExpr::MutableReceiverCall {
        receiver: Box::new(core_expr_from_syntax(receiver)?),
        method: method.to_string(),
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
        effects: core_receiver_mutation_effect_set(),
    })
}

/// Checks whether a receiver-method dispatch bucket is unambiguously mutable.
///
/// Inputs:
/// - `receiver_methods`: local receiver-method dispatch signatures.
/// - `method`: source-level method name from a field-access callee.
/// - `arity`: non-receiver argument count.
///
/// Output:
/// - `true` only when at least one candidate exists and every candidate in the
///   method/arity bucket is declared with a mutable receiver.
/// - `false` for missing buckets or mixed mutable/immutable overload sets.
///
/// Transformation:
/// - Treats CoreIR mutation as a semantic commitment. Ambiguous overload sets
///   remain ordinary summary-only calls until type-directed Core lowering can
///   select one exact receiver type.
fn receiver_method_set_is_exclusively_mutable(
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    method: &str,
    arity: usize,
) -> bool {
    receiver_methods
        .get(&(method.to_string(), arity))
        .is_some_and(|candidates| {
            !candidates.is_empty()
                && candidates
                    .iter()
                    .all(|candidate| candidate.receiver_mutable)
        })
}

/// Converts a receiver-method syntax-output call into a primitive intrinsic.
///
/// Inputs:
/// - `expr`: local syntax-output call whose callee may be a field-access method
///   head such as `value.contains`.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` when the receiver method maps to the selected
///   primitive receiver surface.
/// - `None` when the call is remote, not a receiver method, has unsupported
///   receiver/argument expressions, or does not match an intrinsic operation.
///
/// Transformation:
/// - Lowers `receiver.method(args...)` into the same intrinsic as the
///   primitive owner module call, such as `std.core.Int.to_string(receiver)` or
///   `std.core.String.trim(receiver)`, prepending the receiver to the CoreIR
///   argument list so targets do not need to understand source method syntax.
fn core_receiver_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }

    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let module = core_receiver_intrinsic_module(receiver, method, args.len())?;
    let args = std::iter::once(receiver)
        .chain(args.iter())
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    core_intrinsic_expr_from_parts(module, method, args, expr.span.into())
}

/// Resolves a primitive receiver method to its CoreIR intrinsic owner module.
///
/// Inputs:
/// - `receiver`: syntax-output receiver expression from `receiver.method(...)`.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Canonical std primitive module path when the receiver/method pair maps to
///   a compiler-owned intrinsic.
///
/// Transformation:
/// - Uses the receiver expression kind as the formal CoreIR lowering boundary
///   for literal primitives so receiver syntax lowers to the same intrinsic
///   identity as explicit module calls.
fn core_receiver_intrinsic_module(
    receiver: &SyntaxExprOutput,
    method: &str,
    arg_count: usize,
) -> Option<&'static str> {
    match (receiver.kind, method, arg_count) {
        (SyntaxExprKind::Int, "to_string", 0) => Some("std.core.Int"),
        (SyntaxExprKind::Float, "to_string", 0) => Some("std.core.Float"),
        (SyntaxExprKind::Binary, _, _) => Some("std.core.String"),
        _ => None,
    }
}

/// Builds a typed CoreIR intrinsic expression from resolved source call parts.
///
/// Inputs:
/// - `module`: canonical primitive owner path, such as `std.core.String`.
/// - `function`: primitive operation name.
/// - `args`: already-lowered CoreIR arguments in intrinsic order.
/// - `span`: source span for diagnostics and contract text.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` when the module/function/arity maps to a
///   selected primitive intrinsic.
/// - `None` when the operation is not intrinsic-backed.
///
/// Transformation:
/// - Performs the final intrinsic registry lookup and packages the closed
///   intrinsic id, arguments, return type, pure effect set, and source span into
///   a backend-neutral CoreIR node.
fn core_intrinsic_expr_from_parts(
    module: &str,
    function: &str,
    args: Vec<CoreExpr>,
    span: Span,
) -> Option<CoreExpr> {
    if let Some(intrinsic) = core_primitive_intrinsic(module, function, args.len()) {
        let return_type = core_primitive_intrinsic_return_type(&intrinsic);

        return Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(intrinsic),
            args,
            return_type,
            effects: core_pure_effect_set(),
            span,
        }));
    }

    let capability = core_runtime_capability(module, function, args.len())?;
    let return_type = core_runtime_capability_return_type(&capability);
    Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(capability),
        args,
        return_type,
        effects: core_io_effect_set(),
        span,
    }))
}

/// Resolves a `std.core` primitive operation name and arity to an intrinsic.
///
/// Inputs:
/// - `module`: source-level remote module path.
/// - `function`: source-level operation name after the module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` when the operation is currently selected
///   for primitive intrinsic lowering.
/// - `None` for portable-backed operations, unknown modules, unknown names, or
///   arity mismatch.
///
/// Transformation:
/// - Dispatches stable std.core primitive API calls to closed compiler-owned
///   intrinsic identities without carrying backend module/function names into
///   CoreIR.
fn core_primitive_intrinsic(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match module {
        "std.core.Bool" => core_bool_primitive_intrinsic(function, arity),
        "std.core.Atom" => core_atom_primitive_intrinsic(function, arity),
        "std.core.Type" => core_type_primitive_intrinsic(function, arity),
        "std.core.Int" => core_int_primitive_intrinsic(function, arity),
        "std.core.Float" => core_float_primitive_intrinsic(function, arity),
        "std.core.String" => core_string_primitive_intrinsic(function, arity),
        "std.collections.List" => core_list_primitive_intrinsic(function, arity),
        "std.collections.Iterator" => core_iterator_primitive_intrinsic(function, arity),
        "std.collections.Map" => core_map_primitive_intrinsic(function, arity),
        "std.collections.Set" => core_set_primitive_intrinsic(function, arity),
        "std.core.Task" => core_task_primitive_intrinsic(function, arity),
        "std.beam.Agent" => core_beam_agent_primitive_intrinsic(function, arity),
        "std.beam.GenServer" => core_beam_gen_server_primitive_intrinsic(function, arity),
        "std.beam.NativeBridge" => core_beam_native_bridge_primitive_intrinsic(function, arity),
        "std.beam.Supervisor" => core_beam_supervisor_primitive_intrinsic(function, arity),
        "std.beam.Task" => core_beam_task_primitive_intrinsic(function, arity),
        _ => None,
    }
}

/// Resolves a runtime stdlib operation name and arity to a CoreIR capability.
///
/// Inputs:
/// - `module`: source-level remote module path.
/// - `function`: source-level operation name after the module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CoreRuntimeCapability)` when the operation is a selected
///   target-neutral runtime capability.
/// - `None` for primitive operations, ordinary calls, unknown modules, unknown
///   names, or arity mismatch.
///
/// Transformation:
/// - Maps source APIs such as `std.io.Console.println(value)` to backend-neutral
///   CoreIR runtime capability identities without carrying target module names
///   into CoreIR.
fn core_runtime_capability(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CoreRuntimeCapability> {
    match (module, function, arity) {
        ("std.io.Console", "println", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
        ("std.io.File", "exists", 1) => Some(CoreRuntimeCapability::FileExists),
        ("std.io.File", "read_text", 1) => Some(CoreRuntimeCapability::FileReadText),
        ("std.io.File", "write_text", 2) => Some(CoreRuntimeCapability::FileWriteText),
        ("std.io.File", "append_text", 2) => Some(CoreRuntimeCapability::FileAppendText),
        ("std.io.File", "delete", 1) => Some(CoreRuntimeCapability::FileDelete),
        _ => None,
    }
}

/// Resolves a `std.core.Type` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the compiler-owned type
///   intrinsic namespace.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected type-introspection hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps implicit source calls such as `type_of(value)` and
///   `is_type(value, Int)` to stable CoreIR intrinsic identities.
fn core_type_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("type_of", 1) => Some(CorePrimitiveIntrinsic::TypeOf),
        ("is_type", 2) => Some(CorePrimitiveIntrinsic::IsType),
        _ => None,
    }
}

/// Resolves a `std.core.Bool` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Bool`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Bool release hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps the 0.0.1 Bool API hooks to stable CoreIR intrinsic identities so
///   external projects do not depend on backend-generated internal module artifacts.
fn core_bool_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("equal", 2) => Some(CorePrimitiveIntrinsic::BoolEqual),
        ("compare", 2) => Some(CorePrimitiveIntrinsic::BoolCompare),
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::BoolToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::BoolFromString),
        _ => None,
    }
}

/// Resolves a `std.core.Atom` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Atom`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Atom release hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps the language-neutral singleton atom display API to a stable CoreIR
///   intrinsic identity so source code does not depend on backend atom syntax.
fn core_atom_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::AtomToString),
        _ => None,
    }
}

/// Resolves a `std.core.Int` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Int`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Int conversion hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps source API conversion hooks to stable CoreIR intrinsic identities.
fn core_int_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::IntToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::IntFromString),
        _ => None,
    }
}

/// Resolves a `std.core.Float` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Float`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Float conversion hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps source API conversion hooks to stable CoreIR intrinsic identities.
fn core_float_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::FloatFromString),
        _ => None,
    }
}

/// Resolves a `std.core.String` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.core.String`
///   module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` when the operation is currently selected
///   for string intrinsic lowering.
/// - `None` for portable-backed operations, unknown names, or arity mismatch.
///
/// Transformation:
/// - Maps source API names to closed compiler-owned intrinsic identities
///   without carrying backend module/function names into CoreIR.
fn core_string_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("equal", 2) => Some(CorePrimitiveIntrinsic::StringEqual),
        ("compare", 2) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::StringToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("append", 2) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("concat", 1) => Some(CorePrimitiveIntrinsic::StringConcat),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::StringContains),
        ("starts_with", 2) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("ends_with", 2) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("length", 1) => Some(CorePrimitiveIntrinsic::StringLength),
        ("byte_size", 1) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("lowercase", 1) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("uppercase", 1) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("trim", 1) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("trim_start", 1) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("trim_end", 1) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        ("replace", 3) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("split", 2) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("split_once", 2) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        _ => None,
    }
}

/// Resolves a `std.collections.List` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.collections.List` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for the selected 0.0.2 list intrinsic
///   surface.
/// - `None` for unknown names or arity mismatches.
///
/// Transformation:
/// - Maps portable `std.collections.List` API names to closed compiler-owned
///   intrinsic identities so CoreIR and target backends do not expose list
///   details.
fn core_list_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("new", 0) => Some(CorePrimitiveIntrinsic::ListNew),
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("length", 1) => Some(CorePrimitiveIntrinsic::ListLength),
        ("first", 1) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("push", 2) => Some(CorePrimitiveIntrinsic::ListPush),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::ListClear),
        _ => None,
    }
}

/// Resolves a `std.collections.Iterator` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.collections.Iterator`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for the selected traversal intrinsic.
/// - `None` for unknown names or arity mismatches.
///
/// Transformation:
/// - Maps portable iterator APIs to compiler-owned intrinsic identities so
///   CoreIR and target backends own traversal state representation.
fn core_iterator_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("next", 1) => Some(CorePrimitiveIntrinsic::IteratorNext),
        _ => None,
    }
}

/// Resolves a `std.collections.Map` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.collections.Map` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for the selected 0.0.2 map intrinsic
///   surface.
/// - `None` for unknown names or arity mismatches.
///
/// Transformation:
/// - Maps portable `std.collections.Map` API names to closed compiler-owned intrinsic
///   identities so CoreIR and target backends do not expose backend-specific map
///   details.
fn core_map_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("new", 0) => Some(CorePrimitiveIntrinsic::MapNew),
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        _ => None,
    }
}

/// Resolves a `std.collections.Set` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.collections.Set` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for the selected 0.0.2 set intrinsic
///   surface.
/// - `None` for unknown names or arity mismatches.
///
/// Transformation:
/// - Maps portable `std.collections.Set` API names to closed compiler-owned intrinsic
///   identities so CoreIR and target backends do not expose representation
///   details.
fn core_set_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("new", 0) => Some(CorePrimitiveIntrinsic::SetNew),
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("size", 1) => Some(CorePrimitiveIntrinsic::SetSize),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::SetContains),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::SetIterator),
        ("add", 2) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("remove", 2) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::SetClear),
        _ => None,
    }
}

/// Resolves a `std.core.Task` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.core.Task` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for the first admitted executable Task
///   operations.
/// - `None` for deferred async operations that still require a runtime
///   scheduling contract.
///
/// Transformation:
/// - Maps the portable completed-task surface to compiler-owned CoreIR
///   intrinsic identities so target profiles can admit only the backend-owned
///   Task subset.
fn core_task_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("done", 1) => Some(CorePrimitiveIntrinsic::TaskDone),
        ("result", 1) => Some(CorePrimitiveIntrinsic::TaskResult),
        _ => None,
    }
}

/// Resolves a `std.beam.Agent` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Agent` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable BEAM Agent
///   operations.
/// - `None` for operations that have not yet received backend lowering.
///
/// Transformation:
/// - Maps the BEAM-owned state-process surface to closed CoreIR intrinsic
///   identities so target profiles can admit only operations with concrete
///   Erlang backend lowering.
fn core_beam_agent_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("start", 1) => Some(CorePrimitiveIntrinsic::BeamAgentStart),
        ("get", 1) => Some(CorePrimitiveIntrinsic::BeamAgentGet),
        ("get_and_update", 2) => Some(CorePrimitiveIntrinsic::BeamAgentGetAndUpdate),
        ("update", 2) => Some(CorePrimitiveIntrinsic::BeamAgentUpdate),
        ("cast", 2) => Some(CorePrimitiveIntrinsic::BeamAgentCast),
        ("stop", 1) => Some(CorePrimitiveIntrinsic::BeamAgentStop),
        _ => None,
    }
}

/// Resolves a `std.beam.GenServer` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.GenServer`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable BEAM GenServer
///   operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the BEAM-owned callback process surface to closed CoreIR intrinsic
///   identities so target profiles and backends can handle GenServer calls
///   without stringly typed module dispatch.
fn core_beam_gen_server_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("start", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerStart),
        ("call", 2) => Some(CorePrimitiveIntrinsic::BeamGenServerCall),
        ("cast", 2) => Some(CorePrimitiveIntrinsic::BeamGenServerCast),
        ("stop", 1) => Some(CorePrimitiveIntrinsic::BeamGenServerStop),
        _ => None,
    }
}

/// Resolves a `std.beam.NativeBridge` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.NativeBridge`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable BEAM NativeBridge
///   proof operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the SafeNative bridge handle surface to closed CoreIR intrinsic
///   identities so the Erlang backend can validate bridge plumbing before real
///   native worker transport is attached.
fn core_beam_native_bridge_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("start", 1) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeStart),
        ("call", 2) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeCall),
        ("dispose", 1) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeDispose),
        ("stop", 1) => Some(CorePrimitiveIntrinsic::BeamNativeBridgeStop),
        _ => None,
    }
}

/// Resolves a `std.beam.Supervisor` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Supervisor`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable BEAM Supervisor
///   operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the supervision contract surface to closed CoreIR intrinsic
///   identities so target profiles and backends can handle the local
///   supervision proof without stringly typed module dispatch.
fn core_beam_supervisor_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("child_spec", 1) => Some(CorePrimitiveIntrinsic::BeamSupervisorChildSpec),
        ("start", 2) => Some(CorePrimitiveIntrinsic::BeamSupervisorStart),
        ("stop", 2) => Some(CorePrimitiveIntrinsic::BeamSupervisorStop),
        _ => None,
    }
}

/// Resolves a `std.beam.Task` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Task` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable BEAM Task
///   operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the BEAM-owned task-process surface to closed CoreIR intrinsic
///   identities so target profiles and backends can handle BEAM Task
///   operations without stringly typed module calls.
fn core_beam_task_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("start", 1) => Some(CorePrimitiveIntrinsic::BeamTaskStart),
        ("result", 1) => Some(CorePrimitiveIntrinsic::BeamTaskResult),
        ("cancel", 1) => Some(CorePrimitiveIntrinsic::BeamTaskCancel),
        _ => None,
    }
}

/// Returns the Core return type for a primitive intrinsic.
///
/// Inputs:
/// - `intrinsic`: compiler-owned primitive intrinsic identity.
///
/// Output:
/// - Backend-neutral `CoreType` result expected from the intrinsic call.
///
/// Transformation:
/// - Encodes the intrinsic registry's output column as CoreIR type payloads so
///   target lowering can validate operation results without re-reading source
///   signatures.
fn core_primitive_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::TypeOf => CoreType::Named("Type".to_string()),
        CorePrimitiveIntrinsic::IsType => CoreType::Bool,
        CorePrimitiveIntrinsic::BoolToString
        | CorePrimitiveIntrinsic::AtomToString
        | CorePrimitiveIntrinsic::IntToString
        | CorePrimitiveIntrinsic::FloatToString
        | CorePrimitiveIntrinsic::StringToString
        | CorePrimitiveIntrinsic::StringAppend
        | CorePrimitiveIntrinsic::StringConcat
        | CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringUppercase
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringReplace => CoreType::String,
        CorePrimitiveIntrinsic::BoolEqual => CoreType::Bool,
        CorePrimitiveIntrinsic::BoolCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::StringCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::BoolFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Bool],
        },
        CorePrimitiveIntrinsic::StringFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::String],
        },
        CorePrimitiveIntrinsic::IntFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Int],
        },
        CorePrimitiveIntrinsic::FloatFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Float],
        },
        CorePrimitiveIntrinsic::StringEqual
        | CorePrimitiveIntrinsic::StringIsEmpty
        | CorePrimitiveIntrinsic::StringContains
        | CorePrimitiveIntrinsic::StringStartsWith
        | CorePrimitiveIntrinsic::StringEndsWith => CoreType::Bool,
        CorePrimitiveIntrinsic::StringLength | CorePrimitiveIntrinsic::StringByteSize => {
            CoreType::Int
        }
        CorePrimitiveIntrinsic::StringSplit => CoreType::List(Box::new(CoreType::String)),
        CorePrimitiveIntrinsic::StringSplitOnce => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::String),
                CoreTupleTypeElem::Type(CoreType::String),
            ])],
        },
        CorePrimitiveIntrinsic::ListNew
        | CorePrimitiveIntrinsic::ListIterator
        | CorePrimitiveIntrinsic::ListPush
        | CorePrimitiveIntrinsic::ListClear => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::ListIsEmpty => CoreType::Bool,
        CorePrimitiveIntrinsic::ListLength => CoreType::Int,
        CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::IteratorNext => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::MapNew
        | CorePrimitiveIntrinsic::MapPut
        | CorePrimitiveIntrinsic::MapRemove
        | CorePrimitiveIntrinsic::MapClear => CoreType::Named("Map".to_string()),
        CorePrimitiveIntrinsic::MapIsEmpty | CorePrimitiveIntrinsic::MapContainsKey => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::MapSize => CoreType::Int,
        CorePrimitiveIntrinsic::MapGet => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::MapIterator => CoreType::List(Box::new(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Named("Dynamic".to_string())),
            CoreTupleTypeElem::Type(CoreType::Named("Dynamic".to_string())),
        ]))),
        CorePrimitiveIntrinsic::SetNew
        | CorePrimitiveIntrinsic::SetAdd
        | CorePrimitiveIntrinsic::SetRemove
        | CorePrimitiveIntrinsic::SetClear => CoreType::Named("Set".to_string()),
        CorePrimitiveIntrinsic::SetIsEmpty | CorePrimitiveIntrinsic::SetContains => CoreType::Bool,
        CorePrimitiveIntrinsic::SetSize => CoreType::Int,
        CorePrimitiveIntrinsic::SetIterator => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::TaskDone => CoreType::Apply {
            constructor: "Task".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::TaskResult => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamAgentStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Apply {
                    constructor: "Agent".to_string(),
                    args: vec![CoreType::Named("Dynamic".to_string())],
                },
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamAgentGet | CorePrimitiveIntrinsic::BeamAgentGetAndUpdate => {
            CoreType::Named("Dynamic".to_string())
        }
        CorePrimitiveIntrinsic::BeamAgentUpdate
        | CorePrimitiveIntrinsic::BeamAgentCast
        | CorePrimitiveIntrinsic::BeamAgentStop => CoreType::Apply {
            constructor: "Agent".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::BeamGenServerStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Apply {
                    constructor: "ServerRef".to_string(),
                    args: vec![
                        CoreType::Named("Dynamic".to_string()),
                        CoreType::Named("Dynamic".to_string()),
                        CoreType::Named("Dynamic".to_string()),
                        CoreType::Named("Dynamic".to_string()),
                    ],
                },
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamGenServerCall => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamGenServerCast | CorePrimitiveIntrinsic::BeamGenServerStop => {
            CoreType::Apply {
                constructor: "ServerRef".to_string(),
                args: vec![
                    CoreType::Named("Dynamic".to_string()),
                    CoreType::Named("Dynamic".to_string()),
                    CoreType::Named("Dynamic".to_string()),
                    CoreType::Named("Dynamic".to_string()),
                ],
            }
        }
        CorePrimitiveIntrinsic::BeamNativeBridgeStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Apply {
                    constructor: "NativeBridge".to_string(),
                    args: vec![CoreType::Named("Dynamic".to_string())],
                },
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamNativeBridgeCall => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamNativeBridgeDispose
        | CorePrimitiveIntrinsic::BeamNativeBridgeStop => CoreType::Apply {
            constructor: "NativeBridge".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::BeamSupervisorChildSpec => CoreType::Apply {
            constructor: "ChildSpec".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::BeamSupervisorStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamSupervisorStop => CoreType::Named("Supervisor".to_string()),
        CorePrimitiveIntrinsic::BeamTaskStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Apply {
                    constructor: "Task".to_string(),
                    args: vec![CoreType::Named("Dynamic".to_string())],
                },
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamTaskResult => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::BeamTaskCancel => CoreType::Apply {
            constructor: "Task".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
    }
}

/// Returns the Core return type for a runtime capability.
///
/// Inputs:
/// - `capability`: compiler-owned runtime capability identity.
///
/// Output:
/// - Backend-neutral `CoreType` result expected from the capability call.
///
/// Transformation:
/// - Encodes the runtime capability registry's output column as CoreIR type
///   payloads so target lowering can validate effectful operation results
///   without re-reading source signatures.
fn core_runtime_capability_return_type(capability: &CoreRuntimeCapability) -> CoreType {
    match capability {
        CoreRuntimeCapability::ConsolePrintln => CoreType::Named("Unit".to_string()),
        CoreRuntimeCapability::FileExists => CoreType::Bool,
        CoreRuntimeCapability::FileReadText => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileWriteText
        | CoreRuntimeCapability::FileAppendText
        | CoreRuntimeCapability::FileDelete => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
    }
}

/// Builds the canonical pure Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `pure` label.
///
/// Transformation:
/// - Centralizes the effect payload used by primitive intrinsics that do not
///   perform observable side effects.
pub(crate) fn core_pure_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["pure".to_string()],
    }
}

/// Builds the canonical IO Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `io` label.
///
/// Transformation:
/// - Centralizes the effect payload used by runtime capabilities that perform
///   observable console or stream effects.
pub(crate) fn core_io_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["io".to_string()],
    }
}

/// Builds the canonical mutable receiver Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `receiver_mutation` label.
///
/// Transformation:
/// - Centralizes the effect payload used by receiver methods whose source
///   receiver is declared mutable, keeping mutation separate from ordinary
///   `Unit`-returning calls in CoreIR.
pub(crate) fn core_receiver_mutation_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["receiver_mutation".to_string()],
    }
}
