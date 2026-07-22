use super::{CoreExpr, CoreType};
use crate::terlan_syntax::span::Span;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    ValueToString,
    IntToString,
    IntFromString,
    IntToStringBase,
    IntFromStringBase,
    FloatToString,
    FloatFromString,
    FloatFloor,
    FloatCeil,
    FloatLog,
    FloatPi,
    FloatTau,
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
    StringReverse,
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
    ListRest,
    ListConcat,
    ListSubtract,
    ListIterator,
    ListPush,
    ListClear,
    IteratorNext,
    MapNew,
    MapFromEntries,
    MapIsEmpty,
    MapSize,
    MapGet,
    MapTake,
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
    TaskFailed,
    TaskResult,
    VmEffectRun,
    VmProcessYield,
    VmProcessSendInt,
    VmProcessReceiveInt,
    VmProcessSendString,
    VmProcessReceiveString,
    VmProcessSendBytes,
    VmProcessReceiveBytes,
    VmProcessSendBinary,
    VmProcessReceiveBinary,
    VmProcessSendAtom,
    VmProcessReceiveAtom,
    VmProcessSleep,
    VmProcessFail,
    VmProcessSchedule,
    VmAgentStart,
    VmAgentGet,
    VmAgentGetAndUpdate,
    VmAgentUpdate,
    VmAgentCast,
    VmAgentStop,
    VmGenServerStart,
    VmGenServerCall,
    VmGenServerCast,
    VmGenServerStop,
    VmNativeBridgeStart,
    VmNativeBridgeCall,
    VmNativeBridgeDispose,
    VmNativeBridgeStop,
    VmBytesFromList,
    VmBytesToList,
    VmBytesLength,
    VmBytesConcat,
    VmBytesSlice,
    VmBytesReadUintBe,
    VmBytesReadIntBe,
    VmBytesReadUintLe,
    VmBytesReadIntLe,
    VmBitStringFromBytes,
    VmBitStringFromAllBytes,
    VmBitStringFromExactBytes,
    VmBitStringRequireExactBits,
    VmBitStringFromUintBe,
    VmBitStringFromIntBe,
    VmBitStringFromUintLe,
    VmBitStringFromIntLe,
    VmBitStringUtf8Scalar,
    VmBitStringToUtf8Scalar,
    VmBitStringUtf16BeScalar,
    VmBitStringUtf16LeScalar,
    VmBitStringToUtf16BeScalar,
    VmBitStringToUtf16LeScalar,
    VmBitStringUtf32BeScalar,
    VmBitStringUtf32LeScalar,
    VmBitStringToUtf32BeScalar,
    VmBitStringToUtf32LeScalar,
    VmBitStringBitLength,
    VmBitStringByteLength,
    VmBitStringIsByteAligned,
    VmBitStringSlice,
    VmBitStringConcat,
    VmBitStringToBytes,
    VmBitStringToUintBe,
    VmBitStringToIntBe,
    VmBitStringToUintLe,
    VmBitStringToIntLe,
    VmTimeoutMilliseconds,
    VmTimeoutForever,
    VmTcpListen,
    VmTcpListenWithBacklog,
    VmTcpAccept,
    VmTcpConnect,
    VmTcpSend,
    VmTcpReceive,
    VmTcpClose,
    VmTcpCloseListener,
    VmPortOpen,
    VmPortWrite,
    VmPortRead,
    VmPortClose,
    VmSupervisorStartRoot,
    VmSupervisorChildSpec,
    VmSupervisorStart,
    VmSupervisorStop,
    VmTaskStart,
    VmTaskResult,
    VmTaskCancel,
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
            Self::ValueToString => "core.value.to_string",
            Self::IntToString => "core.int.to_string",
            Self::IntFromString => "core.int.from_string",
            Self::IntToStringBase => "core.int.to_string_base",
            Self::IntFromStringBase => "core.int.from_string_base",
            Self::FloatToString => "core.float.to_string",
            Self::FloatFromString => "core.float.from_string",
            Self::FloatFloor => "core.float.floor",
            Self::FloatCeil => "core.float.ceil",
            Self::FloatLog => "core.float.log",
            Self::FloatPi => "core.float.pi",
            Self::FloatTau => "core.float.tau",
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
            Self::StringReverse => "core.string.reverse",
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
            Self::ListRest => "core.list.rest",
            Self::ListConcat => "core.list.concat",
            Self::ListSubtract => "core.list.subtract",
            Self::ListIterator => "core.list.iterator",
            Self::ListPush => "core.list.push",
            Self::ListClear => "core.list.clear",
            Self::IteratorNext => "core.iterator.next",
            Self::MapNew => "core.map.new",
            Self::MapFromEntries => "core.map.from_entries",
            Self::MapIsEmpty => "core.map.is_empty",
            Self::MapSize => "core.map.size",
            Self::MapGet => "core.map.get",
            Self::MapTake => "core.map.take",
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
            Self::TaskFailed => "core.task.failed",
            Self::TaskResult => "core.task.result",
            Self::VmEffectRun => "vm.effect.run",
            Self::VmProcessYield => "vm.process.yield_now",
            Self::VmProcessSendInt => "vm.process.send_int",
            Self::VmProcessReceiveInt => "vm.process.receive_int",
            Self::VmProcessSendString => "vm.process.send_string",
            Self::VmProcessReceiveString => "vm.process.receive_string",
            Self::VmProcessSendBytes => "vm.process.send_bytes",
            Self::VmProcessReceiveBytes => "vm.process.receive_bytes",
            Self::VmProcessSendBinary => "vm.process.send_binary",
            Self::VmProcessReceiveBinary => "vm.process.receive_binary",
            Self::VmProcessSendAtom => "vm.process.send_atom",
            Self::VmProcessReceiveAtom => "vm.process.receive_atom",
            Self::VmProcessSleep => "vm.process.sleep",
            Self::VmProcessFail => "vm.process.fail",
            Self::VmProcessSchedule => "vm.process.schedule",
            Self::VmAgentStart => "vm.agent.start",
            Self::VmAgentGet => "vm.agent.get",
            Self::VmAgentGetAndUpdate => "vm.agent.get_and_update",
            Self::VmAgentUpdate => "vm.agent.update",
            Self::VmAgentCast => "vm.agent.cast",
            Self::VmAgentStop => "vm.agent.stop",
            Self::VmGenServerStart => "vm.gen_server.start",
            Self::VmGenServerCall => "vm.gen_server.call",
            Self::VmGenServerCast => "vm.gen_server.cast",
            Self::VmGenServerStop => "vm.gen_server.stop",
            Self::VmNativeBridgeStart => "vm.native_bridge.start",
            Self::VmNativeBridgeCall => "vm.native_bridge.call",
            Self::VmNativeBridgeDispose => "vm.native_bridge.dispose",
            Self::VmNativeBridgeStop => "vm.native_bridge.stop",
            Self::VmBytesFromList => "vm.bytes.from_list",
            Self::VmBytesToList => "vm.bytes.to_list",
            Self::VmBytesLength => "vm.bytes.length",
            Self::VmBytesConcat => "vm.bytes.concat",
            Self::VmBytesSlice => "vm.bytes.slice",
            Self::VmBytesReadUintBe => "vm.bytes.read_uint_be",
            Self::VmBytesReadIntBe => "vm.bytes.read_int_be",
            Self::VmBytesReadUintLe => "vm.bytes.read_uint_le",
            Self::VmBytesReadIntLe => "vm.bytes.read_int_le",
            Self::VmBitStringFromBytes => "vm.bitstring.from_bytes",
            Self::VmBitStringFromAllBytes => "vm.bitstring.from_all_bytes",
            Self::VmBitStringFromExactBytes => "vm.bitstring.from_exact_bytes",
            Self::VmBitStringRequireExactBits => "vm.bitstring.require_exact_bits",
            Self::VmBitStringFromUintBe => "vm.bitstring.from_uint_be",
            Self::VmBitStringFromIntBe => "vm.bitstring.from_int_be",
            Self::VmBitStringFromUintLe => "vm.bitstring.from_uint_le",
            Self::VmBitStringFromIntLe => "vm.bitstring.from_int_le",
            Self::VmBitStringUtf8Scalar => "vm.bitstring.utf8_scalar",
            Self::VmBitStringToUtf8Scalar => "vm.bitstring.to_utf8_scalar",
            Self::VmBitStringUtf16BeScalar => "vm.bitstring.utf16_be_scalar",
            Self::VmBitStringUtf16LeScalar => "vm.bitstring.utf16_le_scalar",
            Self::VmBitStringToUtf16BeScalar => "vm.bitstring.to_utf16_be_scalar",
            Self::VmBitStringToUtf16LeScalar => "vm.bitstring.to_utf16_le_scalar",
            Self::VmBitStringUtf32BeScalar => "vm.bitstring.utf32_be_scalar",
            Self::VmBitStringUtf32LeScalar => "vm.bitstring.utf32_le_scalar",
            Self::VmBitStringToUtf32BeScalar => "vm.bitstring.to_utf32_be_scalar",
            Self::VmBitStringToUtf32LeScalar => "vm.bitstring.to_utf32_le_scalar",
            Self::VmBitStringBitLength => "vm.bitstring.bit_length",
            Self::VmBitStringByteLength => "vm.bitstring.byte_length",
            Self::VmBitStringIsByteAligned => "vm.bitstring.is_byte_aligned",
            Self::VmBitStringSlice => "vm.bitstring.slice",
            Self::VmBitStringConcat => "vm.bitstring.concat",
            Self::VmBitStringToBytes => "vm.bitstring.to_bytes",
            Self::VmBitStringToUintBe => "vm.bitstring.to_uint_be",
            Self::VmBitStringToIntBe => "vm.bitstring.to_int_be",
            Self::VmBitStringToUintLe => "vm.bitstring.to_uint_le",
            Self::VmBitStringToIntLe => "vm.bitstring.to_int_le",
            Self::VmTimeoutMilliseconds => "vm.timeout.milliseconds",
            Self::VmTimeoutForever => "vm.timeout.forever",
            Self::VmTcpListen => "vm.tcp.listen",
            Self::VmTcpListenWithBacklog => "vm.tcp.listen_with_backlog",
            Self::VmTcpAccept => "vm.tcp.accept",
            Self::VmTcpConnect => "vm.tcp.connect",
            Self::VmTcpSend => "vm.tcp.send",
            Self::VmTcpReceive => "vm.tcp.receive",
            Self::VmTcpClose => "vm.tcp.close",
            Self::VmTcpCloseListener => "vm.tcp.close_listener",
            Self::VmPortOpen => "vm.port.open",
            Self::VmPortWrite => "vm.port.write",
            Self::VmPortRead => "vm.port.read",
            Self::VmPortClose => "vm.port.close",
            Self::VmSupervisorStartRoot => "vm.supervisor.start_root",
            Self::VmSupervisorChildSpec => "vm.supervisor.child_spec",
            Self::VmSupervisorStart => "vm.supervisor.start",
            Self::VmSupervisorStop => "vm.supervisor.stop",
            Self::VmTaskStart => "vm.task.start",
            Self::VmTaskResult => "vm.task.result",
            Self::VmTaskCancel => "vm.task.cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Closed Core intrinsic identity.
