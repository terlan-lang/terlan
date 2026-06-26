use terlan_hir::module_path_to_safe_native_module;
use terlan_syntax::syntax_output::{SyntaxAnnotationOutput, SyntaxDeclarationPayload};
use terlan_syntax::{SyntaxDeclarationOutput, SyntaxParamOutput};
use terlan_typeck::{
    core_primitive_intrinsic_return_type, CoreEffectSet, CoreExpr, CoreIntrinsicCall,
    CoreIntrinsicId, CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreType,
};

use super::super::erl::ErlExpr;
use super::super::sanitize_erlang_var;
use super::lower_core_intrinsic_call_to_erlang;

/// Lowers an annotated syntax declaration body through the CoreIR intrinsic backend.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying compiler annotations.
/// - `params`: ordered declaration parameters used as intrinsic arguments.
/// - `module_name`: canonical source module that owns the declaration.
///
/// Output:
/// - `Some(ErlExpr)` when the declaration has a supported compiler intrinsic.
/// - `None` when the declaration is not an intrinsic-backed std surface.
///
/// Transformation:
/// - Preserves source parameter order, derives the intrinsic identity, builds a
///   typed CoreIR intrinsic call, and delegates target lowering to the CoreIR
///   Erlang backend.
pub(in crate::emit) fn lower_intrinsic_annotation_body(
    decl: &SyntaxDeclarationOutput,
    params: &[SyntaxParamOutput],
    module_name: &str,
) -> Option<ErlExpr> {
    lower_intrinsic_annotation_body_for_names(
        decl,
        params.iter().map(|param| param.name.as_str()),
        module_name,
    )
}

/// Builds an Erlang body from a compiler intrinsic annotation and argument names.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying annotations.
/// - `arg_names`: ordered source argument names passed to the intrinsic.
/// - `module_name`: canonical source module that owns `decl`.
///
/// Output:
/// - `Some(ErlExpr)` when the declaration has a supported intrinsic annotation
///   and all arguments can be represented as Core variables.
/// - `None` when no supported annotation is present.
///
/// Transformation:
/// - Converts source argument names into CoreIR variables, derives or reads the
///   intrinsic key, builds a typed CoreIR intrinsic call, and delegates final
///   lowering to the CoreIR Erlang intrinsic backend.
pub(in crate::emit) fn lower_intrinsic_annotation_body_for_names<'a>(
    decl: &SyntaxDeclarationOutput,
    arg_names: impl Iterator<Item = &'a str>,
    module_name: &str,
) -> Option<ErlExpr> {
    let arg_names = arg_names.collect::<Vec<_>>();
    if let Some(expr) = lower_compiler_native_annotation_body(decl, &arg_names, module_name) {
        return Some(expr);
    }

    let id = decl
        .annotations
        .iter()
        .find_map(|annotation| core_intrinsic_id_from_annotation(decl, module_name, annotation))?;
    let args = arg_names
        .iter()
        .map(|name| CoreExpr::Var(name.to_string()))
        .collect::<Vec<_>>();
    let call = CoreIntrinsicCall {
        return_type: core_intrinsic_return_type(&id),
        effects: core_intrinsic_effect_set(&id),
        id,
        args,
        span: decl.span.into(),
    };
    lower_core_intrinsic_call_to_erlang(&call)
}

/// Lowers a generic `@compiler.native {operation}` declaration to its
/// package-local SafeNative Erlang module.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying annotations.
/// - `arg_names`: backend argument names in declaration order.
/// - `module_name`: canonical source module that owns the declaration.
///
/// Output:
/// - `Some(ErlExpr)` for a compiler-native operation annotation.
/// - `None` when the declaration does not use `@compiler.native`.
///
/// Transformation:
/// - Maps `polars.DataFrame.read_csv` style source declarations to
///   `polars_data_frame_safe_native:read_csv(...)`. The generated SafeNative
///   module owns not-loaded behavior until a concrete native adapter transport
///   is attached.
fn lower_compiler_native_annotation_body(
    decl: &SyntaxDeclarationOutput,
    arg_names: &[&str],
    module_name: &str,
) -> Option<ErlExpr> {
    let function = declaration_function_name(decl)?;
    decl.annotations
        .iter()
        .find_map(|annotation| compiler_native_operation(annotation))
        .map(|_operation| ErlExpr::Call {
            module: Some(module_path_to_safe_native_module(module_name)),
            function: function.to_string(),
            args: arg_names
                .iter()
                .map(|name| ErlExpr::Var(sanitize_erlang_var(name)))
                .collect(),
        })
}

