use super::{CoreExpr, CoreType};
use crate::terlan_syntax::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Backend-neutral effect labels for a Core expression.
///
/// Inputs: effect names discovered during lowering. Output: effect set.
/// Transformation: stores effects as labels for deterministic validation and
/// target capability checks.
pub struct CoreEffectSet {
    pub effects: Vec<String>,
}

impl CoreEffectSet {
    /// Renders a Core effect set as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: effect labels attached to a Core expression.
    ///
    /// Output:
    /// - Stable `Effects(...)` text for CoreIR contract snapshots.
    ///
    /// Transformation:
    /// - Sorts effect labels so semantically identical effect sets produce the
    ///   same contract text regardless of construction order.
    pub(crate) fn contract_text(&self) -> String {
        let mut effects = self.effects.clone();
        effects.sort();
        format!("Effects({})", effects.join(","))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Compiler-owned primitive intrinsic identity.
///
/// Inputs: resolved intrinsic operation. Output: closed primitive enum.
/// Transformation: replaces target module calls with backend-neutral intrinsic
/// identities.
pub enum CorePrimitiveIntrinsic {
    TypeOf,
    IsType,
    BoolEqual,
    BoolCompare,
    BoolToString,
    BoolFromString,
    AtomToString,
    IntToString,
    IntFromString,
    FloatToString,
    FloatFromString,
    StringEqual,
    StringCompare,
    StringToString,
    StringFromString,
    StringIsEmpty,
    StringAppend,
    StringConcat,
    StringContains,
    StringStartsWith,
    StringEndsWith,
    StringLength,
    StringByteSize,
    StringLowercase,
    StringUppercase,
    StringTrim,
    StringTrimStart,
    StringTrimEnd,
    StringReplace,
    StringSplit,
    StringSplitOnce,
    ListNew,
    ListIsEmpty,
    ListLength,
    ListFirst,
    ListIterator,
    ListPush,
    ListClear,
    IteratorNext,
    MapNew,
    MapFromEntries,
    MapIsEmpty,
    MapSize,
    MapGet,
    MapContainsKey,
    MapIterator,
    MapPut,
    MapRemove,
    MapClear,
    SetNew,
    SetFromList,
    SetIsEmpty,
    SetSize,
    SetContains,
    SetIterator,
    SetAdd,
    SetRemove,
    SetClear,
    TaskDone,
    TaskResult,
    BeamAgentStart,
    BeamAgentGet,
    BeamAgentGetAndUpdate,
    BeamAgentUpdate,
    BeamAgentCast,
    BeamAgentStop,
    BeamGenServerStart,
    BeamGenServerCall,
    BeamGenServerCast,
    BeamGenServerStop,
    BeamNativeBridgeStart,
    BeamNativeBridgeCall,
    BeamNativeBridgeDispose,
    BeamNativeBridgeStop,
    BeamBytesFromList,
    BeamBytesToList,
    BeamBytesLength,
    BeamBytesConcat,
    BeamTimeoutMilliseconds,
    BeamTimeoutForever,
    BeamTcpConnect,
    BeamTcpSend,
    BeamTcpReceive,
    BeamTcpClose,
    BeamPortOpen,
    BeamPortWrite,
    BeamPortRead,
    BeamPortClose,
    BeamSupervisorStartRoot,
    BeamSupervisorChildSpec,
    BeamSupervisorStart,
    BeamSupervisorStop,
    BeamTaskStart,
    BeamTaskResult,
    BeamTaskCancel,
}

impl CorePrimitiveIntrinsic {
    /// Returns the stable registry key for a primitive intrinsic.
    ///
    /// Inputs:
    /// - `self`: compiler-owned primitive intrinsic identity.
    ///
    /// Output:
    /// - Stable `core.<primitive>.<operation>` key from the CoreIR primitive
    ///   intrinsic registry.
    ///
    /// Transformation:
    /// - Maps the closed Rust enum variant to the backend-neutral serialized
    ///   intrinsic key used by contract text and backend lowering.
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::TypeOf => "core.type.type_of",
            Self::IsType => "core.type.is_type",
            Self::BoolEqual => "core.bool.equal",
            Self::BoolCompare => "core.bool.compare",
            Self::BoolToString => "core.bool.to_string",
            Self::BoolFromString => "core.bool.from_string",
            Self::AtomToString => "core.atom.to_string",
            Self::IntToString => "core.int.to_string",
            Self::IntFromString => "core.int.from_string",
            Self::FloatToString => "core.float.to_string",
            Self::FloatFromString => "core.float.from_string",
            Self::StringEqual => "core.string.equal",
            Self::StringCompare => "core.string.compare",
            Self::StringToString => "core.string.to_string",
            Self::StringFromString => "core.string.from_string",
            Self::StringIsEmpty => "core.string.is_empty",
            Self::StringAppend => "core.string.append",
            Self::StringConcat => "core.string.concat",
            Self::StringContains => "core.string.contains",
            Self::StringStartsWith => "core.string.starts_with",
            Self::StringEndsWith => "core.string.ends_with",
            Self::StringLength => "core.string.length",
            Self::StringByteSize => "core.string.byte_size",
            Self::StringLowercase => "core.string.lowercase",
            Self::StringUppercase => "core.string.uppercase",
            Self::StringTrim => "core.string.trim",
            Self::StringTrimStart => "core.string.trim_start",
            Self::StringTrimEnd => "core.string.trim_end",
            Self::StringReplace => "core.string.replace",
            Self::StringSplit => "core.string.split",
            Self::StringSplitOnce => "core.string.split_once",
            Self::ListNew => "core.list.new",
            Self::ListIsEmpty => "core.list.is_empty",
            Self::ListLength => "core.list.length",
            Self::ListFirst => "core.list.first",
            Self::ListIterator => "core.list.iterator",
            Self::ListPush => "core.list.push",
            Self::ListClear => "core.list.clear",
            Self::IteratorNext => "core.iterator.next",
            Self::MapNew => "core.map.new",
            Self::MapFromEntries => "core.map.from_entries",
            Self::MapIsEmpty => "core.map.is_empty",
            Self::MapSize => "core.map.size",
            Self::MapGet => "core.map.get",
            Self::MapContainsKey => "core.map.contains_key",
            Self::MapIterator => "core.map.iterator",
            Self::MapPut => "core.map.put",
            Self::MapRemove => "core.map.remove",
            Self::MapClear => "core.map.clear",
            Self::SetNew => "core.set.new",
            Self::SetFromList => "core.set.from_list",
            Self::SetIsEmpty => "core.set.is_empty",
            Self::SetSize => "core.set.size",
            Self::SetContains => "core.set.contains",
            Self::SetIterator => "core.set.iterator",
            Self::SetAdd => "core.set.add",
            Self::SetRemove => "core.set.remove",
            Self::SetClear => "core.set.clear",
            Self::TaskDone => "core.task.done",
            Self::TaskResult => "core.task.result",
            Self::BeamAgentStart => "beam.agent.start",
            Self::BeamAgentGet => "beam.agent.get",
            Self::BeamAgentGetAndUpdate => "beam.agent.get_and_update",
            Self::BeamAgentUpdate => "beam.agent.update",
            Self::BeamAgentCast => "beam.agent.cast",
            Self::BeamAgentStop => "beam.agent.stop",
            Self::BeamGenServerStart => "beam.gen_server.start",
            Self::BeamGenServerCall => "beam.gen_server.call",
            Self::BeamGenServerCast => "beam.gen_server.cast",
            Self::BeamGenServerStop => "beam.gen_server.stop",
            Self::BeamNativeBridgeStart => "beam.native_bridge.start",
            Self::BeamNativeBridgeCall => "beam.native_bridge.call",
            Self::BeamNativeBridgeDispose => "beam.native_bridge.dispose",
            Self::BeamNativeBridgeStop => "beam.native_bridge.stop",
            Self::BeamBytesFromList => "beam.bytes.from_list",
            Self::BeamBytesToList => "beam.bytes.to_list",
            Self::BeamBytesLength => "beam.bytes.length",
            Self::BeamBytesConcat => "beam.bytes.concat",
            Self::BeamTimeoutMilliseconds => "beam.timeout.milliseconds",
            Self::BeamTimeoutForever => "beam.timeout.forever",
            Self::BeamTcpConnect => "beam.tcp.connect",
            Self::BeamTcpSend => "beam.tcp.send",
            Self::BeamTcpReceive => "beam.tcp.receive",
            Self::BeamTcpClose => "beam.tcp.close",
            Self::BeamPortOpen => "beam.port.open",
            Self::BeamPortWrite => "beam.port.write",
            Self::BeamPortRead => "beam.port.read",
            Self::BeamPortClose => "beam.port.close",
            Self::BeamSupervisorStartRoot => "beam.supervisor.start_root",
            Self::BeamSupervisorChildSpec => "beam.supervisor.child_spec",
            Self::BeamSupervisorStart => "beam.supervisor.start",
            Self::BeamSupervisorStop => "beam.supervisor.stop",
            Self::BeamTaskStart => "beam.task.start",
            Self::BeamTaskResult => "beam.task.result",
            Self::BeamTaskCancel => "beam.task.cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Runtime capability intrinsic identity.
///
/// Inputs: resolved runtime operation. Output: closed capability enum.
/// Transformation: records portable runtime requirements without selecting a
/// backend implementation.
pub enum CoreRuntimeCapability {
    ConsolePrintln,
    FileExists,
    FileReadText,
    FileWriteText,
    FileAppendText,
    FileDelete,
}

impl CoreRuntimeCapability {
    /// Returns the stable registry key for a runtime capability.
    ///
    /// Inputs:
    /// - `self`: compiler-owned runtime capability identity.
    ///
    /// Output:
    /// - Stable `runtime.<domain>.<operation>` key used by CoreIR contract
    ///   text and backend lowering.
    ///
    /// Transformation:
    /// - Maps the closed runtime capability enum to the backend-neutral
    ///   serialized key without exposing target modules in CoreIR.
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::ConsolePrintln => "runtime.console.println",
            Self::FileExists => "runtime.file.exists",
            Self::FileReadText => "runtime.file.read_text",
            Self::FileWriteText => "runtime.file.write_text",
            Self::FileAppendText => "runtime.file.append_text",
            Self::FileDelete => "runtime.file.delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Closed Core intrinsic identity.
///
/// Inputs: primitive or runtime intrinsic classification. Output: namespaced
/// intrinsic identity. Transformation: keeps both intrinsic families behind one
/// expression node shape.
pub enum CoreIntrinsicId {
    Primitive(CorePrimitiveIntrinsic),
    Runtime(CoreRuntimeCapability),
}

impl CoreIntrinsicId {
    /// Returns the stable registry key for a Core intrinsic identity.
    ///
    /// Inputs:
    /// - `self`: closed Core intrinsic identity.
    ///
    /// Output:
    /// - Stable registry key for deterministic CoreIR contract text.
    ///
    /// Transformation:
    /// - Delegates to the namespace-specific intrinsic identity while keeping
    ///   backend-specific names out of CoreIR.
    fn registry_key(&self) -> &'static str {
        match self {
            Self::Primitive(intrinsic) => intrinsic.registry_key(),
            Self::Runtime(capability) => capability.registry_key(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core intrinsic call expression payload.
///
/// Inputs: intrinsic identity, typed args, return type, effects, and span.
/// Output: backend-neutral intrinsic call. Transformation: carries enough data
/// for target lowering and proof contracts without exposing source call syntax.
pub struct CoreIntrinsicCall {
    pub id: CoreIntrinsicId,
    pub args: Vec<CoreExpr>,
    pub return_type: CoreType,
    pub effects: CoreEffectSet,
    pub span: Span,
}

impl CoreIntrinsicCall {
    /// Renders a Core intrinsic call as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed intrinsic call payload.
    ///
    /// Output:
    /// - Stable `Intrinsic(...)` text for CoreIR contract snapshots.
    ///
    /// Transformation:
    /// - Serializes the backend-neutral intrinsic key, typed arguments,
    ///   return type, effects, and source span without exposing backend module
    ///   calls.
    pub(crate) fn contract_text(&self) -> String {
        let args = self
            .args
            .iter()
            .map(CoreExpr::contract_text)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "Intrinsic({};args={};return={};effects={};span={}:{}))",
            self.id.registry_key(),
            args,
            self.return_type.contract_text(),
            self.effects.contract_text(),
            self.span.start,
            self.span.end
        )
    }
}
