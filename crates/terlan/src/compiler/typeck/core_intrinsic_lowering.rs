use super::core_expr_lowering::core_expr_from_syntax;
use super::*;

mod effects;
mod process_intrinsics;
mod registry;
mod return_types;

#[cfg(test)]
mod effect_test;

pub(crate) use effects::{
    core_io_effect_set, core_pure_effect_set, core_receiver_mutation_effect_set,
    core_vm_effect_execution_set,
};
use process_intrinsics::{
    core_typed_message_expr_from_parts, core_typed_process_intrinsic_expr_from_parts,
};
pub(crate) use registry::core_primitive_intrinsic;
use registry::core_runtime_capability;
use return_types::core_runtime_capability_return_type;

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

/// Reports whether a compiler-owned intrinsic identity is known and pure.
///
/// Runtime capabilities are effectful even when their std declarations carry
/// placeholder bodies. Unknown intrinsic declarations remain conservatively
/// unproven instead of being inferred pure from those placeholders.
pub(crate) fn core_intrinsic_is_pure(module: &str, function: &str, arity: usize) -> bool {
    if module == "std.vm.Message" && matches!((function, arity), ("wrap", 1) | ("unwrap", 1)) {
        return true;
    }
    if module == "std.vm.Process" {
        if matches!(
            (function, arity),
            ("entry", 1)
                | ("timer", 1)
                | ("resource_kind", 1)
                | ("exit_reason", 1)
                | ("priority", 0)
                | ("normal", 0)
                | ("background", 0)
        ) {
            return true;
        }
        if matches!(
            (function, arity),
            ("current", 0)
                | ("send", 2)
                | ("receive", 0)
                | ("spawn", 1)
                | ("sleep", 1)
                | ("link", 1)
                | ("monitor", 1)
                | ("acquire", 1)
                | ("cancel", 1)
                | ("fail", 1)
                | ("schedule", 1)
        ) {
            return false;
        }
    }
    if let Some(intrinsic) = core_primitive_intrinsic(module, function, arity) {
        return !matches!(
            intrinsic,
            CorePrimitiveIntrinsic::VmEffectRun
                | CorePrimitiveIntrinsic::VmProcessYield
                | CorePrimitiveIntrinsic::VmProcessSendInt
                | CorePrimitiveIntrinsic::VmProcessSendString
                | CorePrimitiveIntrinsic::VmProcessReceiveString
                | CorePrimitiveIntrinsic::VmProcessSendBytes
                | CorePrimitiveIntrinsic::VmProcessReceiveBytes
                | CorePrimitiveIntrinsic::VmProcessSendBinary
                | CorePrimitiveIntrinsic::VmProcessReceiveBinary
                | CorePrimitiveIntrinsic::VmProcessSendAtom
                | CorePrimitiveIntrinsic::VmProcessReceiveAtom
                | CorePrimitiveIntrinsic::VmProcessSleep
                | CorePrimitiveIntrinsic::VmProcessFail
                | CorePrimitiveIntrinsic::VmProcessSchedule
        );
    }
    core_runtime_capability(module, function, arity).is_none() && !module.starts_with("std.")
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
    if function == "String" && args.len() == 1 {
        return Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ValueToString),
            args,
            return_type: CoreType::String,
            effects: core_pure_effect_set(),
            span: expr.span.into(),
        }));
    }
    if args.len() == 1 {
        let intrinsic = match function {
            "Bool" => Some(CorePrimitiveIntrinsic::BoolFromString),
            "Int" => Some(CorePrimitiveIntrinsic::IntFromString),
            "Float" => Some(CorePrimitiveIntrinsic::FloatFromString),
            _ => None,
        };
        if let Some(intrinsic) = intrinsic {
            return Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
                return_type: core_primitive_intrinsic_return_type(&intrinsic),
                id: CoreIntrinsicId::Primitive(intrinsic),
                args,
                effects: core_pure_effect_set(),
                span: expr.span.into(),
            }));
        }
    }
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
    core_typed_process_intrinsic_expr_from_parts(
        module,
        function.as_str(),
        &expr.type_args,
        args.clone(),
        expr.span.into(),
    )
    .or_else(|| {
        core_typed_memory_intrinsic_expr_from_parts(
            module,
            function.as_str(),
            &expr.type_args,
            args.clone(),
            expr.span.into(),
        )
    })
    .or_else(|| {
        core_typed_message_expr_from_parts(module, function.as_str(), &expr.type_args, args.clone())
    })
    .or_else(|| {
        core_typed_collection_intrinsic_expr_from_parts(
            module,
            function.as_str(),
            &expr.type_args,
            args.clone(),
            expr.span.into(),
        )
    })
    .or_else(|| core_intrinsic_expr_from_parts(module, function.as_str(), args, expr.span.into()))
}

