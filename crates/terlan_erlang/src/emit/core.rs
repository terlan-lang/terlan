use terlan_typeck::{
    CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

use super::erl::*;

mod annotations;
mod beam;
mod collections;
mod expr;
mod helpers;
mod patterns;
mod runtime;
mod scalar;
mod type_values;

pub(super) use annotations::{
    lower_intrinsic_annotation_body, lower_intrinsic_annotation_body_for_names,
};
use beam::*;
use collections::*;
#[cfg(test)]
pub(super) use expr::lower_core_expr_to_erlang;
use expr::lower_core_exprs_to_erlang;
use helpers::*;
use patterns::*;
pub(super) use runtime::{
    lower_runtime_console_println, lower_runtime_file_append_text, lower_runtime_file_delete,
    lower_runtime_file_exists, lower_runtime_file_read_text, lower_runtime_file_write_text,
};
use scalar::*;
use type_values::*;

/// Lowers a CoreIR intrinsic call into an Erlang expression.
///
/// Inputs:
/// - `call`: typed backend-neutral intrinsic call.
///
/// Output:
/// - `Some(ErlExpr)` for the currently supported primitive intrinsic set.
/// - `None` for malformed arities or intrinsic variants not handled by the
///   Erlang backend.
///
/// Transformation:
/// - Selects target runtime operations for primitive conversions, string
///   operations, and admitted runtime capabilities while keeping source-facing
///   `std` APIs and CoreIR registry keys backend neutral.
#[allow(dead_code)]
pub(super) fn lower_core_intrinsic_call_to_erlang(call: &CoreIntrinsicCall) -> Option<ErlExpr> {
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TypeOf) => {
            return lower_core_type_of(&call.args);
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IsType) => {
            return lower_core_is_type(&call.args);
        }
        _ => {}
    }

    let args = lower_core_exprs_to_erlang(&call.args)?;
    match &call.id {
        CoreIntrinsicId::Primitive(intrinsic) => {
            lower_core_primitive_intrinsic_to_erlang(intrinsic, args)
        }
        CoreIntrinsicId::Runtime(capability) => match capability {
            CoreRuntimeCapability::ConsolePrintln => lower_runtime_console_println(args),
            CoreRuntimeCapability::FileExists => lower_runtime_file_exists(args),
            CoreRuntimeCapability::FileReadText => lower_runtime_file_read_text(args),
            CoreRuntimeCapability::FileWriteText => lower_runtime_file_write_text(args),
            CoreRuntimeCapability::FileAppendText => lower_runtime_file_append_text(args),
            CoreRuntimeCapability::FileDelete => lower_runtime_file_delete(args),
        },
    }
}