/// Extracts the operation id from a compiler-native annotation.
///
/// Inputs:
/// - `annotation`: syntax-output annotation metadata.
///
/// Output:
/// - Operation id text for `@compiler.native {operation}`.
/// - `None` for other annotations.
///
/// Transformation:
/// - Reuses the annotation metadata normalization shape used by explicit
///   compiler-intrinsic annotations.
fn compiler_native_operation(annotation: &SyntaxAnnotationOutput) -> Option<&str> {
    if annotation.path != ["compiler", "native"] {
        return None;
    }
    normalized_intrinsic_annotation_key(annotation.args.as_deref()?)
}

/// Returns the backend function name for function and method declarations.
///
/// Inputs:
/// - `decl`: syntax-output declaration.
///
/// Output:
/// - Source function/method name, or `None` for non-callable declarations.
///
/// Transformation:
/// - Keeps generic compiler-native lowering independent from specific package
///   operation ids while preserving the declaration's existing Erlang ABI.
fn declaration_function_name(decl: &SyntaxDeclarationOutput) -> Option<&str> {
    match &decl.payload {
        SyntaxDeclarationPayload::Function { name, .. }
        | SyntaxDeclarationPayload::Method { name, .. } => Some(name),
        _ => None,
    }
}

/// Parses a supported CoreIR intrinsic key from an annotation.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying the annotation.
/// - `module_name`: canonical source module that owns `decl`.
/// - `annotation`: syntax-output annotation metadata.
///
/// Output:
/// - `Some(CoreIntrinsicId)` for supported `@compiler.intrinsic` keys.
/// - `None` for unrelated annotations or unsupported intrinsic keys.
///
/// Transformation:
/// - Requires the annotation path `compiler.intrinsic`, then either reads an
///   explicit escape-hatch key from metadata or derives the key from source
///   module and declaration identity.
fn core_intrinsic_id_from_annotation(
    decl: &SyntaxDeclarationOutput,
    module_name: &str,
    annotation: &SyntaxAnnotationOutput,
) -> Option<CoreIntrinsicId> {
    if annotation.path != ["compiler", "intrinsic"] {
        return None;
    }
    let key = match annotation.args.as_deref() {
        Some(args) => normalized_intrinsic_annotation_key(args)?.to_string(),
        None => derived_intrinsic_annotation_key(module_name, decl)?,
    };
    core_intrinsic_id_from_key(&key)
}

/// Derives a CoreIR intrinsic key from an annotated declaration.
///
/// Inputs:
/// - `module_name`: canonical source module that owns the declaration.
/// - `decl`: syntax-output declaration marked with `@compiler.intrinsic`.
///
/// Output:
/// - Stable intrinsic key when the module is a compiler-owned std surface.
/// - `None` when the declaration shape or module is not intrinsic-owned.
///
/// Transformation:
/// - Maps source-owned module/function identity onto the internal registry key
///   so std source does not repeat `core.string.trim` style metadata.
fn derived_intrinsic_annotation_key(
    module_name: &str,
    decl: &SyntaxDeclarationOutput,
) -> Option<String> {
    let function = match &decl.payload {
        SyntaxDeclarationPayload::Function { name, .. }
        | SyntaxDeclarationPayload::Method { name, .. } => name.as_str(),
        _ => return None,
    };
    let prefix = intrinsic_prefix_for_module(module_name)?;
    Some(format!("{prefix}.{function}"))
}

