//! Typed CoreIR process-transition recognition for direct AOT lowering.

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

use super::{expression::native_type, NativeTransitionOperation, NativeType};

pub(super) fn is_process_transition(expr: &CoreExpr) -> bool {
    process_transition(expr).is_some()
}

pub(super) fn process_transition(
    expr: &CoreExpr,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    let CoreExpr::Intrinsic(call) = expr else {
        return None;
    };
    match &call.id {
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
        CoreIntrinsicId::VmProcessSendMessage(value_type) => typed_send_transition(
            call,
            native_type(Some(value_type), &value_type.contract_text())?,
        ),
        CoreIntrinsicId::VmProcessReceiveMessage(value_type) => typed_receive_transition(
            call,
            native_type(Some(value_type), &value_type.contract_text())?,
        ),
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
        _ => None,
    }
}

fn capability_transition(
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    capability: &CoreRuntimeCapability,
) -> Option<(NativeTransitionOperation, Vec<CoreExpr>, Option<NativeType>)> {
    let (tag, arity) = match capability {
        CoreRuntimeCapability::ConsolePrintln => (1, 1),
        CoreRuntimeCapability::FileExists => (2, 1),
        CoreRuntimeCapability::FileReadText => (3, 1),
        CoreRuntimeCapability::FileWriteText => (4, 2),
        CoreRuntimeCapability::FileAppendText => (5, 2),
        CoreRuntimeCapability::FileDelete => (6, 1),
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