/// Lowers a primitive intrinsic with already-lowered Erlang arguments.
///
/// Inputs:
/// - `intrinsic`: compiler-owned primitive intrinsic identity.
/// - `args`: Erlang expressions already lowered from source or CoreIR
///   arguments.
///
/// Output:
/// - Erlang expression implementing the primitive intrinsic.
///
/// Transformation:
/// - Centralizes primitive runtime lowering so the transitional syntax bridge
///   and formal CoreIR emitter use the same BEAM implementation for primitive
///   receiver and module-call surfaces.
pub(super) fn lower_core_primitive_intrinsic_to_erlang(
    intrinsic: &CorePrimitiveIntrinsic,
    args: Vec<ErlExpr>,
) -> Option<ErlExpr> {
    match intrinsic {
        CorePrimitiveIntrinsic::TypeOf => lower_core_type_of_intrinsic(args),
        CorePrimitiveIntrinsic::IsType => lower_core_is_type_intrinsic(args),
        CorePrimitiveIntrinsic::BoolEqual => lower_core_bool_equal(args),
        CorePrimitiveIntrinsic::BoolCompare => lower_core_bool_compare(args),
        CorePrimitiveIntrinsic::BoolToString => lower_core_bool_to_string(args),
        CorePrimitiveIntrinsic::BoolFromString => lower_core_bool_from_string(args),
        CorePrimitiveIntrinsic::AtomToString => lower_core_atom_to_string(args),
        CorePrimitiveIntrinsic::IntToString => lower_core_int_to_string(args),
        CorePrimitiveIntrinsic::IntFromString => lower_core_int_from_string(args),
        CorePrimitiveIntrinsic::FloatToString => lower_core_float_to_string(args),
        CorePrimitiveIntrinsic::FloatFromString => lower_core_float_from_string(args),
        CorePrimitiveIntrinsic::StringEqual => lower_core_string_equal(args),
        CorePrimitiveIntrinsic::StringCompare => lower_core_string_compare(args),
        CorePrimitiveIntrinsic::StringToString => lower_core_string_to_string(args),
        CorePrimitiveIntrinsic::StringFromString => lower_core_string_from_string(args),
        CorePrimitiveIntrinsic::StringIsEmpty => lower_core_string_is_empty(args),
        CorePrimitiveIntrinsic::StringAppend => lower_core_string_append(args),
        CorePrimitiveIntrinsic::StringConcat => lower_core_string_concat(args),
        CorePrimitiveIntrinsic::StringContains => lower_core_string_contains(args),
        CorePrimitiveIntrinsic::StringStartsWith => lower_core_string_starts_with(args),
        CorePrimitiveIntrinsic::StringEndsWith => lower_core_string_ends_with(args),
        CorePrimitiveIntrinsic::StringLength => lower_core_string_length(args),
        CorePrimitiveIntrinsic::StringByteSize => lower_core_string_byte_size(args),
        CorePrimitiveIntrinsic::StringLowercase => lower_core_string_unary_call("lowercase", args),
        CorePrimitiveIntrinsic::StringUppercase => lower_core_string_unary_call("uppercase", args),
        CorePrimitiveIntrinsic::StringTrim => lower_core_string_trim(args),
        CorePrimitiveIntrinsic::StringTrimStart => lower_core_string_trim_mode("leading", args),
        CorePrimitiveIntrinsic::StringTrimEnd => lower_core_string_trim_mode("trailing", args),
        CorePrimitiveIntrinsic::StringReplace => lower_core_string_replace(args),
        CorePrimitiveIntrinsic::StringSplit => lower_core_string_split(args),
        CorePrimitiveIntrinsic::StringSplitOnce => lower_core_string_split_once(args),
        CorePrimitiveIntrinsic::ListNew => lower_core_list_new(args),
        CorePrimitiveIntrinsic::ListIsEmpty => lower_core_list_is_empty(args),
        CorePrimitiveIntrinsic::ListLength => lower_core_list_length(args),
        CorePrimitiveIntrinsic::ListFirst => lower_core_list_first(args),
        CorePrimitiveIntrinsic::ListIterator => lower_core_list_iterator(args),
        CorePrimitiveIntrinsic::ListPush => lower_core_list_push(args),
        CorePrimitiveIntrinsic::ListClear => lower_core_list_clear(args),
        CorePrimitiveIntrinsic::IteratorNext => lower_core_iterator_next(args),
        CorePrimitiveIntrinsic::MapNew => lower_core_map_new(args),
        CorePrimitiveIntrinsic::MapIsEmpty => lower_core_map_is_empty(args),
        CorePrimitiveIntrinsic::MapSize => lower_core_map_size(args),
        CorePrimitiveIntrinsic::MapGet => lower_core_map_get(args),
        CorePrimitiveIntrinsic::MapContainsKey => lower_core_map_contains_key(args),
        CorePrimitiveIntrinsic::MapIterator => lower_core_map_iterator(args),
        CorePrimitiveIntrinsic::MapPut => lower_core_map_put(args),
        CorePrimitiveIntrinsic::MapRemove => lower_core_map_remove(args),
        CorePrimitiveIntrinsic::MapClear => lower_core_map_clear(args),
        CorePrimitiveIntrinsic::SetNew => lower_core_set_new(args),
        CorePrimitiveIntrinsic::SetIsEmpty => lower_core_set_is_empty(args),
        CorePrimitiveIntrinsic::SetSize => lower_core_set_size(args),
        CorePrimitiveIntrinsic::SetContains => lower_core_set_contains(args),
        CorePrimitiveIntrinsic::SetIterator => lower_core_set_iterator(args),
        CorePrimitiveIntrinsic::SetAdd => lower_core_set_add(args),
        CorePrimitiveIntrinsic::SetRemove => lower_core_set_remove(args),
        CorePrimitiveIntrinsic::SetClear => lower_core_set_clear(args),
        CorePrimitiveIntrinsic::TaskDone => lower_core_task_done(args),
        CorePrimitiveIntrinsic::TaskResult => lower_core_task_result(args),
        CorePrimitiveIntrinsic::BeamAgentStart => lower_beam_agent_start(args),
        CorePrimitiveIntrinsic::BeamAgentGet => lower_beam_agent_get(args),
        CorePrimitiveIntrinsic::BeamAgentGetAndUpdate => lower_beam_agent_get_and_update(args),
        CorePrimitiveIntrinsic::BeamAgentUpdate => lower_beam_agent_update(args),
        CorePrimitiveIntrinsic::BeamAgentCast => lower_beam_agent_cast(args),
        CorePrimitiveIntrinsic::BeamAgentStop => lower_beam_agent_stop(args),
        CorePrimitiveIntrinsic::BeamGenServerStart => lower_beam_gen_server_start(args),
        CorePrimitiveIntrinsic::BeamGenServerCall => lower_beam_gen_server_call(args),
        CorePrimitiveIntrinsic::BeamGenServerCast => lower_beam_gen_server_cast(args),
        CorePrimitiveIntrinsic::BeamGenServerStop => lower_beam_gen_server_stop(args),
        CorePrimitiveIntrinsic::BeamNativeBridgeStart => lower_beam_native_bridge_start(args),
        CorePrimitiveIntrinsic::BeamNativeBridgeCall => lower_beam_native_bridge_call(args),
        CorePrimitiveIntrinsic::BeamNativeBridgeDispose => lower_beam_native_bridge_dispose(args),
        CorePrimitiveIntrinsic::BeamNativeBridgeStop => lower_beam_native_bridge_stop(args),
        CorePrimitiveIntrinsic::BeamSupervisorChildSpec => lower_beam_supervisor_child_spec(args),
        CorePrimitiveIntrinsic::BeamSupervisorStart => lower_beam_supervisor_start(args),
        CorePrimitiveIntrinsic::BeamSupervisorStop => lower_beam_supervisor_stop(args),
        CorePrimitiveIntrinsic::BeamTaskStart => lower_beam_task_start(args),
        CorePrimitiveIntrinsic::BeamTaskResult => lower_beam_task_result(args),
        CorePrimitiveIntrinsic::BeamTaskCancel => lower_beam_task_cancel(args),
    }
}
