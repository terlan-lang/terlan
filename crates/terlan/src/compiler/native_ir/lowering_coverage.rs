//! Versioned CoreIR-to-NativeIR lowering coverage.

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePattern, CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

/// Current version of the executable lowering coverage contract.
#[cfg(test)]
pub(super) const LOWERING_COVERAGE_VERSION: u32 = 7;

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
        CoreExpr::SqlQuery { .. } => LoweringCoverage::native("SqlQuery"),
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
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsoleEprintln) => {
            LoweringCoverage::native("Intrinsic.runtime.console.eprintln")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ClockUnixTimeNs) => {
            LoweringCoverage::native("Intrinsic.runtime.clock.unix_time_ns")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ClockMonotonicTimeNs) => {
            LoweringCoverage::native("Intrinsic.runtime.clock.monotonic_time_ns")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => {
            LoweringCoverage::native("Intrinsic.runtime.file.exists")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadBytes) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_bytes")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileSize) => {
            LoweringCoverage::native("Intrinsic.runtime.file.size")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileTimestamps) => {
            LoweringCoverage::native("Intrinsic.runtime.file.timestamps")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileSetTimestamps) => {
            LoweringCoverage::native("Intrinsic.runtime.file.set_timestamps")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileIsExecutable) => {
            LoweringCoverage::native("Intrinsic.runtime.file.is_executable")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileSetExecutable) => {
            LoweringCoverage::native("Intrinsic.runtime.file.set_executable")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileCopy) => {
            LoweringCoverage::native("Intrinsic.runtime.file.copy")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileCopyMany) => {
            LoweringCoverage::native("Intrinsic.runtime.file.copy_many")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextMany) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text_many")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextDirectory) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text_directory")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextTreeExcluding) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text_tree_excluding")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextTreeMatching) => {
            LoweringCoverage::native("Intrinsic.runtime.file.read_text_tree_matching")
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
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemArgumentsCount) => {
            LoweringCoverage::native("Intrinsic.runtime.system.arguments.count")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemArgumentsGet) => {
            LoweringCoverage::native("Intrinsic.runtime.system.arguments.get")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemEnvironmentContains) => {
            LoweringCoverage::native("Intrinsic.runtime.system.environment.contains")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemEnvironmentGet) => {
            LoweringCoverage::native("Intrinsic.runtime.system.environment.get")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemEnvironmentCurrentDirectory) => {
            LoweringCoverage::native("Intrinsic.runtime.system.environment.current_directory")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemPlatformCurrentMetrics) => {
            LoweringCoverage::native("Intrinsic.runtime.system.platform.current_metrics")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemProcessLimits) => {
            LoweringCoverage::native("Intrinsic.runtime.system.process.limits")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemProcessRun) => {
            LoweringCoverage::native("Intrinsic.runtime.system.process.run")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemProcessRunMany) => {
            LoweringCoverage::native("Intrinsic.runtime.system.process.run_many")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::SystemProcessRunLengthFramed) => {
            LoweringCoverage::native("Intrinsic.runtime.system.process.run_length_framed")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryEntries) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.entries")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryFilesRecursive) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.files_recursive")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryFilesRecursiveExcluding) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.files_recursive_excluding")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryFindNamedRecursiveExcluding) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.find_named_recursive_excluding")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryTreeUsage) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.tree_usage")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryCopyTreeExcluding) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.copy_tree_excluding")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryCreateSymbolicLink) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.create_symbolic_link")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryCreateAll) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.create_all")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryCreateTemporary) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.create_temporary")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::DirectoryRemoveAll) => {
            LoweringCoverage::native("Intrinsic.runtime.directory.remove_all")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ArchiveCreate) => {
            LoweringCoverage::native("Intrinsic.runtime.archive.create")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ArchiveExtract) => {
            LoweringCoverage::native("Intrinsic.runtime.archive.extract")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashSha256File) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.sha256_file")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashVerifySha256Manifest) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.verify_sha256_manifest")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashSha256Tree) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.sha256_tree")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashSha256SelectedFiles) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.sha256_selected_files")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashSha256LabeledFileDigests) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.sha256_labeled_file_digests")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashSha256LabeledFileContents) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.sha256_labeled_file_contents")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashAuditLabeledFiles) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.audit_labeled_files")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::HashAuditLabeledFilePatterns) => {
            LoweringCoverage::native("Intrinsic.runtime.hash.audit_labeled_file_patterns")
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::GitSourceTreeIdentity) => {
            LoweringCoverage::native("Intrinsic.runtime.git.source_tree_identity")
        }
        CoreIntrinsicId::MemoryLayoutOf(_) => {
            LoweringCoverage::native("Intrinsic.memory.layout_of")
        }
        CoreIntrinsicId::MemoryShallowSize(_) => {
            LoweringCoverage::native("Intrinsic.memory.shallow_size")
        }
        CoreIntrinsicId::MemoryRetainedSize(_) => {
            LoweringCoverage::native("Intrinsic.memory.retained_size")
        }
        CoreIntrinsicId::NativeOperation { .. } => {
            LoweringCoverage::native("Intrinsic.runtime.native_package")
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
        CoreIntrinsicId::VmProcessEntry(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.entry")
        }
        CoreIntrinsicId::VmProcessCurrent(_) => {
            LoweringCoverage::native("Intrinsic.vm.process.current")
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
        P::MemoryLayoutSize => LoweringCoverage::native("Intrinsic.memory.layout.size"),
        P::MemoryLayoutAlignment => LoweringCoverage::native("Intrinsic.memory.layout.alignment"),
        P::MemoryLayoutStorage => LoweringCoverage::native("Intrinsic.memory.layout.storage"),
        P::VmDebuggerBreak => LoweringCoverage::native("Intrinsic.vm.debugger.break"),
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
        P::FloatFloor => LoweringCoverage::native("Intrinsic.core.float.floor"),
        P::FloatCeil => LoweringCoverage::native("Intrinsic.core.float.ceil"),
        P::FloatToString => LoweringCoverage::native("Intrinsic.core.float.to_string"),
        P::FloatFromString => LoweringCoverage::native("Intrinsic.core.float.from_string"),
        P::FloatLog => LoweringCoverage::native("Intrinsic.core.float.log"),
        P::FloatPi => LoweringCoverage::native("Intrinsic.core.float.pi"),
        P::FloatTau => LoweringCoverage::native("Intrinsic.core.float.tau"),
        P::BoolToString => LoweringCoverage::native("Intrinsic.core.bool.to_string"),
        P::ValueToString => LoweringCoverage::native("Intrinsic.core.value.to_string"),
        P::IntToString => LoweringCoverage::native("Intrinsic.core.int.to_string"),
        P::IntFromString => LoweringCoverage::native("Intrinsic.core.int.from_string"),
        P::IntToStringBase => LoweringCoverage::native("Intrinsic.core.int.to_string_base"),
        P::IntFromStringBase => LoweringCoverage::native("Intrinsic.core.int.from_string_base"),
        P::ListNew => LoweringCoverage::native("Intrinsic.collections.list.new"),
        P::ListIsEmpty => LoweringCoverage::native("Intrinsic.collections.list.is_empty"),
        P::ListLength => LoweringCoverage::native("Intrinsic.collections.list.length"),
        P::ListGet => LoweringCoverage::native("Intrinsic.collections.list.get"),
        P::ListFirst => LoweringCoverage::native("Intrinsic.collections.list.first"),
        P::ListRest => LoweringCoverage::native("Intrinsic.collections.list.rest"),
        P::ListIterator => LoweringCoverage::native("Intrinsic.collections.list.iterator"),
        P::SetFromList => LoweringCoverage::native("Intrinsic.collections.set.from_list"),
        P::SetContains => LoweringCoverage::native("Intrinsic.collections.set.contains"),
        P::SetNew => LoweringCoverage::native("Intrinsic.collections.set.new"),
        P::SetIsEmpty => LoweringCoverage::native("Intrinsic.collections.set.is_empty"),
        P::SetSize => LoweringCoverage::native("Intrinsic.collections.set.size"),
        P::SetIterator => LoweringCoverage::native("Intrinsic.collections.set.iterator"),
        P::SetAdd => LoweringCoverage::native("Intrinsic.collections.set.add"),
        P::SetRemove => LoweringCoverage::native("Intrinsic.collections.set.remove"),
        P::SetClear => LoweringCoverage::native("Intrinsic.collections.set.clear"),
        P::VmBytesFromList => LoweringCoverage::native("Intrinsic.vm.bytes.from_list"),
        P::VmBytesToList => LoweringCoverage::native("Intrinsic.vm.bytes.to_list"),
        P::VmBytesLength => LoweringCoverage::native("Intrinsic.vm.bytes.length"),
        P::VmBytesStartsWith => LoweringCoverage::native("Intrinsic.vm.bytes.starts_with"),
        P::VmBytesContains => LoweringCoverage::native("Intrinsic.vm.bytes.contains"),
        P::VmBytesFirstNonAsciiWhitespace => {
            LoweringCoverage::native("Intrinsic.vm.bytes.first_non_ascii_whitespace")
        }
        P::VmBytesConcat => LoweringCoverage::native("Intrinsic.vm.bytes.concat"),
        P::VmBytesSlice => LoweringCoverage::native("Intrinsic.vm.bytes.slice"),
        P::VmBytesReadUintBe => LoweringCoverage::native("Intrinsic.vm.bytes.read_uint_be"),
        P::VmBytesReadIntBe => LoweringCoverage::native("Intrinsic.vm.bytes.read_int_be"),
        P::VmBytesReadUintLe => LoweringCoverage::native("Intrinsic.vm.bytes.read_uint_le"),
        P::VmBytesReadIntLe => LoweringCoverage::native("Intrinsic.vm.bytes.read_int_le"),
        P::VmBitStringFromBytes
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
        | P::VmBitStringToIntLe => LoweringCoverage::native("Intrinsic.vm.bitstring"),
        P::TypeOf
        | P::IsType
        | P::BoolEqual
        | P::BoolCompare
        | P::BoolFromString
        | P::AtomToString
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
        | P::StringCharacters
        | P::StringCodepoints
        | P::StringUtf8ByteAt
        | P::StringUtf8FindAnyByte
        | P::StringUtf8Slice
        | P::StringTrim
        | P::StringTrimStart
        | P::StringTrimEnd
        | P::StringReplace
        | P::StringSplit
        | P::StringSplitOnce
        | P::CryptoSha256
        | P::ListConcat
        | P::ListSubtract
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
        | P::TaskDone
        | P::TaskFailed
        | P::TaskResult
        | P::VmEffectRun
        | P::VmNativeBridgeStart
        | P::VmNativeBridgeCall
        | P::VmNativeBridgeDispose
        | P::VmNativeBridgeStop
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
        | P::VmPortClose => {
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