/// Resolves compiler-owned std modules to intrinsic key prefixes.
///
/// Inputs:
/// - `module_name`: canonical source module name.
///
/// Output:
/// - Intrinsic registry prefix for supported primitive/runtime std modules.
///
/// Transformation:
/// - Keeps source package names separate from CoreIR intrinsic namespaces while
///   preserving a predictable one-to-one mapping for release std declarations.
fn intrinsic_prefix_for_module(module_name: &str) -> Option<&'static str> {
    match module_name {
        "std.core.Int" => Some("core.int"),
        "std.core.Float" => Some("core.float"),
        "std.core.Atom" => Some("core.atom"),
        "std.core.String" => Some("core.string"),
        "std.collections.List" => Some("core.list"),
        "std.collections.Iterator" => Some("core.iterator"),
        "std.collections.Map" => Some("core.map"),
        "std.collections.Set" => Some("core.set"),
        "std.core.Task" => Some("core.task"),
        "std.beam.Agent" => Some("beam.agent"),
        "std.beam.NativeBridge" => Some("beam.native_bridge"),
        "std.beam.Supervisor" => Some("beam.supervisor"),
        "std.io.Console" => Some("runtime.console"),
        "std.io.File" => Some("runtime.file"),
        _ => None,
    }
}

/// Normalizes preserved annotation metadata into an intrinsic key.
///
/// Inputs:
/// - `args`: raw annotation metadata text preserved by the parser.
///
/// Output:
/// - Trimmed intrinsic key text without the outer metadata braces.
///
/// Transformation:
/// - Accepts the parser's current `{...}` preservation shape for explicit
///   escape-hatch annotations and trims surrounding whitespace.
fn normalized_intrinsic_annotation_key(args: &str) -> Option<&str> {
    let args = args.trim();
    args.strip_prefix('{')?.strip_suffix('}').map(str::trim)
}

/// Maps a stable CoreIR intrinsic key into the compiler id.
///
/// Inputs:
/// - `key`: intrinsic registry key, such as `core.string.contains`.
///
/// Output:
/// - `Some(CoreIntrinsicId)` for the currently supported intrinsic set.
/// - `None` for unknown keys not handled by this backend.
///
/// Transformation:
/// - Converts documented registry strings into the closed Rust id consumed by
///   CoreIR and backend lowering.
fn core_intrinsic_id_from_key(key: &str) -> Option<CoreIntrinsicId> {
    match key {
        "core.int.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntToString,
        )),
        "core.int.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntFromString,
        )),
        "core.float.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatToString,
        )),
        "core.float.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatFromString,
        )),
        "core.atom.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::AtomToString,
        )),
        "core.string.equal" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringEqual,
        )),
        "core.string.compare" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringCompare,
        )),
        "core.string.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringToString,
        )),
        "core.string.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringFromString,
        )),
        "core.string.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringIsEmpty,
        )),
        "core.string.append" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringAppend,
        )),
        "core.string.concat" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringConcat,
        )),
        "core.string.contains" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringContains,
        )),
        "core.string.starts_with" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringStartsWith,
        )),
        "core.string.ends_with" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringEndsWith,
        )),
        "core.string.length" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringLength,
        )),
        "core.string.byte_size" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringByteSize,
        )),
        "core.string.lowercase" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringLowercase,
        )),
        "core.string.uppercase" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringUppercase,
        )),
        "core.string.trim" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrim,
        )),
        "core.string.trim_start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrimStart,
        )),
        "core.string.trim_end" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrimEnd,
        )),
        "core.string.replace" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringReplace,
        )),
        "core.string.split" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringSplit,
        )),
        "core.string.split_once" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringSplitOnce,
        )),
        "core.list.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)),
        "core.list.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListIsEmpty,
        )),
        "core.list.length" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListLength,
        )),
        "core.list.first" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListFirst,
        )),
        "core.list.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListIterator,
        )),
        "core.list.push" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush)),
        "core.list.clear" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListClear,
        )),
        "core.iterator.next" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IteratorNext,
        )),
        "core.map.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew)),
        "core.map.from_entries" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapFromEntries,
        )),
        "core.map.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapIsEmpty,
        )),
        "core.map.size" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapSize)),
        "core.map.get" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet)),
        "core.map.contains_key" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapContainsKey,
        )),
        "core.map.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapIterator,
        )),
        "core.map.put" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapPut)),
        "core.map.remove" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapRemove,
        )),
        "core.map.clear" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapClear)),
        "core.set.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew)),
        "core.set.from_list" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetFromList,
        )),
        "core.set.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetIsEmpty,
        )),
        "core.set.size" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize)),
        "core.set.contains" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetContains,
        )),
        "core.set.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetIterator,
        )),
        "core.set.add" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd)),
        "core.set.remove" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetRemove,
        )),
        "core.set.clear" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetClear)),
        "core.task.done" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskDone)),
        "core.task.result" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::TaskResult,
        )),
        "beam.agent.start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentStart,
        )),
        "beam.agent.get" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentGet,
        )),
        "beam.agent.get_and_update" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentGetAndUpdate,
        )),
        "beam.agent.update" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentUpdate,
        )),
        "beam.agent.cast" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentCast,
        )),
        "beam.agent.stop" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamAgentStop,
        )),
        "beam.native_bridge.start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamNativeBridgeStart,
        )),
        "beam.native_bridge.call" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamNativeBridgeCall,
        )),
        "beam.native_bridge.dispose" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamNativeBridgeDispose,
        )),
        "beam.native_bridge.stop" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamNativeBridgeStop,
        )),
        "beam.supervisor.child_spec" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamSupervisorChildSpec,
        )),
        "beam.supervisor.start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamSupervisorStart,
        )),
        "beam.supervisor.stop" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamSupervisorStop,
        )),
        "beam.task.start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamTaskStart,
        )),
        "beam.task.result" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamTaskResult,
        )),
        "beam.task.cancel" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::BeamTaskCancel,
        )),
        "runtime.console.println" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::ConsolePrintln,
        )),
        "runtime.file.exists" => Some(CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists)),
        "runtime.file.read_text" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileReadText,
        )),
        "runtime.file.write_text" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileWriteText,
        )),
        "runtime.file.append_text" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileAppendText,
        )),
        "runtime.file.delete" => Some(CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileDelete)),
        _ => None,
    }
}

