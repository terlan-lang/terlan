use terlan_hir::ModuleInterface;
use terlan_syntax::span::Span;

mod patterns;
mod proof_payloads;
mod types;

pub use patterns::{CoreMapPatternField, CorePattern, CoreRecordPatternField};
pub use proof_payloads::{
    CoreCheckedPreservationEvidence, CoreCheckedPreservationEvidenceKind, CoreProofCoverage,
    CoreProofReadiness, CoreSubstitutionFreshnessEvidence,
};
use types::core_type_contract_text;
pub(crate) use types::{
    atom_type_literal_payload, core_type_from_body_variants, core_type_from_text,
};
pub use types::{CoreMapTypeField, CoreStructTypeField, CoreTupleTypeElem, CoreType};

pub const CORE_IR_SCHEMA: &str = "terlan.core_ir.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Source identity attached to a CoreIR module.
///
/// Inputs: typed phase source metadata. Output: stable source-kind and syntax
/// fingerprint fields. Transformation: carries provenance into CoreIR without
/// embedding parser or backend state.
pub struct CoreSourceIdentity {
    pub source_kind: String,
    pub syntax_contract_fingerprint: Option<String>,
}

/// Import class preserved at the backend-neutral CoreIR boundary.
///
/// Inputs:
/// - Syntax-output import declaration kind, or resolver interface imports when
///   source kind is unavailable.
///
/// Output:
/// - Stable import-kind tag for target-profile validation.
///
/// Transformation:
/// - Distinguishes normal module imports from asset imports without carrying
///   backend resolver state into CoreIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreImportKind {
    Module,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Import preserved at the backend-neutral CoreIR boundary.
