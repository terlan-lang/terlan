
/// Returns whether CoreV0 may execute a Core intrinsic through Terlan VM.
///
/// Inputs:
/// - `profile`: target profile under validation.
/// - `call`: intrinsic payload produced by CoreIR lowering.
///
/// Output:
/// - `true` for primitive intrinsics implemented by the VM evaluator.
///
/// Transformation:
/// - Mirrors the VM-owned primitive subset without admitting VM-only
///   intrinsics or arbitrary runtime effects.
pub(in crate::validation::target_profile) fn target_profile_supports_vm_intrinsic(
    profile: TargetProfile,
    call: &CoreIntrinsicCall,
) -> bool {
    matches!(profile, TargetProfile::CoreV0)
        && matches!(
            call.id,
            CoreIntrinsicId::Primitive(
                CorePrimitiveIntrinsic::TypeOf
                    | CorePrimitiveIntrinsic::IsType
                    | CorePrimitiveIntrinsic::BoolEqual
                    | CorePrimitiveIntrinsic::BoolCompare
                    | CorePrimitiveIntrinsic::BoolToString
                    | CorePrimitiveIntrinsic::BoolFromString
                    | CorePrimitiveIntrinsic::AtomToString
                    | CorePrimitiveIntrinsic::ValueToString
                    | CorePrimitiveIntrinsic::IntToString
                    | CorePrimitiveIntrinsic::IntFromString
                    | CorePrimitiveIntrinsic::IntToStringBase
                    | CorePrimitiveIntrinsic::IntFromStringBase
                    | CorePrimitiveIntrinsic::FloatToString
                    | CorePrimitiveIntrinsic::FloatFromString
                    | CorePrimitiveIntrinsic::FloatFloor
                    | CorePrimitiveIntrinsic::FloatCeil
                    | CorePrimitiveIntrinsic::FloatLog
                    | CorePrimitiveIntrinsic::FloatPi
                    | CorePrimitiveIntrinsic::FloatTau
                    | CorePrimitiveIntrinsic::StringEqual
                    | CorePrimitiveIntrinsic::StringCompare
                    | CorePrimitiveIntrinsic::StringToString
                    | CorePrimitiveIntrinsic::StringFromString
                    | CorePrimitiveIntrinsic::StringIsEmpty
                    | CorePrimitiveIntrinsic::StringAppend
                    | CorePrimitiveIntrinsic::StringConcat
                    | CorePrimitiveIntrinsic::StringContains
                    | CorePrimitiveIntrinsic::StringStartsWith
                    | CorePrimitiveIntrinsic::StringEndsWith
                    | CorePrimitiveIntrinsic::StringLength
                    | CorePrimitiveIntrinsic::StringByteSize
                    | CorePrimitiveIntrinsic::StringLowercase
                    | CorePrimitiveIntrinsic::StringUppercase
                    | CorePrimitiveIntrinsic::StringReverse
                    | CorePrimitiveIntrinsic::StringTrim
                    | CorePrimitiveIntrinsic::StringTrimStart
                    | CorePrimitiveIntrinsic::StringTrimEnd
                    | CorePrimitiveIntrinsic::StringReplace
                    | CorePrimitiveIntrinsic::StringSplit
                    | CorePrimitiveIntrinsic::StringSplitOnce
                    | CorePrimitiveIntrinsic::ListNew
                    | CorePrimitiveIntrinsic::ListIsEmpty
                    | CorePrimitiveIntrinsic::ListLength
                    | CorePrimitiveIntrinsic::ListFirst
                    | CorePrimitiveIntrinsic::ListRest
                    | CorePrimitiveIntrinsic::ListConcat
                    | CorePrimitiveIntrinsic::ListSubtract
                    | CorePrimitiveIntrinsic::ListIterator
                    | CorePrimitiveIntrinsic::ListPush
                    | CorePrimitiveIntrinsic::ListClear
                    | CorePrimitiveIntrinsic::IteratorNext
                    | CorePrimitiveIntrinsic::MapNew
                    | CorePrimitiveIntrinsic::MapFromEntries
                    | CorePrimitiveIntrinsic::MapIsEmpty
                    | CorePrimitiveIntrinsic::MapSize
                    | CorePrimitiveIntrinsic::MapGet
                    | CorePrimitiveIntrinsic::MapTake
                    | CorePrimitiveIntrinsic::MapContainsKey
                    | CorePrimitiveIntrinsic::MapIterator
                    | CorePrimitiveIntrinsic::MapPut
                    | CorePrimitiveIntrinsic::MapRemove
                    | CorePrimitiveIntrinsic::MapClear
                    | CorePrimitiveIntrinsic::SetNew
                    | CorePrimitiveIntrinsic::SetFromList
                    | CorePrimitiveIntrinsic::SetIsEmpty
                    | CorePrimitiveIntrinsic::SetSize
                    | CorePrimitiveIntrinsic::SetContains
                    | CorePrimitiveIntrinsic::SetIterator
                    | CorePrimitiveIntrinsic::SetAdd
                    | CorePrimitiveIntrinsic::SetRemove
                    | CorePrimitiveIntrinsic::SetClear
                    | CorePrimitiveIntrinsic::TaskDone
                    | CorePrimitiveIntrinsic::TaskResult
            ) | CoreIntrinsicId::Runtime(
                CoreRuntimeCapability::ConsolePrintln
                    | CoreRuntimeCapability::FileExists
                    | CoreRuntimeCapability::FileReadText
                    | CoreRuntimeCapability::FileWriteText
                    | CoreRuntimeCapability::FileAppendText
                    | CoreRuntimeCapability::FileDelete
            )
        )
}