///
/// Inputs: primitive or runtime intrinsic classification. Output: namespaced
/// intrinsic identity. Transformation: keeps both intrinsic families behind one
/// expression node shape.
pub enum CoreIntrinsicId {
    Primitive(CorePrimitiveIntrinsic),
    Runtime(CoreRuntimeCapability),
    /// Typed public process send retaining its concrete message payload type.
    VmProcessSendMessage(CoreType),
    /// Typed public process receive retaining its concrete message payload type.
    VmProcessReceiveMessage(CoreType),
    /// Typed process spawn retaining the child mailbox payload type.
    VmProcessSpawn(CoreType),
    /// Typed process link retaining the peer mailbox payload type.
    VmProcessLink(CoreType),
    /// Typed process monitor retaining the target mailbox payload type.
    VmProcessMonitor(CoreType),
    /// Typed resource acquisition retaining the resource family type.
    VmProcessAcquireResource(CoreType),
    /// Typed process cancellation retaining the target mailbox payload type.
    VmProcessCancel(CoreType),
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
    fn registry_key(&self) -> String {
        match self {
            Self::Primitive(intrinsic) => intrinsic.registry_key().to_string(),
            Self::Runtime(capability) => capability.registry_key().to_string(),
            Self::VmProcessSendMessage(value_type) => {
                format!("vm.process.send[{}]", value_type.contract_text())
            }
            Self::VmProcessReceiveMessage(value_type) => {
                format!("vm.process.receive[{}]", value_type.contract_text())
            }
            Self::VmProcessSpawn(value_type) => {
                format!("vm.process.spawn[{}]", value_type.contract_text())
            }
            Self::VmProcessLink(value_type) => {
                format!("vm.process.link[{}]", value_type.contract_text())
            }
            Self::VmProcessMonitor(value_type) => {
                format!("vm.process.monitor[{}]", value_type.contract_text())
            }
            Self::VmProcessAcquireResource(value_type) => {
                format!("vm.process.acquire[{}]", value_type.contract_text())
            }
            Self::VmProcessCancel(value_type) => {
                format!("vm.process.cancel[{}]", value_type.contract_text())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