/// Lowers explicitly typed collection constructors without erasing their
/// concrete element type from CoreIR.
fn core_typed_collection_intrinsic_expr_from_parts(
    module: &str,
    function: &str,
    type_args: &[crate::terlan_syntax::SyntaxTypeOutput],
    args: Vec<CoreExpr>,
    span: Span,
) -> Option<CoreExpr> {
    if module != "std.collections.List"
        || function != "new"
        || !args.is_empty()
        || type_args.len() != 1
    {
        return None;
    }
    let element = core_type_from_text(&type_args[0].text)?;
    Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew),
        args,
        return_type: CoreType::List(Box::new(element)),
        effects: core_pure_effect_set(),
        span,
    }))
}

/// Lowers explicit memory introspection while retaining its concrete value type.
fn core_typed_memory_intrinsic_expr_from_parts(
    module: &str,
    function: &str,
    type_args: &[crate::terlan_syntax::SyntaxTypeOutput],
    args: Vec<CoreExpr>,
    span: Span,
) -> Option<CoreExpr> {
    if module != "std.core.Memory" || type_args.len() != 1 {
        return None;
    }
    let value_type = core_type_from_text(&type_args[0].text)?;
    let (id, return_type) = match (function, args.len()) {
        ("layout_of", 0) => (
            CoreIntrinsicId::MemoryLayoutOf(value_type),
            CoreType::Named("std.core.Memory.Layout".to_string()),
        ),
        ("shallow_size", 1) => (
            CoreIntrinsicId::MemoryShallowSize(value_type),
            CoreType::Int,
        ),
        ("retained_size", 1) => (
            CoreIntrinsicId::MemoryRetainedSize(value_type),
            CoreType::Int,
        ),
        _ => return None,
    };
    Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
        id,
        args,
        return_type,
        effects: core_pure_effect_set(),
        span,
    }))
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
pub(super) fn core_mutable_receiver_call_expr_from_syntax(
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
        .chain(ordered_core_receiver_intrinsic_args(
            method,
            args,
            &expr.arg_names,
        )?)
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    core_intrinsic_expr_from_parts(module, method, args, expr.span.into())
}

