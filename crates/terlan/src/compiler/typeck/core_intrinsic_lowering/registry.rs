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
        "std.core.Memory" => core_memory_primitive_intrinsic(function, arity),
        "std.core.Int" => core_int_primitive_intrinsic(function, arity),
        "std.core.Float" => core_float_primitive_intrinsic(function, arity),
        "std.core.String" => core_string_primitive_intrinsic(function, arity),
        "std.crypto.Hash" => core_crypto_hash_primitive_intrinsic(function, arity),
        "std.collections.List" => core_list_primitive_intrinsic(function, arity),
        "std.collections.Iterator" => core_iterator_primitive_intrinsic(function, arity),
        "std.collections.Map" => core_map_primitive_intrinsic(function, arity),
        "std.core.Object" => core_map_primitive_intrinsic(function, arity),
        "std.collections.Set" => core_set_primitive_intrinsic(function, arity),
        "std.core.Task" => core_task_primitive_intrinsic(function, arity),
        "std.core.Effect" => core_vm_effect_primitive_intrinsic(function, arity),
        "std.vm.Debugger" => core_vm_debugger_primitive_intrinsic(function, arity),
        "std.vm.Process" => core_vm_process_primitive_intrinsic(function, arity),
        "std.vm.NativeBridge" => core_vm_native_bridge_primitive_intrinsic(function, arity),
        "std.vm.Bytes" => core_vm_bytes_primitive_intrinsic(function, arity),
        "std.vm.BitString" => core_vm_bitstring_primitive_intrinsic(function, arity),
        "std.vm.Timeout" => core_vm_timeout_primitive_intrinsic(function, arity),
        "std.vm.Tcp" => core_vm_tcp_primitive_intrinsic(function, arity),
        "std.vm.Port" => core_vm_port_primitive_intrinsic(function, arity),
        _ => None,
    }
}

fn core_crypto_hash_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("sha256", 1) => Some(CorePrimitiveIntrinsic::CryptoSha256),
        _ => None,
    }
}

fn core_vm_debugger_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("debug", 0) => Some(CorePrimitiveIntrinsic::VmDebuggerBreak),
        _ => None,
    }
}

/// Resolves physical-layout descriptor projections to closed intrinsics.
fn core_memory_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("size", 1) => Some(CorePrimitiveIntrinsic::MemoryLayoutSize),
        ("alignment", 1) => Some(CorePrimitiveIntrinsic::MemoryLayoutAlignment),
        ("storage", 1) => Some(CorePrimitiveIntrinsic::MemoryLayoutStorage),
        _ => None,
    }
}

/// Resolves scheduler-facing process operations to closed CoreIR identities.
fn core_vm_process_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("yield_now", 0) => Some(CorePrimitiveIntrinsic::VmProcessYield),
        ("send_int", 2) => Some(CorePrimitiveIntrinsic::VmProcessSendInt),
        ("receive_int", 0) => Some(CorePrimitiveIntrinsic::VmProcessReceiveInt),
        ("send_string", 2) => Some(CorePrimitiveIntrinsic::VmProcessSendString),
        ("receive_string", 0) => Some(CorePrimitiveIntrinsic::VmProcessReceiveString),
        ("send_bytes", 2) => Some(CorePrimitiveIntrinsic::VmProcessSendBytes),
        ("receive_bytes", 0) => Some(CorePrimitiveIntrinsic::VmProcessReceiveBytes),
        ("send_binary", 2) => Some(CorePrimitiveIntrinsic::VmProcessSendBinary),
        ("receive_binary", 0) => Some(CorePrimitiveIntrinsic::VmProcessReceiveBinary),
        ("send_atom", 2) => Some(CorePrimitiveIntrinsic::VmProcessSendAtom),
        ("receive_atom", 0) => Some(CorePrimitiveIntrinsic::VmProcessReceiveAtom),
        ("sleep", 1) => Some(CorePrimitiveIntrinsic::VmProcessSleep),
        ("fail", 1) => Some(CorePrimitiveIntrinsic::VmProcessFail),
        ("schedule", 1) => Some(CorePrimitiveIntrinsic::VmProcessSchedule),
        _ => None,
    }
}

