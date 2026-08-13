//! Typed CoreIR process-transition recognition for direct AOT lowering.

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

use super::{expression::native_type, NativeTransitionOperation, NativeType};

pub(super) use crate::runtime::native_image::control::{
    TVM_SQL_CAPABILITY_PREFIX_WORDS as SQL_CAPABILITY_PREFIX_WORDS,
    TVM_SQL_CAPABILITY_TAG as SQL_CAPABILITY_TAG,
};

pub(super) fn is_process_transition(expr: &CoreExpr) -> bool {
    process_transition(expr).is_some()
}

pub(super) fn process_transition(
    expr: &CoreExpr,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    if matches!(expr, CoreExpr::SqlQuery { .. }) {
        return sql_query_transition(expr);
    }
    let CoreExpr::Intrinsic(call) = expr else {
        return None;
    };
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmDebuggerBreak)
            if call.args.is_empty() =>
        {
            Some((NativeTransitionOperation::Debug, Vec::new(), None))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessYield)
            if call.args.is_empty() =>
        {
            Some((NativeTransitionOperation::Yield, Vec::new(), None))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendInt)
            if call.args.len() == 2 =>
        {
            Some((NativeTransitionOperation::Send, call.args.clone(), None))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendString) => {
            typed_send_transition(call, NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendBytes) => {
            typed_send_transition(call, NativeType::BytesRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendBinary) => {
            typed_send_transition(call, NativeType::BinaryRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendAtom) => {
            typed_send_transition(call, NativeType::Atom)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveInt)
            if call.args.is_empty() =>
        {
            Some((
                NativeTransitionOperation::Receive,
                Vec::new(),
                Some(NativeType::Int),
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveString) => {
            typed_receive_transition(call, NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveBytes) => {
            typed_receive_transition(call, NativeType::BytesRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveBinary) => {
            typed_receive_transition(call, NativeType::BinaryRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveAtom) => {
            typed_receive_transition(call, NativeType::Atom)
        }
        CoreIntrinsicId::VmProcessSendMessage(value_type) => typed_send_transition_as(
            call,
            native_type(Some(value_type), &value_type.contract_text())?,
            value_type,
        ),
        CoreIntrinsicId::VmProcessReceiveMessage(value_type) => typed_receive_transition(
            call,
            native_type(Some(value_type), &value_type.contract_text())?,
        ),
        CoreIntrinsicId::VmProcessCurrent(_) if call.args.is_empty() => Some((
            NativeTransitionOperation::Identity,
            Vec::new(),
            Some(NativeType::Int),
        )),
        CoreIntrinsicId::VmProcessSpawn(_) if call.args.len() == 1 => Some((
            NativeTransitionOperation::Spawn,
            call.args.clone(),
            Some(NativeType::Int),
        )),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSleep)
            if call.args.len() == 1 =>
        {
            Some((NativeTransitionOperation::Timer, call.args.clone(), None))
        }
        CoreIntrinsicId::VmProcessLink(_) if call.args.len() == 1 => {
            Some((NativeTransitionOperation::Link, call.args.clone(), None))
        }
        CoreIntrinsicId::VmProcessMonitor(_) if call.args.len() == 1 => Some((
            NativeTransitionOperation::Monitor,
            call.args.clone(),
            Some(NativeType::Int),
        )),
        CoreIntrinsicId::VmProcessAcquireResource(_) if call.args.len() == 1 => Some((
            NativeTransitionOperation::Resource,
            call.args.clone(),
            Some(NativeType::Int),
        )),
        CoreIntrinsicId::VmProcessCancel(_) if call.args.len() == 1 => Some((
            NativeTransitionOperation::Cancellation,
            call.args.clone(),
            None,
        )),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessFail)
            if call.args.len() == 1 =>
        {
            Some((NativeTransitionOperation::Failure, call.args.clone(), None))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSchedule)
            if call.args.len() == 1 =>
        {
            Some((
                NativeTransitionOperation::Scheduling,
                call.args.clone(),
                None,
            ))
        }
        CoreIntrinsicId::Runtime(capability) => capability_transition(call, capability),
        CoreIntrinsicId::NativeOperation {
            operation,
            parameter_types,
        } => native_operation_transition(call, operation, parameter_types),
        _ => None,
    }
}

/// Lowers one checked SQL form into a VM-owned asynchronous capability frame.
///
/// Parameter values remain as CoreIR here. Yield lowering has the complete
/// lexical type environment and expands each value to its exact three-word
/// boundary type plus value word before Cranelift emission.
fn sql_query_transition(
    expr: &CoreExpr,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    let CoreExpr::SqlQuery {
        row_type,
        bound_sql,
        parameters,
        query_kind,
        transaction_requirement,
        cardinality,
        result_type,
        result_core_type,
        projection_fields,
    } = expr
    else {
        return None;
    };
    let result = native_type(Some(result_core_type), result_type)?;
    let mut arguments = vec![CoreExpr::Int(SQL_CAPABILITY_TAG)];
    arguments.extend(typed_transition_metadata(result));
    arguments.extend([
        CoreExpr::Binary(format!("{row_type:?}")),
        CoreExpr::Binary(format!("{bound_sql:?}")),
        CoreExpr::Binary(format!("{query_kind:?}")),
        CoreExpr::Binary(format!("{transaction_requirement:?}")),
        CoreExpr::Binary(format!("{cardinality:?}")),
        CoreExpr::List(
            projection_fields
                .iter()
                .map(|field| CoreExpr::Binary(format!("{field:?}")))
                .collect(),
        ),
        CoreExpr::Int(parameters.len() as i64),
    ]);
    debug_assert_eq!(arguments.len(), SQL_CAPABILITY_PREFIX_WORDS);
    arguments.extend(parameters.iter().cloned());
    Some((
        NativeTransitionOperation::Capability,
        arguments,
        Some(result),
    ))
}

/// Lowers one compiler-native declaration into a typed package-capability frame.
fn native_operation_transition(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    operation: &str,
    parameter_types: &[crate::terlan_typeck::CoreType],
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    if call.args.len() != parameter_types.len() {
        return None;
    }
    let result = native_type(Some(&call.return_type), &call.return_type.contract_text())?;
    let parameter_native_types = parameter_types
        .iter()
        .map(|ty| native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()?;
    let mut arguments = vec![CoreExpr::Int(7)];
    arguments.extend(typed_transition_metadata(result));
    arguments.push(CoreExpr::Binary(format!("{operation:?}")));
    arguments.push(CoreExpr::Int(call.args.len() as i64));
    for ((argument, core_type), native_type) in call
        .args
        .iter()
        .zip(parameter_types)
        .zip(parameter_native_types)
    {
        arguments.extend(typed_transition_metadata(native_type));
        arguments.push(CoreExpr::Cast {
            expr: Box::new(argument.clone()),
            target_type: core_type.clone(),
        });
    }
    Some((
        NativeTransitionOperation::Capability,
        arguments,
        Some(result),
    ))
}

fn capability_transition(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    capability: &CoreRuntimeCapability,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    let (tag, arity) = match capability {
        CoreRuntimeCapability::ConsolePrintln => (1, 1),
        CoreRuntimeCapability::ConsoleEprintln => (35, 1),
        CoreRuntimeCapability::ClockUnixTimeNs => (36, 0),
        CoreRuntimeCapability::ClockMonotonicTimeNs => (37, 0),
        CoreRuntimeCapability::FileExists => (2, 1),
        CoreRuntimeCapability::FileReadText => (3, 1),
        CoreRuntimeCapability::FileReadBytes => (30, 1),
        CoreRuntimeCapability::FileSize => (38, 1),
        CoreRuntimeCapability::FileTimestamps => (40, 1),
        CoreRuntimeCapability::FileSetTimestamps => (41, 3),
        CoreRuntimeCapability::FileIsExecutable => (49, 1),
        CoreRuntimeCapability::FileSetExecutable => (50, 2),
        CoreRuntimeCapability::FileCopy => (52, 2),
        CoreRuntimeCapability::FileCopyMany => (54, 1),
        CoreRuntimeCapability::FileReadTextMany => (18, 1),
        CoreRuntimeCapability::FileReadTextDirectory => (19, 1),
        CoreRuntimeCapability::FileReadTextTreeExcluding => (20, 2),
        CoreRuntimeCapability::FileReadTextTreeMatching => (21, 6),
        CoreRuntimeCapability::FileWriteText => (4, 2),
        CoreRuntimeCapability::FileAppendText => (5, 2),
        CoreRuntimeCapability::FileDelete => (6, 1),
        CoreRuntimeCapability::SystemArgumentsCount => (8, 0),
        CoreRuntimeCapability::SystemArgumentsGet => (9, 1),
        CoreRuntimeCapability::SystemEnvironmentContains => (10, 1),
        CoreRuntimeCapability::SystemEnvironmentGet => (11, 1),
        CoreRuntimeCapability::SystemEnvironmentCurrentDirectory => (12, 0),
        CoreRuntimeCapability::SystemPlatformCurrentMetrics => (59, 0),
        CoreRuntimeCapability::SystemProcessLimits => (33, 0),
        CoreRuntimeCapability::SystemProcessRun => (22, 1),
        CoreRuntimeCapability::SystemProcessRunMany => (31, 1),
        CoreRuntimeCapability::SystemProcessRunLengthFramed => (48, 1),
        CoreRuntimeCapability::DirectoryEntries => (13, 1),
        CoreRuntimeCapability::DirectoryFilesRecursive => (14, 1),
        CoreRuntimeCapability::DirectoryFilesRecursiveExcluding => (17, 2),
        CoreRuntimeCapability::DirectoryFindNamedRecursiveExcluding => (39, 3),
        CoreRuntimeCapability::DirectoryTreeUsage => (32, 1),
        CoreRuntimeCapability::DirectoryCopyTreeExcluding => (24, 3),
        CoreRuntimeCapability::DirectoryCreateSymbolicLink => (34, 2),
        CoreRuntimeCapability::DirectoryCreateAll => (15, 1),
        CoreRuntimeCapability::DirectoryCreateTemporary => (23, 1),
        CoreRuntimeCapability::DirectoryRemoveAll => (16, 1),
        CoreRuntimeCapability::ArchiveCreate => (51, 2),
        CoreRuntimeCapability::ArchiveExtract => (43, 2),
        CoreRuntimeCapability::HashSha256File => (44, 1),
        CoreRuntimeCapability::HashVerifySha256Manifest => (45, 2),
        CoreRuntimeCapability::HashSha256Tree => (46, 1),
        CoreRuntimeCapability::HashSha256SelectedFiles => (47, 2),
        CoreRuntimeCapability::HashSha256LabeledFileDigests => (55, 1),
        CoreRuntimeCapability::HashSha256LabeledFileContents => (58, 1),
        CoreRuntimeCapability::HashAuditLabeledFiles => (56, 2),
        CoreRuntimeCapability::HashAuditLabeledFilePatterns => (57, 3),
        CoreRuntimeCapability::GitSourceTreeIdentity => (53, 1),
    };
    let result = native_type(Some(&call.return_type), &call.return_type.contract_text())?;
    (call.args.len() == arity).then(|| {
        let mut arguments = vec![CoreExpr::Int(tag)];
        arguments.extend(typed_transition_metadata(result));
        arguments.extend(call.args.clone());
        (
            NativeTransitionOperation::Capability,
            arguments,
            Some(result),
        )
    })
}

/// Lowers one exact typed mailbox send into its fixed transition frame.
fn typed_send_transition(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    native_type: NativeType,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    if call.args.len() != 2 {
        return None;
    }
    let mut arguments = vec![call.args[0].clone()];
    arguments.extend(typed_transition_metadata(native_type));
    arguments.push(call.args[1].clone());
    Some((NativeTransitionOperation::SendTyped, arguments, None))
}

/// Lowers a typed mailbox send while retaining the concrete source payload
/// type as a checked representation-preserving cast. Generic record lowering
/// uses that target to emit the exact monomorphized managed identity.
fn typed_send_transition_as(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    native_type: NativeType,
    value_type: &crate::terlan_typeck::CoreType,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    if call.args.len() != 2 {
        return None;
    }
    let mut arguments = vec![call.args[0].clone()];
    arguments.extend(typed_transition_metadata(native_type));
    arguments.push(CoreExpr::Cast {
        expr: Box::new(call.args[1].clone()),
        target_type: value_type.clone(),
    });
    Some((NativeTransitionOperation::SendTyped, arguments, None))
}

/// Lowers one exact typed mailbox receive into its fixed transition frame.
fn typed_receive_transition(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    native_type: NativeType,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    call.args.is_empty().then(|| {
        (
            NativeTransitionOperation::ReceiveTyped,
            typed_transition_metadata(native_type),
            Some(native_type),
        )
    })
}

/// Encodes one exact native type as scalar CoreIR transition arguments.
fn typed_transition_metadata(native_type: NativeType) -> Vec<CoreExpr> {
    native_type
        .boundary_type()
        .transition_words()
        .into_iter()
        .map(CoreExpr::Int)
        .collect()
}
