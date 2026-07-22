use cranelift_codegen::ir::{condcodes::IntCC, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;

use super::super::{status, NativeTransitionOperation};

pub(super) struct TransitionFlags {
    pub(super) sent: Value,
    pub(super) typed_sent: Value,
    pub(super) typed_received: Value,
    pub(super) injected_input: Value,
    pub(super) one_argument: Value,
    pub(super) transitioned: Value,
    pub(super) capability: Value,
}

pub(super) fn transition_status(operation: NativeTransitionOperation) -> i32 {
    match operation {
        NativeTransitionOperation::Yield => status::YIELD,
        NativeTransitionOperation::Send => status::SEND,
        NativeTransitionOperation::SendTyped => status::SEND_TYPED,
        NativeTransitionOperation::Receive => status::RECEIVE,
        NativeTransitionOperation::ReceiveTyped => status::RECEIVE_TYPED,
        NativeTransitionOperation::Spawn => status::SPAWN,
        NativeTransitionOperation::Timer => status::TIMER,
        NativeTransitionOperation::Link => status::LINK,
        NativeTransitionOperation::Monitor => status::MONITOR,
        NativeTransitionOperation::Resource => status::RESOURCE,
        NativeTransitionOperation::Cancellation => status::CANCELLATION,
        NativeTransitionOperation::Failure => status::FAILURE,
        NativeTransitionOperation::Scheduling => status::SCHEDULING,
        NativeTransitionOperation::Capability => status::CAPABILITY,
    }
}

pub(super) fn transition_flags(
    builder: &mut FunctionBuilder<'_>,
    call_status: Value,
) -> TransitionFlags {
    let yielded = status_flag(builder, call_status, status::YIELD);
    let sent = status_flag(builder, call_status, status::SEND);
    let typed_sent = status_flag(builder, call_status, status::SEND_TYPED);
    let received = status_flag(builder, call_status, status::RECEIVE);
    let typed_received = status_flag(builder, call_status, status::RECEIVE_TYPED);
    let spawned = status_flag(builder, call_status, status::SPAWN);
    let timed = status_flag(builder, call_status, status::TIMER);
    let linked = status_flag(builder, call_status, status::LINK);
    let monitored = status_flag(builder, call_status, status::MONITOR);
    let resource = status_flag(builder, call_status, status::RESOURCE);
    let cancelled = status_flag(builder, call_status, status::CANCELLATION);
    let failed = status_flag(builder, call_status, status::FAILURE);
    let scheduled = status_flag(builder, call_status, status::SCHEDULING);
    let capability = status_flag(builder, call_status, status::CAPABILITY);
    let any_sent = builder.ins().bor(sent, typed_sent);
    let yielded_or_sent = builder.ins().bor(yielded, any_sent);
    let timed_or_linked = builder.ins().bor(timed, linked);
    let effect_transition = builder.ins().bor(yielded_or_sent, timed_or_linked);
    let any_received = builder.ins().bor(received, typed_received);
    let received_or_spawned = builder.ins().bor(any_received, spawned);
    let monitored_or_resource = builder.ins().bor(monitored, resource);
    let injected_input = builder
        .ins()
        .bor(received_or_spawned, monitored_or_resource);
    let terminal = builder.ins().bor(cancelled, failed);
    let terminal_or_scheduled = builder.ins().bor(terminal, scheduled);
    let effect_or_terminal = builder.ins().bor(effect_transition, terminal_or_scheduled);
    let transitioned = builder.ins().bor(effect_or_terminal, injected_input);
    let transitioned = builder.ins().bor(transitioned, capability);
    let spawned_or_timed = builder.ins().bor(spawned, timed);
    let linked_or_monitored = builder.ins().bor(linked, monitored);
    let resource_or_terminal = builder.ins().bor(resource, terminal_or_scheduled);
    let relationship_or_resource = builder.ins().bor(linked_or_monitored, resource_or_terminal);
    let one_argument = builder
        .ins()
        .bor(spawned_or_timed, relationship_or_resource);
    TransitionFlags {
        sent,
        typed_sent,
        typed_received,
        injected_input,
        one_argument,
        transitioned,
        capability,
    }
}

fn status_flag(builder: &mut FunctionBuilder<'_>, call_status: Value, status: i32) -> Value {
    builder
        .ins()
        .icmp_imm(IntCC::Equal, call_status, i64::from(status))
}