/// Returns whether the current target profile owns a concrete Agent operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.vm.Agent` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Agent type contract available while forcing executable runtime
///   operations to wait for an explicit VM backend implementation.
pub(super) fn target_profile_supports_vm_agent_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm)
        && matches!(
            function,
            "start" | "get" | "get_and_update" | "update" | "cast" | "stop"
        )
}

/// Returns whether the current target profile owns a concrete GenServer operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.vm.GenServer` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Admits only the first VM callback-process operations implemented
///   through the shared VM process intrinsic layer.
pub(super) fn target_profile_supports_vm_gen_server_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm) && matches!(function, "start" | "call" | "cast" | "stop")
}

/// Returns whether the current target profile owns a concrete NativeBridge operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.vm.NativeBridge` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the NativeBridge callable contract visible to source and summaries
///   while admitting only VM-profile operations that have compiler-owned
///   bridge proof lowering.
pub(super) fn target_profile_supports_vm_native_bridge_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm)
        && matches!(function, "start" | "call" | "dispose" | "stop")
}

/// Returns whether the current target profile owns a concrete Supervisor operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.vm.Supervisor` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Supervisor callable contract visible to source and summaries
///   while admitting only the VM profile operations that have local
///   compiler-owned lowering.
pub(super) fn target_profile_supports_vm_supervisor_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm) && matches!(function, "child_spec" | "start" | "stop")
}

/// Returns whether the current target profile owns a concrete VM Task operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.vm.Task` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the VM Task type contract available while rejecting executable
///   task process operations until they are implemented through the shared
///   VM process intrinsic layer.
pub(super) fn target_profile_supports_vm_task_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm) && matches!(function, "start" | "result" | "cancel")
}

/// Appends a target-profile violation for one std runtime operation family.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `module`: remote-call module identity.
/// - `function`: remote-call function identity.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching runtime calls
///   that do not have backend support.
///
/// Transformation:
/// - Converts concrete runtime API calls into stable unsupported-target
///   diagnostics while leaving the type-level std contracts available for
///   parsing, resolution, and typechecking. This is the shared target-profile
///   validation path for Task, Agent, and future VM process abstractions.
pub(super) fn validate_std_runtime_operation_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    module: &str,
    function: &str,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if is_std_runtime_module(module, canonical_module, call_heads)
        && !supports_operation(profile, function)
    {
        violations.push(TargetProfileViolation::unsupported(
            diagnostic_label,
            profile,
            &format!("{function_scope} {location}"),
            &format!("{canonical_module}.{function}"),
        ));
    }
}

/// Appends a target-profile violation for summary-only std runtime operations.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `summary`: expression summary that may describe a remote std runtime call.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching remote call
///   summaries without backend support.
///
/// Transformation:
/// - Reads the summary's remote module and first child callee text to detect
///   runtime operations before full typed Core payload lowering exists for that
///   call family. This keeps summary-only and typed payload validation aligned.
pub(super) fn validate_std_runtime_operation_summary_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if matches!(
        summary.core_expr.as_ref(),
        Some(CoreExpr::RemoteCall { .. } | CoreExpr::Intrinsic(_))
    ) {
        return;
    }

    let Some(module) = summary.remote.as_deref() else {
        return;
    };
    if !is_std_runtime_module(module, canonical_module, call_heads) {
        return;
    }

    let function = summary
        .children
        .first()
        .and_then(|child| child.text.as_deref())
        .unwrap_or("<unknown>");
    if supports_operation(profile, function) {
        return;
    }
    violations.push(TargetProfileViolation::unsupported(
        diagnostic_label,
        profile,
        &format!("{function_scope} {location}"),
        &format!("{canonical_module}.{function}"),
    ));
}
