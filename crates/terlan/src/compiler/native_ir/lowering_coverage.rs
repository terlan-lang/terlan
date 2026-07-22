//! Versioned CoreIR-to-NativeIR lowering coverage.

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePattern, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

/// Current version of the executable lowering coverage contract.
pub(super) const LOWERING_COVERAGE_VERSION: u32 = 6;

/// One backend disposition for a CoreIR node family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LoweringDisposition {
    /// NativeIR and the native object emitter implement the node directly.
    NativeLowered,
    /// A mandatory compiler pass removes the node before NativeIR admission.
    CompilerRewrite,
    /// Native admission rejects the node with a stable diagnostic.
    Rejected,
}

/// Versioned coverage entry for one CoreIR node family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LoweringCoverage {
    /// Stable CoreIR node-family name.
    pub(super) node: &'static str,
    /// Backend disposition assigned to the node family.
    pub(super) disposition: LoweringDisposition,
    /// Stable diagnostic code used when the disposition is `Rejected`.
    pub(super) diagnostic: Option<&'static str>,
}

impl LoweringCoverage {
    /// Creates one directly lowered coverage entry.
    const fn native(node: &'static str) -> Self {
        Self {
            node,
            disposition: LoweringDisposition::NativeLowered,
            diagnostic: None,
        }
    }

    /// Creates one compiler-rewritten coverage entry.
    const fn rewritten(node: &'static str) -> Self {
        Self {
            node,
            disposition: LoweringDisposition::CompilerRewrite,
            diagnostic: None,
        }
    }

    /// Creates one rejected coverage entry with a stable diagnostic code.
    const fn rejected(node: &'static str, diagnostic: &'static str) -> Self {
        Self {
            node,
            disposition: LoweringDisposition::Rejected,
            diagnostic: Some(diagnostic),
        }
    }
}

/// Classifies one executable CoreIR expression node.
///
/// This match is deliberately exhaustive. Adding a CoreIR expression without
/// assigning its native disposition is a compile-time failure.
pub(super) fn expression_coverage(expr: &CoreExpr) -> LoweringCoverage {
    match expr {
        CoreExpr::Int(_) => LoweringCoverage::native("Int"),
        CoreExpr::Float(_) => LoweringCoverage::native("Float"),
        CoreExpr::Binary(_) => LoweringCoverage::native("Binary"),
        CoreExpr::Atom(_) => LoweringCoverage::native("Atom"),
        CoreExpr::Var(_) => LoweringCoverage::native("Var"),
        CoreExpr::Tuple(_) => LoweringCoverage::rewritten("Tuple"),
        CoreExpr::List(_) => LoweringCoverage::native("List"),
        CoreExpr::ListCons { .. } => LoweringCoverage::native("ListCons"),
        CoreExpr::FixedArray(_) => LoweringCoverage::rewritten("FixedArray"),
        CoreExpr::Index { .. } => LoweringCoverage::rewritten("Index"),
        CoreExpr::ListComprehension { .. } => LoweringCoverage::rewritten("ListComprehension"),
        CoreExpr::Let { .. } => LoweringCoverage::native("Let"),
        CoreExpr::Map(_) => LoweringCoverage::native("Map"),
        CoreExpr::RecordConstruct { .. } => LoweringCoverage::native("RecordConstruct"),
        CoreExpr::FieldAccess { .. } => LoweringCoverage::native("FieldAccess"),
        CoreExpr::RecordAccess { .. } => LoweringCoverage::native("RecordAccess"),
        CoreExpr::RecordUpdate { .. } => LoweringCoverage::native("RecordUpdate"),
        CoreExpr::TemplateInstantiate { .. } => LoweringCoverage::rewritten("TemplateInstantiate"),
        CoreExpr::ConstructorChain { .. } => LoweringCoverage::rewritten("ConstructorChain"),
        CoreExpr::RemoteFunRef { .. } => LoweringCoverage::native("RemoteFunRef"),
        CoreExpr::RemoteCall { .. } => LoweringCoverage::rewritten("RemoteCall"),
        CoreExpr::ConstructorCall { .. } => LoweringCoverage::native("ConstructorCall"),
        CoreExpr::Call { .. } => LoweringCoverage::native("Call"),
        CoreExpr::MutableReceiverCall { .. } => LoweringCoverage::rewritten("MutableReceiverCall"),
        CoreExpr::FunctionCall { .. } => LoweringCoverage::native("FunctionCall"),
        CoreExpr::Cast { .. } => LoweringCoverage::native("Cast"),
        CoreExpr::Intrinsic(call) => call
            .effects
            .effects
            .iter()
            .map(|effect| effect_coverage(effect))
            .find(|coverage| coverage.disposition == LoweringDisposition::Rejected)
            .unwrap_or_else(|| intrinsic_coverage(&call.id)),
        CoreExpr::SqlQuery { .. } => LoweringCoverage::rejected("SqlQuery", "native_ir.sql_query"),
        CoreExpr::Case { .. } => LoweringCoverage::native("Case"),
        CoreExpr::Try { .. } => LoweringCoverage::native("Try"),
        CoreExpr::If { .. } => LoweringCoverage::native("If"),
        CoreExpr::Lam { .. } => LoweringCoverage::native("Lam"),
        CoreExpr::UnaryOp { .. } => LoweringCoverage::native("UnaryOp"),
        CoreExpr::BinaryOp { .. } => LoweringCoverage::native("BinaryOp"),
    }
}