/// Returns the CoreIR return type for an intrinsic id.
///
/// Inputs:
/// - `id`: compiler-owned intrinsic identity.
///
/// Output:
/// - Backend-neutral CoreIR return type for the intrinsic or runtime
///   capability.
///
/// Transformation:
/// - Mirrors the documented intrinsic and runtime capability registries so
///   annotation-driven backend emission can construct a typed CoreIR intrinsic
///   call without re-reading source function signatures.
fn core_intrinsic_return_type(id: &CoreIntrinsicId) -> CoreType {
    match id {
        CoreIntrinsicId::Primitive(intrinsic) => core_primitive_intrinsic_return_type(intrinsic),
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln) => {
            CoreType::Named("Unit".to_string())
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => CoreType::Bool,
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileWriteText
            | CoreRuntimeCapability::FileAppendText
            | CoreRuntimeCapability::FileDelete,
        ) => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
    }
}

/// Returns the CoreIR effect set for an intrinsic id.
///
/// Inputs:
/// - `id`: compiler-owned intrinsic identity.
///
/// Output:
/// - Backend-neutral effect set attached to the intrinsic call.
///
/// Transformation:
/// - Marks primitive intrinsics as pure and runtime console output as `io` so
///   downstream CoreIR consumers can distinguish value computations from
///   observable effects.
fn core_intrinsic_effect_set(id: &CoreIntrinsicId) -> CoreEffectSet {
    match id {
        CoreIntrinsicId::Primitive(_) => CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::ConsolePrintln
            | CoreRuntimeCapability::FileExists
            | CoreRuntimeCapability::FileReadText
            | CoreRuntimeCapability::FileWriteText
            | CoreRuntimeCapability::FileAppendText
            | CoreRuntimeCapability::FileDelete,
        ) => CoreEffectSet {
            effects: vec!["io".to_string()],
        },
    }
}
