use super::*;

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
pub(crate) fn core_primitive_intrinsic(
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
        "std.core.Object" => core_map_primitive_intrinsic(function, arity),
        "std.collections.Set" => core_set_primitive_intrinsic(function, arity),
        "std.core.Task" => core_task_primitive_intrinsic(function, arity),
        "std.beam.Agent" => core_beam_agent_primitive_intrinsic(function, arity),
        "std.beam.GenServer" => core_beam_gen_server_primitive_intrinsic(function, arity),
        "std.beam.NativeBridge" => core_beam_native_bridge_primitive_intrinsic(function, arity),
        "std.beam.Bytes" => core_beam_bytes_primitive_intrinsic(function, arity),
        "std.beam.Timeout" => core_beam_timeout_primitive_intrinsic(function, arity),
        "std.beam.Tcp" => core_beam_tcp_primitive_intrinsic(function, arity),
        "std.beam.Port" => core_beam_port_primitive_intrinsic(function, arity),
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
/// - Maps source APIs such as `std.io.Console.println(value)` and
///   `std.log.info(value)` to backend-neutral CoreIR runtime capability
///   identities without carrying target module names into CoreIR.
pub(super) fn core_runtime_capability(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CoreRuntimeCapability> {
    match (module, function, arity) {
        ("std.io.Console", "println", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
        ("std.log", "debug", 1)
        | ("std.log", "info", 1)
        | ("std.log", "warn", 1)
        | ("std.log", "error", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
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
        ("from_entries", 1) => Some(CorePrimitiveIntrinsic::MapFromEntries),
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
        ("from_list", 1) => Some(CorePrimitiveIntrinsic::SetFromList),
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

/// Resolves a `std.beam.Bytes` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Bytes`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable BEAM byte-buffer
///   operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the byte-buffer contract to closed CoreIR identities so protocol
///   tests can use typed buffers without exposing Erlang binary syntax.
fn core_beam_bytes_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("from_list", 1) => Some(CorePrimitiveIntrinsic::BeamBytesFromList),
        ("to_list", 1) => Some(CorePrimitiveIntrinsic::BeamBytesToList),
        ("length", 1) => Some(CorePrimitiveIntrinsic::BeamBytesLength),
        ("concat", 2) => Some(CorePrimitiveIntrinsic::BeamBytesConcat),
        _ => None,
    }
}

/// Resolves a `std.beam.Timeout` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Timeout`
///   module path.
/// - `arity`: source-visible argument count.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable timeout constructors.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Keeps BEAM timeout representation target-owned while source tests use a
///   typed timeout value.
fn core_beam_timeout_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("milliseconds", 1) => Some(CorePrimitiveIntrinsic::BeamTimeoutMilliseconds),
        ("forever", 0) => Some(CorePrimitiveIntrinsic::BeamTimeoutForever),
        _ => None,
    }
}

/// Resolves a `std.beam.Tcp` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Tcp` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable TCP operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps TCP socket lifecycle operations to closed CoreIR identities so
///   daemon tests can depend on typed sockets instead of backend modules.
fn core_beam_tcp_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("connect", 3) => Some(CorePrimitiveIntrinsic::BeamTcpConnect),
        ("send", 2) => Some(CorePrimitiveIntrinsic::BeamTcpSend),
        ("receive", 3) => Some(CorePrimitiveIntrinsic::BeamTcpReceive),
        ("close", 1) => Some(CorePrimitiveIntrinsic::BeamTcpClose),
        _ => None,
    }
}

/// Resolves a `std.beam.Port` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.beam.Port` module
///   path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable external-port operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps external process lifecycle operations to closed CoreIR identities
///   while leaving command construction as ordinary Terlan structs.
fn core_beam_port_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("open", 1) => Some(CorePrimitiveIntrinsic::BeamPortOpen),
        ("write", 2) => Some(CorePrimitiveIntrinsic::BeamPortWrite),
        ("read", 3) => Some(CorePrimitiveIntrinsic::BeamPortRead),
        ("close", 1) => Some(CorePrimitiveIntrinsic::BeamPortClose),
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
        ("start", 0) => Some(CorePrimitiveIntrinsic::BeamSupervisorStartRoot),
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