/// Classifies one CoreIR pattern node.
///
/// Function-parameter variables are native-lowered. Scalar case patterns enter
/// compiler rewriting; structured destructuring remains rejected.
pub(super) fn pattern_coverage(pattern: &CorePattern) -> LoweringCoverage {
    match pattern {
        CorePattern::Var(_) => LoweringCoverage::native("Pattern.Var"),
        CorePattern::Wildcard => LoweringCoverage::rewritten("Pattern.Wildcard"),
        CorePattern::Int(_) => LoweringCoverage::rewritten("Pattern.Int"),
        CorePattern::Float(_) => LoweringCoverage::rewritten("Pattern.Float"),
        CorePattern::String(_) => LoweringCoverage::native("Pattern.String"),
        CorePattern::StringPattern(_) => {
            LoweringCoverage::rejected("Pattern.StringPattern", "native_ir.pattern.string_segments")
        }
        CorePattern::Atom(_) => LoweringCoverage::rewritten("Pattern.Atom"),
        CorePattern::Tuple(_) => LoweringCoverage::native("Pattern.Tuple"),
        CorePattern::Alias { .. } => LoweringCoverage::rewritten("Pattern.Alias"),
        CorePattern::List(_) => LoweringCoverage::native("Pattern.List"),
        CorePattern::ListCons { .. } => LoweringCoverage::native("Pattern.ListCons"),
        CorePattern::Map(_) => LoweringCoverage::native("Pattern.Map"),
        CorePattern::Record { .. } => LoweringCoverage::native("Pattern.Record"),
        CorePattern::BinaryLayout { .. } => LoweringCoverage::native("Pattern.BinaryLayout"),
        CorePattern::Constructor { .. } => LoweringCoverage::native("Pattern.Constructor"),
    }
}

/// Classifies one closed CoreIR intrinsic identity.
///
/// The exhaustive primitive match is the intrinsic portion of the coverage
/// gate: a newly introduced intrinsic cannot inherit an accidental default.
pub(super) fn intrinsic_coverage(intrinsic: &CoreIntrinsicId) -> LoweringCoverage {
    match intrinsic {
        CoreIntrinsicId::Primitive(primitive) => primitive_intrinsic_coverage(primitive),
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln) => {
            LoweringCoverage::native("Intrinsic.runtime.console.println")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => {
            LoweringCoverage::native("Intrinsic.runtime.file.exists")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileWriteText) => {
            LoweringCoverage::native("Intrinsic.runtime.file.write_text")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileAppendText) => {
            LoweringCoverage::native("Intrinsic.runtime.file.append_text")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileDelete) => {
            LoweringCoverage::native("Intrinsic.runtime.file.delete")
        }
        CoreIntrinsicId::VmProcessSendMessage(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.send")
        }
        CoreIntrinsicId::VmProcessReceiveMessage(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.receive")
        }
        CoreIntrinsicId::VmProcessSpawn(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.spawn")
        }
        CoreIntrinsicId::VmProcessLink(_) => LoweringCoverage::native("Intrinsic.vm.process.link"),
        CoreIntrinsicId::VmProcessMonitor(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.monitor")
        }
        CoreIntrinsicId::VmProcessAcquireResource(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.acquire")
        }
        CoreIntrinsicId::VmProcessCancel(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.cancel")
        }
    }
}