/// Orders primitive receiver intrinsic arguments for CoreIR lowering.
///
/// Inputs:
/// - `method`: primitive receiver method name.
/// - `args`: non-receiver source arguments in written order.
/// - `arg_names`: optional source names parallel to `args`.
///
/// Output:
/// - Argument references in primitive method parameter order.
/// - `None` when named metadata targets an unsupported primitive shape.
///
/// Transformation:
/// - Leaves positional calls unchanged and reorders named arguments according
///   to the shared primitive receiver method parameter-name table.
fn ordered_core_receiver_intrinsic_args<'a>(
    method: &str,
    args: &'a [SyntaxExprOutput],
    arg_names: &[Option<String>],
) -> Option<Vec<&'a SyntaxExprOutput>> {
    if !arg_names.iter().any(Option::is_some) {
        return Some(args.iter().collect());
    }
    let param_names = primitive_receiver_method_arg_names(method, args.len())?;
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
        let effects = match intrinsic {
            CorePrimitiveIntrinsic::VmEffectRun
            | CorePrimitiveIntrinsic::VmDebuggerBreak
            | CorePrimitiveIntrinsic::VmProcessYield
            | CorePrimitiveIntrinsic::VmProcessSendInt
            | CorePrimitiveIntrinsic::VmProcessReceiveInt
            | CorePrimitiveIntrinsic::VmProcessSendString
            | CorePrimitiveIntrinsic::VmProcessReceiveString
            | CorePrimitiveIntrinsic::VmProcessSendBytes
            | CorePrimitiveIntrinsic::VmProcessReceiveBytes
            | CorePrimitiveIntrinsic::VmProcessSendBinary
            | CorePrimitiveIntrinsic::VmProcessReceiveBinary
            | CorePrimitiveIntrinsic::VmProcessSendAtom
            | CorePrimitiveIntrinsic::VmProcessReceiveAtom
            | CorePrimitiveIntrinsic::VmProcessSleep
            | CorePrimitiveIntrinsic::VmProcessFail
            | CorePrimitiveIntrinsic::VmProcessSchedule => core_vm_effect_execution_set(),
            _ => core_pure_effect_set(),
        };

        return Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(intrinsic),
            args,
            return_type,
            effects,
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
pub fn core_primitive_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::TypeOf => CoreType::Named("Type".to_string()),
        CorePrimitiveIntrinsic::IsType => CoreType::Bool,
        CorePrimitiveIntrinsic::MemoryLayoutSize
        | CorePrimitiveIntrinsic::MemoryLayoutAlignment => CoreType::Int,
        CorePrimitiveIntrinsic::MemoryLayoutStorage => CoreType::Atom,
        CorePrimitiveIntrinsic::BoolToString
        | CorePrimitiveIntrinsic::AtomToString
        | CorePrimitiveIntrinsic::ValueToString
        | CorePrimitiveIntrinsic::IntToString
        | CorePrimitiveIntrinsic::FloatToString
        | CorePrimitiveIntrinsic::StringToString
        | CorePrimitiveIntrinsic::StringAppend
        | CorePrimitiveIntrinsic::StringConcat
        | CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringUppercase
        | CorePrimitiveIntrinsic::StringReverse
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringReplace
        | CorePrimitiveIntrinsic::CryptoSha256 => CoreType::String,
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
        CorePrimitiveIntrinsic::IntToStringBase => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::String],
        },
        CorePrimitiveIntrinsic::StringFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::String],
        },
        CorePrimitiveIntrinsic::IntFromString | CorePrimitiveIntrinsic::IntFromStringBase => {
            CoreType::Apply {
                constructor: "Option".to_string(),
                args: vec![CoreType::Int],
            }
        }
        CorePrimitiveIntrinsic::FloatFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Float],
        },
        CorePrimitiveIntrinsic::FloatFloor
        | CorePrimitiveIntrinsic::FloatCeil
        | CorePrimitiveIntrinsic::FloatLog
        | CorePrimitiveIntrinsic::FloatPi
        | CorePrimitiveIntrinsic::FloatTau => CoreType::Float,
        CorePrimitiveIntrinsic::StringEqual
        | CorePrimitiveIntrinsic::StringIsEmpty
        | CorePrimitiveIntrinsic::StringContains
        | CorePrimitiveIntrinsic::StringStartsWith
        | CorePrimitiveIntrinsic::StringEndsWith => CoreType::Bool,
        CorePrimitiveIntrinsic::StringLength
        | CorePrimitiveIntrinsic::StringByteSize
        | CorePrimitiveIntrinsic::StringUtf8ByteAt
        | CorePrimitiveIntrinsic::StringUtf8FindAnyByte => CoreType::Int,
        CorePrimitiveIntrinsic::StringSplit | CorePrimitiveIntrinsic::StringCharacters => {
            CoreType::List(Box::new(CoreType::String))
        }
        CorePrimitiveIntrinsic::StringCodepoints => CoreType::List(Box::new(CoreType::Int)),
        CorePrimitiveIntrinsic::StringUtf8Slice => CoreType::String,
        CorePrimitiveIntrinsic::StringSplitOnce => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::String),
                CoreTupleTypeElem::Type(CoreType::String),
            ])],
        },
        CorePrimitiveIntrinsic::ListNew
        | CorePrimitiveIntrinsic::ListConcat
        | CorePrimitiveIntrinsic::ListSubtract
        | CorePrimitiveIntrinsic::ListIterator
        | CorePrimitiveIntrinsic::ListPush
        | CorePrimitiveIntrinsic::ListClear => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::ListIsEmpty => CoreType::Bool,
        CorePrimitiveIntrinsic::ListLength => CoreType::Int,
        CorePrimitiveIntrinsic::ListGet => CoreType::Dynamic,
        CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::ListRest => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::List(Box::new(CoreType::Named(
                "Dynamic".to_string(),
            )))],
        },
        CorePrimitiveIntrinsic::IteratorNext => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::MapNew
        | CorePrimitiveIntrinsic::MapFromEntries
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
        CorePrimitiveIntrinsic::MapTake => CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Apply {
                constructor: "Option".to_string(),
                args: vec![CoreType::Named("Dynamic".to_string())],
            }),
            CoreTupleTypeElem::Type(CoreType::Named("Map".to_string())),
        ]),
        CorePrimitiveIntrinsic::MapIterator => CoreType::List(Box::new(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Named("Dynamic".to_string())),
            CoreTupleTypeElem::Type(CoreType::Named("Dynamic".to_string())),
        ]))),
        CorePrimitiveIntrinsic::SetNew
        | CorePrimitiveIntrinsic::SetFromList
        | CorePrimitiveIntrinsic::SetAdd
        | CorePrimitiveIntrinsic::SetRemove
        | CorePrimitiveIntrinsic::SetClear => CoreType::Apply {
            constructor: "Set".to_string(),
            args: vec![CoreType::Dynamic],
        },
        CorePrimitiveIntrinsic::SetIsEmpty | CorePrimitiveIntrinsic::SetContains => CoreType::Bool,
        CorePrimitiveIntrinsic::SetSize => CoreType::Int,
        CorePrimitiveIntrinsic::SetIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![CoreType::Dynamic],
        },
        CorePrimitiveIntrinsic::TaskDone => CoreType::Apply {
            constructor: "Task".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::TaskFailed => CoreType::Apply {
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
        CorePrimitiveIntrinsic::VmEffectRun => CoreType::Dynamic,
        CorePrimitiveIntrinsic::VmDebuggerBreak
        | CorePrimitiveIntrinsic::VmProcessYield
        | CorePrimitiveIntrinsic::VmProcessSendInt
        | CorePrimitiveIntrinsic::VmProcessSendString
        | CorePrimitiveIntrinsic::VmProcessSendBytes
        | CorePrimitiveIntrinsic::VmProcessSendBinary
        | CorePrimitiveIntrinsic::VmProcessSendAtom
        | CorePrimitiveIntrinsic::VmProcessSleep
        | CorePrimitiveIntrinsic::VmProcessFail
        | CorePrimitiveIntrinsic::VmProcessSchedule => CoreType::Named("Unit".to_string()),
        CorePrimitiveIntrinsic::VmProcessReceiveInt => CoreType::Int,
        CorePrimitiveIntrinsic::VmProcessReceiveString => CoreType::String,
        CorePrimitiveIntrinsic::VmProcessReceiveBytes => CoreType::Named("Bytes".to_string()),
        CorePrimitiveIntrinsic::VmProcessReceiveBinary => CoreType::Binary,
        CorePrimitiveIntrinsic::VmProcessReceiveAtom => CoreType::Atom,
        CorePrimitiveIntrinsic::VmNativeBridgeStart => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Apply {
                    constructor: "NativeBridge".to_string(),
                    args: vec![CoreType::Named("Dynamic".to_string())],
                },
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmNativeBridgeCall => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Dynamic".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmNativeBridgeDispose
        | CorePrimitiveIntrinsic::VmNativeBridgeStop => CoreType::Apply {
            constructor: "NativeBridge".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::VmBytesFromList
        | CorePrimitiveIntrinsic::VmBytesConcat
        | CorePrimitiveIntrinsic::VmBytesSlice => CoreType::Named("Bytes".to_string()),
        CorePrimitiveIntrinsic::VmBytesToList => CoreType::List(Box::new(CoreType::Int)),
        CorePrimitiveIntrinsic::VmBytesStartsWith | CorePrimitiveIntrinsic::VmBytesContains => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::VmBytesLength
        | CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace
        | CorePrimitiveIntrinsic::VmBytesReadUintBe
        | CorePrimitiveIntrinsic::VmBytesReadIntBe
        | CorePrimitiveIntrinsic::VmBytesReadUintLe
        | CorePrimitiveIntrinsic::VmBytesReadIntLe => CoreType::Int,
        CorePrimitiveIntrinsic::VmBitStringFromBytes
        | CorePrimitiveIntrinsic::VmBitStringFromAllBytes
        | CorePrimitiveIntrinsic::VmBitStringFromExactBytes
        | CorePrimitiveIntrinsic::VmBitStringRequireExactBits
        | CorePrimitiveIntrinsic::VmBitStringFromUintBe
        | CorePrimitiveIntrinsic::VmBitStringFromIntBe
        | CorePrimitiveIntrinsic::VmBitStringFromUintLe
        | CorePrimitiveIntrinsic::VmBitStringFromIntLe
        | CorePrimitiveIntrinsic::VmBitStringUtf8Scalar
        | CorePrimitiveIntrinsic::VmBitStringUtf16BeScalar
        | CorePrimitiveIntrinsic::VmBitStringUtf16LeScalar
        | CorePrimitiveIntrinsic::VmBitStringUtf32BeScalar
        | CorePrimitiveIntrinsic::VmBitStringUtf32LeScalar
        | CorePrimitiveIntrinsic::VmBitStringSlice
        | CorePrimitiveIntrinsic::VmBitStringConcat => CoreType::Named("BitString".to_string()),
        CorePrimitiveIntrinsic::VmBitStringBitLength
        | CorePrimitiveIntrinsic::VmBitStringByteLength
        | CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUintBe
        | CorePrimitiveIntrinsic::VmBitStringToIntBe
        | CorePrimitiveIntrinsic::VmBitStringToUintLe
        | CorePrimitiveIntrinsic::VmBitStringToIntLe => CoreType::Int,
        CorePrimitiveIntrinsic::VmBitStringIsByteAligned => CoreType::Bool,
        CorePrimitiveIntrinsic::VmBitStringToBytes => CoreType::Named("Bytes".to_string()),
        CorePrimitiveIntrinsic::VmTimeoutMilliseconds
        | CorePrimitiveIntrinsic::VmTimeoutForever => CoreType::Named("Timeout".to_string()),
        CorePrimitiveIntrinsic::VmTcpListen | CorePrimitiveIntrinsic::VmTcpListenWithBacklog => {
            CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![
                    CoreType::Named("TcpListener".to_string()),
                    CoreType::Named("Error".to_string()),
                ],
            }
        }
        CorePrimitiveIntrinsic::VmTcpAccept | CorePrimitiveIntrinsic::VmTcpConnect => {
            CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![
                    CoreType::Named("TcpSocket".to_string()),
                    CoreType::Named("Error".to_string()),
                ],
            }
        }
        CorePrimitiveIntrinsic::VmTcpSend => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmTcpReceive => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Bytes".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmTcpClose | CorePrimitiveIntrinsic::VmTcpCloseListener => {
            CoreType::Named("Unit".to_string())
        }
        CorePrimitiveIntrinsic::VmPortOpen => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Port".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmPortWrite => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmPortRead => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Bytes".to_string()),
                CoreType::Named("Error".to_string()),
            ],
        },
        CorePrimitiveIntrinsic::VmPortClose => CoreType::Named("Unit".to_string()),
    }
}