///
/// Inputs: resolved import metadata. Output: module name plus import kind.
/// Transformation: records only the target-neutral import classification.
pub struct CoreImport {
    pub module: String,
    pub kind: CoreImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Export preserved in CoreIR.
///
/// Inputs: resolved public declaration metadata. Output: export name and kind.
/// Transformation: records declaration visibility without backend export
/// syntax.
pub struct CoreExport {
    pub name: String,
    pub kind: CoreExportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Kind of exported Core declaration.
///
/// Inputs: resolved declaration shape. Output: function, type, or constructor
/// export identity. Transformation: keeps arity/min-arity metadata needed by
/// backends without carrying source syntax.
pub enum CoreExportKind {
    Function { arity: usize },
    Type,
    Constructor { min_arity: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Type declaration summarized in CoreIR.
///
/// Inputs: resolved type declaration. Output: source type text and optional
/// typed Core body. Transformation: preserves source-facing type shape while
/// attaching backend-neutral typed structure when available.
pub struct CoreTypeDecl {
    pub name: String,
    pub visibility: CoreVisibility,
    pub params: Vec<String>,
    pub body: Vec<String>,
    pub core_body: Option<CoreType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core visibility for declarations.
///
/// Inputs: source visibility and opacity modifiers. Output: public/private/
/// opaque tag. Transformation: normalizes visibility for backend validation.
pub enum CoreVisibility {
    Public,
    Private,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Function declaration summarized in CoreIR.
///
/// Inputs: resolved function signature and clauses. Output: typed function
/// summary. Transformation: preserves params, return type, visibility, and
/// clause summaries in backend-neutral form.
pub struct CoreFunction {
    pub name: String,
    pub arity: usize,
    pub public: bool,
    pub params: Vec<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
    pub clauses: Vec<CoreFunctionClause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Function or constructor parameter summarized in CoreIR.
///
/// Inputs: source parameter name and type annotation. Output: textual and typed
/// parameter shape. Transformation: attaches optional `CoreType` without
/// changing the source parameter identity.
pub struct CoreParam {
    pub name: String,
    pub ty: String,
    pub core_ty: Option<CoreType>,
}

/// Renders a Core parameter as deterministic contract text.
///
/// Inputs:
/// - `param`: Core function or constructor parameter summary.
///
/// Output:
/// - Stable text containing parameter name, original type text, and typed Core
///   type payload when available.
///
/// Transformation:
/// - Combines the textual annotation with the optional typed `CoreType`
///   payload without changing the parameter identity.
fn core_param_contract_text(param: &CoreParam) -> String {
    format!(
        "{}:{}:core={}",
        param.name,
        param.ty,
        core_type_contract_text(param.core_ty.as_ref())
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One function clause summarized in CoreIR.
///
/// Inputs: pattern list, optional guard, and body expression. Output:
/// clause-level Core summary. Transformation: records source pattern text,
/// typed pattern payloads, proof metadata, guard, and body.
pub struct CoreFunctionClause {
    pub patterns: Vec<String>,
    pub core_patterns: Vec<Option<CorePattern>>,
    pub pattern_proof_coverage: Vec<CoreProofCoverage>,
    pub pattern_checked_preservation_evidence: Vec<Option<CoreCheckedPreservationEvidence>>,
    pub guard: Option<CoreExprSummary>,
    pub body: CoreExprSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Expression summary carried through CoreIR.
///
/// Inputs: typed or partially typed expression lowering result. Output: summary
/// text, optional typed expression, and proof metadata. Transformation:
/// separates typed Core payload from summary-only fallback information.
pub struct CoreExprSummary {
    pub kind: String,
    pub core_expr: Option<CoreExpr>,
    pub checked_preservation_evidence: Option<CoreCheckedPreservationEvidence>,
    pub proof_coverage: CoreProofCoverage,
    pub text: Option<String>,
    pub remote: Option<String>,
    pub operator: Option<String>,
    pub arity: usize,
    pub children: Vec<CoreExprSummary>,
}

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
    fn contract_text(&self) -> String {
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
    MapIsEmpty,
    MapSize,
    MapGet,
    MapContainsKey,
    MapIterator,
    MapPut,
    MapRemove,
    MapClear,
    SetNew,
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
            Self::MapIsEmpty => "core.map.is_empty",
            Self::MapSize => "core.map.size",
            Self::MapGet => "core.map.get",
            Self::MapContainsKey => "core.map.contains_key",
            Self::MapIterator => "core.map.iterator",
            Self::MapPut => "core.map.put",
            Self::MapRemove => "core.map.remove",
            Self::MapClear => "core.map.clear",
            Self::SetNew => "core.set.new",
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
    fn contract_text(&self) -> String {
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Typed backend-neutral Core expression.
///
/// Inputs: syntax-output expressions after resolver/typechecker lowering.
/// Output: structured expression tree. Transformation: removes parser-specific
/// detail while preserving semantics needed by proof, validation, and backend
/// emitters.
pub enum CoreExpr {
    Int(i64),
    Float(String),
    Binary(String),
    Atom(String),
    Var(String),
    Tuple(Vec<CoreExpr>),
    List(Vec<CoreExpr>),
    ListCons {
        head: Box<CoreExpr>,
        tail: Box<CoreExpr>,
    },
    FixedArray(Vec<CoreExpr>),
    Index {
        base: Box<CoreExpr>,
        index: Box<CoreExpr>,
    },
    ListComprehension {
        expr: Box<CoreExpr>,
        pattern: CorePattern,
        source: Box<CoreExpr>,
        guard: Option<Box<CoreExpr>>,
    },
    Let {
        bindings: Vec<CoreLetBinding>,
        body: Box<CoreExpr>,
    },
    Map(Vec<CoreMapExprField>),
    RecordConstruct {
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    FieldAccess {
        base: Box<CoreExpr>,
        field: String,
    },
    RecordAccess {
        base: Box<CoreExpr>,
        name: String,
        field: String,
    },
    RecordUpdate {
        base: Box<CoreExpr>,
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    TemplateInstantiate {
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    ConstructorChain {
        base: String,
        base_constructor_identity: Option<String>,
        args: Vec<CoreExpr>,
        record: Box<CoreExpr>,
    },
    RemoteFunRef {
        module: String,
        function: String,
        arity: usize,
    },
    RemoteCall {
        module: String,
        function: String,
        args: Vec<CoreExpr>,
    },
    ConstructorCall {
        constructor: String,
        constructor_identity: Option<String>,
        args: Vec<CoreExpr>,
    },
    Call {
        function: String,
        args: Vec<CoreExpr>,
    },
    MutableReceiverCall {
        receiver: Box<CoreExpr>,
        method: String,
        args: Vec<CoreExpr>,
        effects: CoreEffectSet,
    },
    FunctionCall {
        callee: Box<CoreExpr>,
        args: Vec<CoreExpr>,
    },
    Cast {
        expr: Box<CoreExpr>,
        target_type: CoreType,
    },
    Intrinsic(CoreIntrinsicCall),
    Case {
        scrutinee: Box<CoreExpr>,
        clauses: Vec<CoreCaseClause>,
    },
    Try {
        body: Box<CoreExpr>,
        of_clauses: Vec<CoreCaseClause>,
        catch_clauses: Vec<CoreCaseClause>,
        after_clause: Option<CoreTryAfter>,
    },
    If {
        clauses: Vec<CoreIfClause>,
    },
    Lam {
        params: Vec<CorePattern>,
        body: Box<CoreExpr>,
    },
    UnaryOp {
        operator: String,
        operand: Box<CoreExpr>,
    },
    BinaryOp {
        operator: String,
        left: Box<CoreExpr>,
        right: Box<CoreExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core map expression field.
///
/// Inputs: map field key/operator/value. Output: typed map field. Transformation:
/// preserves insert/update intent through the `required` flag and stores the
/// lowered value expression.
pub struct CoreMapExprField {
    pub key: String,
    pub required: bool,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core let binding.
///
/// Inputs: binding name and value expression. Output: one local binding.
/// Transformation: represents source let bindings in expression form for
/// backend-neutral lowering.
pub struct CoreLetBinding {
    pub name: String,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core record or template field expression.
///
/// Inputs: field key/operator/value. Output: typed field payload.
/// Transformation: preserves assignment/update intent through `required` and
/// stores the lowered value expression.
pub struct CoreRecordExprField {
    pub key: String,
    pub required: bool,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core case-like branch.
///
/// Inputs: pattern, optional guard, and body expression. Output: typed branch.
/// Transformation: normalizes `case`, `try of`, and `catch` clauses into a
/// shared branch shape.
pub struct CoreCaseClause {
    pub pattern: CorePattern,
    pub guard: Option<CoreExpr>,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core if branch.
///
/// Inputs: condition and body expressions. Output: typed if clause.
/// Transformation: stores predicate/body pairs in source order.
pub struct CoreIfClause {
    pub condition: CoreExpr,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core try cleanup branch.
///
/// Inputs: cleanup trigger and body expressions. Output: typed after branch.
/// Transformation: keeps cleanup semantics explicit for target lowering.
pub struct CoreTryAfter {
    pub trigger: Box<CoreExpr>,
    pub body: Box<CoreExpr>,
}

impl CoreExpr {
    /// Renders a typed Core expression as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core expression from the initial Lean-covered subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the structural Core expression without source spans,
    ///   backend syntax, or syntax-output summary text.
    pub(crate) fn contract_text(&self) -> String {
        match self {
            CoreExpr::Int(value) => format!("Int({value})"),
            CoreExpr::Float(value) => format!("Float({value})"),
            CoreExpr::Binary(value) => format!("Binary({value})"),
            CoreExpr::Atom(value) => format!("Atom({value})"),
            CoreExpr::Var(name) => format!("Var({name})"),
            CoreExpr::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::List(elements) => format!(
                "List({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ListCons { head, tail } => {
                format!(
                    "ListCons({}|{})",
                    head.contract_text(),
                    tail.contract_text()
                )
            }
            CoreExpr::FixedArray(elements) => format!(
                "FixedArray({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Index { base, index } => {
                format!("Index({};{})", base.contract_text(), index.contract_text())
            }
            CoreExpr::ListComprehension {
                expr,
                pattern,
                source,
                guard,
            } => match guard {
                Some(guard) => format!(
                    "ListComprehension({}|{}<-{} when {})",
                    expr.contract_text(),
                    pattern.contract_text(),
                    source.contract_text(),
                    guard.contract_text()
                ),
                None => format!(
                    "ListComprehension({}|{}<-{})",
                    expr.contract_text(),
                    pattern.contract_text(),
                    source.contract_text()
                ),
            },
            CoreExpr::Let { bindings, body } => format!(
                "Let({};{})",
                bindings
                    .iter()
                    .map(CoreLetBinding::contract_text)
                    .collect::<Vec<_>>()
                    .join(";"),
                body.contract_text()
            ),
            CoreExpr::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::RecordConstruct { name, fields } => format!(
                "RecordConstruct({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::FieldAccess { base, field } => {
                format!("FieldAccess({}.{})", base.contract_text(), field)
            }
            CoreExpr::RecordAccess { base, name, field } => {
                format!("RecordAccess({}#{}.{})", base.contract_text(), name, field)
            }
            CoreExpr::RecordUpdate { base, name, fields } => format!(
                "RecordUpdate({}#{};{})",
                base.contract_text(),
                name,
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::TemplateInstantiate { name, fields } => format!(
                "TemplateInstantiate({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorChain {
                base,
                base_constructor_identity,
                args,
                record,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match base_constructor_identity {
                    Some(identity) => format!(
                        "ConstructorChain({base};identity={identity};{args} with {})",
                        record.contract_text()
                    ),
                    None => format!(
                        "ConstructorChain({base};{args} with {})",
                        record.contract_text()
                    ),
                }
            }
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => format!("RemoteFunRef({module}:{function}/{arity})"),
            CoreExpr::RemoteCall {
                module,
                function,
                args,
            } => format!(
                "RemoteCall({module}:{function};{})",
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match constructor_identity {
                    Some(identity) => {
                        format!("ConstructorCall({constructor};identity={identity};{args})")
                    }
                    None => format!("ConstructorCall({constructor};{args})"),
                }
            }
            CoreExpr::Call { function, args } => format!(
                "Call({};{})",
                function,
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::MutableReceiverCall {
                receiver,
                method,
                args,
                effects,
            } => format!(
                "MutableReceiverCall({}.{};args={};effects={})",
                receiver.contract_text(),
                method,
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                effects.contract_text()
            ),
            CoreExpr::FunctionCall { callee, args } => format!(
                "FunctionCall({};{})",
                callee.contract_text(),
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Cast { expr, target_type } => {
                format!(
                    "Cast({} as {})",
                    expr.contract_text(),
                    target_type.contract_text()
                )
            }
            CoreExpr::Intrinsic(call) => call.contract_text(),
            CoreExpr::Case { scrutinee, clauses } => format!(
                "Case({};{})",
                scrutinee.contract_text(),
                clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                let of_clauses = of_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                let catch_clauses = catch_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                match after_clause {
                    Some(after_clause) => format!(
                        "Try({};of={};catch={};after={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses,
                        after_clause.contract_text()
                    ),
                    None => format!(
                        "Try({};of={};catch={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses
                    ),
                }
            }
            CoreExpr::If { clauses } => format!(
                "If({})",
                clauses
                    .iter()
                    .map(CoreIfClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Lam { params, body } => format!(
                "Lam({};{})",
                params
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                body.contract_text()
            ),
            CoreExpr::UnaryOp { operator, operand } => {
                format!("UnaryOp({};{})", operator, operand.contract_text())
            }
            CoreExpr::BinaryOp {
                operator,
                left,
                right,
            } => format!(
                "BinaryOp({};{}, {})",
                operator,
                left.contract_text(),
                right.contract_text()
            ),
        }
    }
}

impl CoreMapExprField {
    /// Renders a typed Core map-expression field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core map-expression field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source insert/update operator, and
    ///   recursively rendered value expression without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreLetBinding {
    /// Renders one typed Core let binding as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: local binding lowered from syntax output.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the binding name and recursively rendered value expression
    ///   without source spans or backend syntax.
    fn contract_text(&self) -> String {
        format!("{}={}", self.name, self.value.contract_text())
    }
}

impl CoreRecordExprField {
    /// Renders a typed Core record-construction field as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed Core record field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source field assignment operator, and
    ///   recursively rendered value expression without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { "=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreCaseClause {
    /// Renders a typed Core case clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed unguarded case clause from the current Core subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the pattern/body pair without source spans, backend syntax,
    ///   or syntax-output summary text.
    fn contract_text(&self) -> String {
        let body = self.body.contract_text();
        match &self.guard {
            Some(guard) => format!(
                "{} when {}=>{}",
                self.pattern.contract_text(),
                guard.contract_text(),
                body
            ),
            None => format!("{}=>{}", self.pattern.contract_text(), body),
        }
    }
}

impl CoreIfClause {
    /// Renders a typed Core if clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed condition/body branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the condition/body pair without source spans, backend
    ///   syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.condition.contract_text(),
            self.body.contract_text()
        )
    }
}

impl CoreTryAfter {
    /// Renders a typed Core try cleanup branch as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed try cleanup branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the cleanup trigger/body pair without source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.trigger.contract_text(),
            self.body.contract_text()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Constructor declaration summarized in CoreIR.
///
/// Inputs: resolved constructor declaration. Output: constructor signature.
/// Transformation: records public flag, fixed params, optional vararg, return
/// type, and typed return shape without backend constructor code.
pub struct CoreConstructorDecl {
    pub name: String,
    pub public: bool,
    pub min_arity: usize,
    pub params: Vec<CoreParam>,
    pub vararg: Option<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
}

/// Source category for a backend-neutral trait conformance fact.
///
/// Inputs:
/// - Syntax-output declaration form that introduced the conformance.
///
/// Output:
/// - Stable category carried in CoreIR.
///
/// Transformation:
/// - Classifies source syntax without choosing a backend representation for
///   trait dictionaries, receiver methods, or adapter functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreTraitConformanceSource {
    Implements,
    ExplicitImpl,
}

/// Backend-neutral trait conformance fact preserved in CoreIR.
///
/// Inputs:
/// - Syntax-output `implements` declarations or explicit `impl Trait for Type`
///   declarations.
///
/// Output:
/// - Stable conformance summary for downstream target-profile validation and
///   future backend lowering.
///
/// Transformation:
/// - Preserves trait reference text, owner type text, source category, and
///   visibility without lowering to target-specific runtime dictionaries.
///   Struct `derives` clauses are intentionally excluded because they derive
///   struct shape, not trait conformance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTraitConformance {
    pub trait_ref: String,
    pub for_type: String,
    pub source: CoreTraitConformanceSource,
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Core module metadata and proof/readiness counters.
///
/// Inputs: generated Core module summaries. Output: deterministic counts for
/// release gates and proof readiness. Transformation: aggregates typed,
/// summary-only, proof-coverage, runtime-boundary, and constructor-resolution
/// counters.
pub struct CoreModuleMetadata {
    pub interface_function_count: usize,
    pub interface_type_count: usize,
    pub constructor_count: usize,
    pub proof_readiness: CoreProofReadiness,
    pub lean_covered_expr_count: usize,
    pub partial_expr_count: usize,
    pub proof_model_required_expr_count: usize,
    pub runtime_boundary_expr_count: usize,
    pub artifact_only_expr_count: usize,
    pub lean_covered_pattern_count: usize,
    pub partial_pattern_count: usize,
    pub proof_model_required_pattern_count: usize,
    pub runtime_boundary_pattern_count: usize,
    pub artifact_only_pattern_count: usize,
    pub typed_core_expr_count: usize,
    pub summary_only_expr_count: usize,
    pub typed_core_pattern_count: usize,
    pub summary_only_pattern_count: usize,
    pub typed_core_type_count: usize,
    pub summary_only_type_count: usize,
    pub checked_preservation_expr_count: usize,
    pub checked_preservation_pattern_count: usize,
    pub checked_preservation_expr_structural_count: usize,
    pub checked_preservation_pattern_structural_count: usize,
    pub checked_preservation_expr_no_runtime_bindings_count: usize,
    pub checked_preservation_pattern_no_runtime_bindings_count: usize,
    pub checked_preservation_expr_runtime_bindings_required_count: usize,
    pub checked_preservation_pattern_runtime_bindings_required_count: usize,
    pub resolved_constructor_call_identity_count: usize,
    pub resolved_constructor_chain_identity_count: usize,
    pub resolved_constructor_pattern_identity_count: usize,
    pub unresolved_constructor_call_candidate_count: usize,
    pub unresolved_constructor_chain_candidate_count: usize,
    pub unresolved_constructor_pattern_candidate_count: usize,
}

/// Backend-agnostic core module produced by the formal typed phase.
///
/// Inputs:
/// - `resolved` module from the resolver after syntax checks.
///
/// Output:
/// - A core representation whose current payload is still declarative and
///   backend-independent.
///
/// Transformation:
/// - Performs the current production handoff point between the typed resolver
///   phase and future backend-specific lowering.
#[derive(Debug, Clone)]
pub struct CoreModule {
    /// Stable CoreIR schema identifier.
    pub schema: String,
    /// Resolved module name for downstream bookkeeping.
    pub module: String,
    /// Source identity for the phase that produced this module.
    pub source: CoreSourceIdentity,
    /// Resolved module imports visible to this Core module.
    pub imports: Vec<CoreImport>,
    /// Public exports represented by this Core module.
    pub exports: Vec<CoreExport>,
    /// Resolved type declarations represented by this Core module.
    pub types: Vec<CoreTypeDecl>,
    /// Function signatures represented by this Core module.
    pub functions: Vec<CoreFunction>,
    /// Constructor signatures represented by this Core module.
    pub constructors: Vec<CoreConstructorDecl>,
    /// Backend-neutral trait conformance facts represented by this Core module.
    pub trait_conformances: Vec<CoreTraitConformance>,
    /// Backend-independent counts and phase metadata.
    pub metadata: CoreModuleMetadata,
    /// Public interface snapshot for backend-independent emission.
    pub interface: ModuleInterface,
}

impl CoreModule {
    /// Renders the interface portion of the core module for golden tests and
    /// deterministic snapshot comparison.
    pub fn interface_text(&self) -> String {
        self.interface.to_terlan_interface_text()
    }

    /// Renders a deterministic CoreIR contract snapshot.
    ///
    /// Inputs:
    /// - `self`: Core module artifact produced by formal typechecking.
    ///
    /// Output:
    /// - Stable line-oriented text suitable for golden fixtures.
    ///
    /// Transformation:
    /// - Serializes only backend-agnostic CoreIR identity and declaration
    ///   summaries. It intentionally omits backend syntax and emitted artifacts.
    pub fn contract_text(&self) -> String {
        let mut lines = vec![
            format!("schema={}", self.schema),
            format!("module={}", self.module),
            format!("source_kind={}", self.source.source_kind),
            format!(
                "syntax_contract_fingerprint={}",
                self.source
                    .syntax_contract_fingerprint
                    .as_deref()
                    .unwrap_or("none")
            ),
        ];
        lines.extend(self.imports.iter().map(|import| match import.kind {
            CoreImportKind::Module => format!("import={}", import.module),
            CoreImportKind::File => format!("import=file:{}", import.module),
            CoreImportKind::Css => format!("import=css:{}", import.module),
            CoreImportKind::Markdown => format!("import=markdown:{}", import.module),
        }));
        lines.extend(self.exports.iter().map(|export| {
            format!(
                "export={}{}",
                export.name,
                match export.kind {
                    CoreExportKind::Function { arity } => format!("/{}", arity),
                    CoreExportKind::Type => ":type".to_string(),
                    CoreExportKind::Constructor { min_arity } =>
                        format!(":constructor/{}", min_arity),
                }
            )
        }));
        lines.extend(self.types.iter().map(|decl| {
            format!(
                "type={} visibility={:?} params={} body={} body_core={}",
                decl.name,
                decl.visibility,
                decl.params.join(","),
                decl.body.join(" "),
                core_type_contract_text(decl.core_body.as_ref())
            )
        }));
        lines.extend(self.functions.iter().map(|function| {
            let params = function
                .params
                .iter()
                .map(core_param_contract_text)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "function={}/{} public={} params={} return={} return_core={}",
                function.name,
                function.arity,
                function.public,
                params,
                function.return_type,
                core_type_contract_text(function.core_return_type.as_ref())
            )
        }));
        lines.extend(self.functions.iter().flat_map(|function| {
            function
                .clauses
                .iter()
                .enumerate()
                .map(move |(index, clause)| {
                    format!(
                        "function_clause={}/{}#{} patterns={} core_patterns={} pattern_proof={} pattern_preservation={} guard={} body={}",
                        function.name,
                        function.arity,
                        index,
                        clause.patterns.join(","),
                        clause
                            .core_patterns
                            .iter()
                            .map(|pattern| pattern
                                .as_ref()
                                .map(CorePattern::contract_text)
                                .unwrap_or_else(|| "unsupported".to_string()))
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .pattern_proof_coverage
                            .iter()
                            .map(CoreProofCoverage::as_str)
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .pattern_checked_preservation_evidence
                            .iter()
                            .map(|evidence| evidence
                                .as_ref()
                                .map(CoreCheckedPreservationEvidence::contract_text)
                                .unwrap_or_else(|| "none".to_string()))
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .guard
                            .as_ref()
                            .map(core_expr_summary_text)
                            .unwrap_or_else(|| "none".to_string()),
                        core_expr_summary_text(&clause.body)
                    )
                })
        }));
        lines.extend(self.constructors.iter().map(|constructor| {
            let params = constructor
                .params
                .iter()
                .map(core_param_contract_text)
                .collect::<Vec<_>>()
                .join(",");
            let vararg = constructor
                .vararg
                .as_ref()
                .map(core_param_contract_text)
                .unwrap_or_else(|| "none".to_string());
            format!(
                "constructor={} public={} min_arity={} params={} vararg={} return={} return_core={}",
                constructor.name,
                constructor.public,
                constructor.min_arity,
                params,
                vararg,
                constructor.return_type,
                core_type_contract_text(constructor.core_return_type.as_ref())
            )
        }));
        lines.extend(self.trait_conformances.iter().map(|conformance| {
            format!(
                "trait_conformance={} for={} source={:?} public={}",
                conformance.trait_ref, conformance.for_type, conformance.source, conformance.public
            )
        }));
        lines.push(format!(
            "metadata=functions:{} types:{} constructors:{} proof_readiness:{} proof_expr_lean:{} proof_expr_partial:{} proof_expr_model_required:{} proof_expr_runtime_boundary:{} proof_expr_artifact_only:{} proof_pattern_lean:{} proof_pattern_partial:{} proof_pattern_model_required:{} proof_pattern_runtime_boundary:{} proof_pattern_artifact_only:{} typed_core_expr:{} summary_only_expr:{} typed_core_pattern:{} summary_only_pattern:{} typed_core_type:{} summary_only_type:{} checked_preservation_expr:{} checked_preservation_pattern:{} checked_preservation_expr_structural:{} checked_preservation_pattern_structural:{} checked_preservation_expr_no_runtime_bindings:{} checked_preservation_pattern_no_runtime_bindings:{} checked_preservation_expr_runtime_bindings_required:{} checked_preservation_pattern_runtime_bindings_required:{} resolved_constructor_call_identity:{} resolved_constructor_chain_identity:{} resolved_constructor_pattern_identity:{} unresolved_constructor_call_candidate:{} unresolved_constructor_chain_candidate:{} unresolved_constructor_pattern_candidate:{}",
            self.metadata.interface_function_count,
            self.metadata.interface_type_count,
            self.metadata.constructor_count,
            self.metadata.proof_readiness.as_str(),
            self.metadata.lean_covered_expr_count,
            self.metadata.partial_expr_count,
            self.metadata.proof_model_required_expr_count,
            self.metadata.runtime_boundary_expr_count,
            self.metadata.artifact_only_expr_count,
            self.metadata.lean_covered_pattern_count,
            self.metadata.partial_pattern_count,
            self.metadata.proof_model_required_pattern_count,
            self.metadata.runtime_boundary_pattern_count,
            self.metadata.artifact_only_pattern_count,
            self.metadata.typed_core_expr_count,
            self.metadata.summary_only_expr_count,
            self.metadata.typed_core_pattern_count,
            self.metadata.summary_only_pattern_count,
            self.metadata.typed_core_type_count,
            self.metadata.summary_only_type_count,
            self.metadata.checked_preservation_expr_count,
            self.metadata.checked_preservation_pattern_count,
            self.metadata.checked_preservation_expr_structural_count,
            self.metadata.checked_preservation_pattern_structural_count,
            self.metadata
                .checked_preservation_expr_no_runtime_bindings_count,
            self.metadata
                .checked_preservation_pattern_no_runtime_bindings_count,
            self.metadata
                .checked_preservation_expr_runtime_bindings_required_count,
            self.metadata
                .checked_preservation_pattern_runtime_bindings_required_count,
            self.metadata.resolved_constructor_call_identity_count,
            self.metadata.resolved_constructor_chain_identity_count,
            self.metadata.resolved_constructor_pattern_identity_count,
            self.metadata.unresolved_constructor_call_candidate_count,
            self.metadata.unresolved_constructor_chain_candidate_count,
            self.metadata.unresolved_constructor_pattern_candidate_count
        ));
        lines.join("\n")
    }
}

/// Renders a CoreIR expression summary as deterministic compact text.
///
/// Inputs:
/// - `expr`: Core expression summary.
///
/// Output:
/// - Stable text for snapshots and contract summaries.
///
/// Transformation:
/// - Combines expression kind, optional identity fields, arity, and child
///   summaries into a compact backend-neutral string.
fn core_expr_summary_text(expr: &CoreExprSummary) -> String {
    let mut parts = vec![expr.kind.clone()];
    if let Some(core_expr) = &expr.core_expr {
        parts.push(format!("core={}", core_expr.contract_text()));
    }
    if let Some(evidence) = &expr.checked_preservation_evidence {
        parts.push(format!("preservation={}", evidence.contract_text()));
    }
    parts.push(format!("proof={}", expr.proof_coverage.as_str()));
    if let Some(remote) = &expr.remote {
        parts.push(format!("remote={}", remote));
    }
    if let Some(text) = &expr.text {
        parts.push(format!("text={}", text));
    }
    if let Some(operator) = &expr.operator {
        parts.push(format!("op={}", operator));
    }
    parts.push(format!("arity={}", expr.arity));
    if !expr.children.is_empty() {
        parts.push(format!(
            "children=[{}]",
            expr.children
                .iter()
                .map(core_expr_summary_text)
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    parts.join(":")
}