/// Classifies one compiler-owned primitive intrinsic.
fn primitive_intrinsic_coverage(intrinsic: &CorePrimitiveIntrinsic) -> LoweringCoverage {
    use CorePrimitiveIntrinsic as P;

    match intrinsic {
        P::VmProcessYield => LoweringCoverage::native("Intrinsic.vm.process.yield_now"),
        P::VmProcessSendInt => LoweringCoverage::native("Intrinsic.vm.process.send_int"),
        P::VmProcessReceiveInt => LoweringCoverage::native("Intrinsic.vm.process.receive_int"),
        P::VmProcessSendString => LoweringCoverage::native("Intrinsic.vm.process.send_string"),
        P::VmProcessReceiveString => {
            LoweringCoverage::native("Intrinsic.vm.process.receive_string")
        }
        P::VmProcessSendBytes => LoweringCoverage::native("Intrinsic.vm.process.send_bytes"),
        P::VmProcessReceiveBytes => LoweringCoverage::native("Intrinsic.vm.process.receive_bytes"),
        P::VmProcessSendBinary => LoweringCoverage::native("Intrinsic.vm.process.send_binary"),
        P::VmProcessReceiveBinary => {
            LoweringCoverage::native("Intrinsic.vm.process.receive_binary")
        }
        P::VmProcessSendAtom => LoweringCoverage::native("Intrinsic.vm.process.send_atom"),
        P::VmProcessReceiveAtom => LoweringCoverage::native("Intrinsic.vm.process.receive_atom"),
        P::VmProcessSleep => LoweringCoverage::native("Intrinsic.vm.process.sleep"),
        P::VmProcessFail => LoweringCoverage::native("Intrinsic.vm.process.fail"),
        P::VmProcessSchedule => LoweringCoverage::native("Intrinsic.vm.process.schedule"),
        P::TypeOf
        | P::IsType
        | P::BoolEqual
        | P::BoolCompare
        | P::BoolToString
        | P::BoolFromString
        | P::AtomToString
        | P::ValueToString
        | P::IntToString
        | P::IntFromString
        | P::IntToStringBase
        | P::IntFromStringBase
        | P::FloatToString
        | P::FloatFromString
        | P::FloatFloor
        | P::FloatCeil
        | P::FloatLog
        | P::FloatPi
        | P::FloatTau
        | P::StringEqual
        | P::StringCompare
        | P::StringToString
        | P::StringFromString
        | P::StringIsEmpty
        | P::StringAppend
        | P::StringConcat
        | P::StringContains
        | P::StringStartsWith
        | P::StringEndsWith
        | P::StringLength
        | P::StringByteSize
        | P::StringLowercase
        | P::StringUppercase
        | P::StringReverse
        | P::StringTrim
        | P::StringTrimStart
        | P::StringTrimEnd
        | P::StringReplace
        | P::StringSplit
        | P::StringSplitOnce
        | P::ListNew
        | P::ListIsEmpty
        | P::ListLength
        | P::ListFirst
        | P::ListRest
        | P::ListConcat
        | P::ListSubtract
        | P::ListIterator
        | P::ListPush
        | P::ListClear
        | P::IteratorNext
        | P::MapNew
        | P::MapFromEntries
        | P::MapIsEmpty
        | P::MapSize
        | P::MapGet
        | P::MapTake
        | P::MapContainsKey
        | P::MapIterator
        | P::MapPut
        | P::MapRemove
        | P::MapClear
        | P::SetNew
        | P::SetFromList
        | P::SetIsEmpty
        | P::SetSize
        | P::SetContains
        | P::SetIterator
        | P::SetAdd
        | P::SetRemove
        | P::SetClear
        | P::TaskDone
        | P::TaskFailed
        | P::TaskResult
        | P::VmEffectRun
        | P::VmAgentStart
        | P::VmAgentGet
        | P::VmAgentGetAndUpdate
        | P::VmAgentUpdate
        | P::VmAgentCast
        | P::VmAgentStop
        | P::VmGenServerStart
        | P::VmGenServerCall
        | P::VmGenServerCast
        | P::VmGenServerStop
        | P::VmNativeBridgeStart
        | P::VmNativeBridgeCall
        | P::VmNativeBridgeDispose
        | P::VmNativeBridgeStop
        | P::VmBytesFromList
        | P::VmBytesToList
        | P::VmBytesLength
        | P::VmBytesConcat
        | P::VmBytesSlice
        | P::VmBytesReadUintBe
        | P::VmBytesReadIntBe
        | P::VmBytesReadUintLe
        | P::VmBytesReadIntLe
        | P::VmBitStringFromBytes
        | P::VmBitStringFromAllBytes
        | P::VmBitStringFromExactBytes
        | P::VmBitStringRequireExactBits
        | P::VmBitStringFromUintBe
        | P::VmBitStringFromIntBe
        | P::VmBitStringFromUintLe
        | P::VmBitStringFromIntLe
        | P::VmBitStringUtf8Scalar
        | P::VmBitStringToUtf8Scalar
        | P::VmBitStringUtf16BeScalar
        | P::VmBitStringUtf16LeScalar
        | P::VmBitStringToUtf16BeScalar
        | P::VmBitStringToUtf16LeScalar
        | P::VmBitStringUtf32BeScalar
        | P::VmBitStringUtf32LeScalar
        | P::VmBitStringToUtf32BeScalar
        | P::VmBitStringToUtf32LeScalar
        | P::VmBitStringBitLength
        | P::VmBitStringByteLength
        | P::VmBitStringIsByteAligned
        | P::VmBitStringSlice
        | P::VmBitStringConcat
        | P::VmBitStringToBytes
        | P::VmBitStringToUintBe
        | P::VmBitStringToIntBe
        | P::VmBitStringToUintLe
        | P::VmBitStringToIntLe
        | P::VmTimeoutMilliseconds
        | P::VmTimeoutForever
        | P::VmTcpListen
        | P::VmTcpListenWithBacklog
        | P::VmTcpAccept
        | P::VmTcpConnect
        | P::VmTcpSend
        | P::VmTcpReceive
        | P::VmTcpClose
        | P::VmTcpCloseListener
        | P::VmPortOpen
        | P::VmPortWrite
        | P::VmPortRead
        | P::VmPortClose
        | P::VmSupervisorStartRoot
        | P::VmSupervisorChildSpec
        | P::VmSupervisorStart
        | P::VmSupervisorStop
        | P::VmTaskStart
        | P::VmTaskResult
        | P::VmTaskCancel => {
            LoweringCoverage::rejected("Intrinsic.primitive", "native_ir.unsupported_intrinsic")
        }
    }
}

/// Classifies one CoreIR effect label.
pub(super) fn effect_coverage(effect: &str) -> LoweringCoverage {
    match effect {
        "pure" => LoweringCoverage::native("Effect.pure"),
        "vm_effect_execution" => LoweringCoverage::native("Effect.vm_effect_execution"),
        "io" => LoweringCoverage::native("Effect.io"),
        "receiver_mutation" => LoweringCoverage::rewritten("Effect.receiver_mutation"),
        _ => LoweringCoverage::rejected("Effect.unknown", "native_ir.effect.unknown"),
    }
}