/// Resolves the explicit VM execution boundary for `std.core.Effect`.
///
/// Inputs:
/// - `function`: source-level Effect operation name.
/// - `arity`: argument count after call normalization.
///
/// Output:
/// - `Some(VmEffectRun)` only for `run/1`.
///
/// Transformation:
/// - Keeps pure Effect constructors as ordinary source functions while giving
///   runtime execution one closed CoreIR identity.
fn core_vm_effect_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("run", 1) => Some(CorePrimitiveIntrinsic::VmEffectRun),
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
///   `std.log.Log.info(value)` to backend-neutral CoreIR runtime capability
///   identities without carrying target module names into CoreIR.
pub(super) fn core_runtime_capability(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CoreRuntimeCapability> {
    match (module, function, arity) {
        ("std.io.Console", "println", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
        ("std.io.Console", "eprintln", 1) => Some(CoreRuntimeCapability::ConsoleEprintln),
        ("std.time.Clock", "unix_time_ns", 0) => Some(CoreRuntimeCapability::ClockUnixTimeNs),
        ("std.time.Clock", "monotonic_time_ns", 0) => {
            Some(CoreRuntimeCapability::ClockMonotonicTimeNs)
        }
        ("std.log.Log", "debug", 1)
        | ("std.log.Log", "info", 1)
        | ("std.log.Log", "warn", 1)
        | ("std.log.Log", "error", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
        ("std.io.File", "exists", 1) => Some(CoreRuntimeCapability::FileExists),
        ("std.io.File", "read_text", 1) => Some(CoreRuntimeCapability::FileReadText),
        ("std.io.File", "read_bytes", 1) => Some(CoreRuntimeCapability::FileReadBytes),
        ("std.io.File", "size", 1) => Some(CoreRuntimeCapability::FileSize),
        ("std.io.File", "timestamps", 1) => Some(CoreRuntimeCapability::FileTimestamps),
        ("std.io.File", "set_timestamps", 3) => Some(CoreRuntimeCapability::FileSetTimestamps),
        ("std.io.File", "is_executable", 1) => Some(CoreRuntimeCapability::FileIsExecutable),
        ("std.io.File", "set_executable", 2) => Some(CoreRuntimeCapability::FileSetExecutable),
        ("std.io.File", "copy", 2) => Some(CoreRuntimeCapability::FileCopy),
        ("std.io.File", "copy_many", 1) => Some(CoreRuntimeCapability::FileCopyMany),
        ("std.io.File", "read_text_many", 1) => Some(CoreRuntimeCapability::FileReadTextMany),
        ("std.io.File", "read_text_directory", 1) => {
            Some(CoreRuntimeCapability::FileReadTextDirectory)
        }
        ("std.io.File", "read_text_tree_excluding", 2) => {
            Some(CoreRuntimeCapability::FileReadTextTreeExcluding)
        }
        ("std.io.File", "read_text_tree_matching", 6) => {
            Some(CoreRuntimeCapability::FileReadTextTreeMatching)
        }
        ("std.io.File", "write_text", 2) => Some(CoreRuntimeCapability::FileWriteText),
        ("std.io.File", "append_text", 2) => Some(CoreRuntimeCapability::FileAppendText),
        ("std.io.File", "delete", 1) => Some(CoreRuntimeCapability::FileDelete),
        ("std.system.Arguments", "count", 0) => Some(CoreRuntimeCapability::SystemArgumentsCount),
        ("std.system.Arguments", "get", 1) => Some(CoreRuntimeCapability::SystemArgumentsGet),
        ("std.system.Environment", "contains", 1) => {
            Some(CoreRuntimeCapability::SystemEnvironmentContains)
        }
        ("std.system.Environment", "get", 1) => Some(CoreRuntimeCapability::SystemEnvironmentGet),
        ("std.system.Environment", "current_directory", 0) => {
            Some(CoreRuntimeCapability::SystemEnvironmentCurrentDirectory)
        }
        ("std.system.Platform", "current_metrics", 0) => {
            Some(CoreRuntimeCapability::SystemPlatformCurrentMetrics)
        }
        ("std.system.Process", "limits", 0) => Some(CoreRuntimeCapability::SystemProcessLimits),
        ("std.system.Process", "run", 1) => Some(CoreRuntimeCapability::SystemProcessRun),
        ("std.system.Process", "run_many", 1) => Some(CoreRuntimeCapability::SystemProcessRunMany),
        ("std.system.Process", "run_length_framed", 1) => {
            Some(CoreRuntimeCapability::SystemProcessRunLengthFramed)
        }
        ("std.io.Directory", "entries", 1) => Some(CoreRuntimeCapability::DirectoryEntries),
        ("std.io.Directory", "files_recursive", 1) => {
            Some(CoreRuntimeCapability::DirectoryFilesRecursive)
        }
        ("std.io.Directory", "files_recursive_excluding", 2) => {
            Some(CoreRuntimeCapability::DirectoryFilesRecursiveExcluding)
        }
        ("std.io.Directory", "find_named_recursive_excluding", 3) => {
            Some(CoreRuntimeCapability::DirectoryFindNamedRecursiveExcluding)
        }
        ("std.io.Directory", "tree_usage", 1) => Some(CoreRuntimeCapability::DirectoryTreeUsage),
        ("std.io.Directory", "copy_tree_excluding", 3) => {
            Some(CoreRuntimeCapability::DirectoryCopyTreeExcluding)
        }
        ("std.io.Directory", "create_symbolic_link", 2) => {
            Some(CoreRuntimeCapability::DirectoryCreateSymbolicLink)
        }
        ("std.io.Directory", "create_all", 1) => Some(CoreRuntimeCapability::DirectoryCreateAll),
        ("std.io.Directory", "create_temporary", 1) => {
            Some(CoreRuntimeCapability::DirectoryCreateTemporary)
        }
        ("std.io.Directory", "remove_all", 1) => Some(CoreRuntimeCapability::DirectoryRemoveAll),
        ("std.io.Archive", "create", 2) => Some(CoreRuntimeCapability::ArchiveCreate),
        ("std.io.Archive", "extract", 2) => Some(CoreRuntimeCapability::ArchiveExtract),
        ("std.crypto.Hash", "sha256_file", 1) => Some(CoreRuntimeCapability::HashSha256File),
        ("std.crypto.Hash", "verify_sha256_manifest", 2) => {
            Some(CoreRuntimeCapability::HashVerifySha256Manifest)
        }
        ("std.crypto.Hash", "sha256_tree", 1) => Some(CoreRuntimeCapability::HashSha256Tree),
        ("std.crypto.Hash", "sha256_selected_files", 2) => {
            Some(CoreRuntimeCapability::HashSha256SelectedFiles)
        }
        ("std.crypto.Hash", "sha256_labeled_file_digests", 1) => {
            Some(CoreRuntimeCapability::HashSha256LabeledFileDigests)
        }
        ("std.crypto.Hash", "sha256_labeled_file_contents", 1) => {
            Some(CoreRuntimeCapability::HashSha256LabeledFileContents)
        }
        ("std.crypto.Hash", "audit_labeled_files", 2) => {
            Some(CoreRuntimeCapability::HashAuditLabeledFiles)
        }
        ("std.crypto.Hash", "audit_labeled_file_patterns", 3) => {
            Some(CoreRuntimeCapability::HashAuditLabeledFilePatterns)
        }
        ("std.vcs.Git", "source_tree_identity", 1) => {
            Some(CoreRuntimeCapability::GitSourceTreeIdentity)
        }
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
        ("to_string_base", 2) => Some(CorePrimitiveIntrinsic::IntToStringBase),
        ("from_string_base", 2) => Some(CorePrimitiveIntrinsic::IntFromStringBase),
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
        ("floor", 1) => Some(CorePrimitiveIntrinsic::FloatFloor),
        ("ceil", 1) => Some(CorePrimitiveIntrinsic::FloatCeil),
        ("log", 1) => Some(CorePrimitiveIntrinsic::FloatLog),
        ("pi", 0) => Some(CorePrimitiveIntrinsic::FloatPi),
        ("tau", 0) => Some(CorePrimitiveIntrinsic::FloatTau),
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
        ("reverse", 1) => Some(CorePrimitiveIntrinsic::StringReverse),
        ("characters", 1) => Some(CorePrimitiveIntrinsic::StringCharacters),
        ("codepoints", 1) => Some(CorePrimitiveIntrinsic::StringCodepoints),
        ("utf8_byte_at", 2) => Some(CorePrimitiveIntrinsic::StringUtf8ByteAt),
        ("utf8_find_any_byte", 3) => Some(CorePrimitiveIntrinsic::StringUtf8FindAnyByte),
        ("utf8_slice", 3) => Some(CorePrimitiveIntrinsic::StringUtf8Slice),
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
        ("rest", 1) => Some(CorePrimitiveIntrinsic::ListRest),
        ("concat", 2) => Some(CorePrimitiveIntrinsic::ListConcat),
        ("subtract", 2) => Some(CorePrimitiveIntrinsic::ListSubtract),
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
        ("take", 2) => Some(CorePrimitiveIntrinsic::MapTake),
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
        ("failed", 1) => Some(CorePrimitiveIntrinsic::TaskFailed),
        ("result", 1) => Some(CorePrimitiveIntrinsic::TaskResult),
        _ => None,
    }
}

/// Resolves a `std.vm.NativeBridge` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.vm.NativeBridge`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for admitted executable VM NativeBridge
///   proof operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the NativeBoundary bridge handle surface to closed CoreIR intrinsic
///   identities so the Vm backend can validate bridge plumbing before real
///   native worker transport is attached.
fn core_vm_native_bridge_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("start", 1) => Some(CorePrimitiveIntrinsic::VmNativeBridgeStart),
        ("call", 2) => Some(CorePrimitiveIntrinsic::VmNativeBridgeCall),
        ("dispose", 1) => Some(CorePrimitiveIntrinsic::VmNativeBridgeDispose),
        ("stop", 1) => Some(CorePrimitiveIntrinsic::VmNativeBridgeStop),
        _ => None,
    }
}

/// Resolves a `std.vm.Bytes` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.vm.Bytes`
///   module path.
/// - `arity`: argument count after receiver methods have been normalized to
///   receiver-first calls.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable VM byte-buffer
///   operations.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Maps the byte-buffer contract to closed CoreIR identities so protocol
///   tests can use typed buffers without exposing Vm binary syntax.
fn core_vm_bytes_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("from_list", 1) => Some(CorePrimitiveIntrinsic::VmBytesFromList),
        ("to_list", 1) => Some(CorePrimitiveIntrinsic::VmBytesToList),
        ("length", 1) => Some(CorePrimitiveIntrinsic::VmBytesLength),
        ("starts_with", 2) => Some(CorePrimitiveIntrinsic::VmBytesStartsWith),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::VmBytesContains),
        ("first_non_ascii_whitespace", 1) => {
            Some(CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace)
        }
        ("concat", 2) => Some(CorePrimitiveIntrinsic::VmBytesConcat),
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBytesSlice),
        ("read_uint_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintBe),
        ("read_int_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntBe),
        ("read_uint_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintLe),
        ("read_int_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntLe),
        _ => None,
    }
}

/// Resolves executable `std.vm.BitString` operations to closed CoreIR IDs.
fn core_vm_bitstring_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("from_bytes", 2) => Some(CorePrimitiveIntrinsic::VmBitStringFromBytes),
        ("from_uint_be", 2) => Some(CorePrimitiveIntrinsic::VmBitStringFromUintBe),
        ("from_int_be", 2) => Some(CorePrimitiveIntrinsic::VmBitStringFromIntBe),
        ("from_uint_le", 2) => Some(CorePrimitiveIntrinsic::VmBitStringFromUintLe),
        ("from_int_le", 2) => Some(CorePrimitiveIntrinsic::VmBitStringFromIntLe),
        ("utf8_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringUtf8Scalar),
        ("to_utf8_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar),
        ("utf16_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringUtf16BeScalar),
        ("utf16_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringUtf16LeScalar),
        ("to_utf16_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar),
        ("to_utf16_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar),
        ("utf32_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringUtf32BeScalar),
        ("utf32_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringUtf32LeScalar),
        ("to_utf32_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar),
        ("to_utf32_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar),
        ("bit_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringBitLength),
        ("byte_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringByteLength),
        ("is_byte_aligned", 1) => Some(CorePrimitiveIntrinsic::VmBitStringIsByteAligned),
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBitStringSlice),
        ("concat", 2) => Some(CorePrimitiveIntrinsic::VmBitStringConcat),
        ("to_bytes", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToBytes),
        ("to_uint_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintBe),
        ("to_int_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntBe),
        ("to_uint_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintLe),
        ("to_int_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntLe),
        _ => None,
    }
}

/// Resolves a `std.vm.Timeout` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.vm.Timeout`
///   module path.
/// - `arity`: source-visible argument count.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for executable timeout constructors.
/// - `None` for unsupported operations or arity mismatch.
///
/// Transformation:
/// - Keeps VM timeout representation target-owned while source tests use a
///   typed timeout value.
fn core_vm_timeout_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("milliseconds", 1) => Some(CorePrimitiveIntrinsic::VmTimeoutMilliseconds),
        ("forever", 0) => Some(CorePrimitiveIntrinsic::VmTimeoutForever),
        _ => None,
    }
}

/// Resolves a `std.vm.Tcp` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.vm.Tcp` module
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
fn core_vm_tcp_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("listen", 1) => Some(CorePrimitiveIntrinsic::VmTcpListen),
        ("listen_with_backlog", 2) => Some(CorePrimitiveIntrinsic::VmTcpListenWithBacklog),
        ("accept", 2) => Some(CorePrimitiveIntrinsic::VmTcpAccept),
        ("connect", 3) => Some(CorePrimitiveIntrinsic::VmTcpConnect),
        ("send", 2) => Some(CorePrimitiveIntrinsic::VmTcpSend),
        ("receive", 3) => Some(CorePrimitiveIntrinsic::VmTcpReceive),
        ("close", 1) => Some(CorePrimitiveIntrinsic::VmTcpClose),
        ("close_listener", 1) => Some(CorePrimitiveIntrinsic::VmTcpCloseListener),
        _ => None,
    }
}

/// Resolves a `std.vm.Port` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.vm.Port` module
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
fn core_vm_port_primitive_intrinsic(
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("open", 1) => Some(CorePrimitiveIntrinsic::VmPortOpen),
        ("write", 2) => Some(CorePrimitiveIntrinsic::VmPortWrite),
        ("read", 3) => Some(CorePrimitiveIntrinsic::VmPortRead),
        ("close", 1) => Some(CorePrimitiveIntrinsic::VmPortClose),
        _ => None,
    }
}
