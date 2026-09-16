mod contract_text;
mod function_source;
mod intrinsics;
mod module;
mod patterns;
mod proof_payloads;
mod termination;
mod types;
mod visit;
pub(crate) use visit::visit_core_expr_mut;

pub use function_source::CoreFunctionSource;
pub use intrinsics::{
    CoreEffectSet, CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic,
    CoreRuntimeCapability,
};
pub use module::{CoreModule, CoreModuleMetadata};
pub use patterns::{
    CoreBinaryPatternDescriptor, CoreBinaryPatternEndian, CoreBinaryPatternField,
    CoreMapPatternField, CorePattern, CoreRecordPatternField, CoreStringPatternCapture,
    CoreStringPatternSegment,
};
pub use proof_payloads::{
    CoreCheckedPreservationEvidence, CoreCheckedPreservationEvidenceKind, CoreProofCoverage,
    CoreProofReadiness, CoreSubstitutionFreshnessEvidence,
};
pub(crate) use termination::analyze_core_function_termination;
pub use termination::{
    analyze_core_termination, validate_core_termination_evidence, CoreActorBehavior,
    CoreDecreaseKind, CoreFunctionTerminationEvidence, CoreProductivityBoundary,
    CoreRecursiveCallEvidence, CoreTerminationEvidence, CoreTerminationReason,
    CoreTerminationState,
};
pub(crate) use types::{
    atom_type_literal_payload, core_type_contract_text, core_type_from_body_variants,
    core_type_from_text,
};
pub use types::{CoreMapTypeField, CoreStructTypeField, CoreTupleTypeElem, CoreType};

pub const CORE_IR_SCHEMA: &str = "terlan.core_ir.v1";

/// One checked property accepted by an external template render plan.
///
/// Inputs: syntax-output template declaration metadata after typechecking.
/// Output: canonical prop name, CoreIR type, and optional lowered default.
/// Transformation: removes parser-owned type/default wrappers before backend
/// admission while preserving declaration order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreTemplateProp {
    /// Source-visible property name.
    pub name: String,
    /// Checked backend-neutral property type.
    pub ty: CoreType,
    /// Optional default expression lowered into CoreIR.
    pub default: Option<CoreExpr>,
}

/// One validated expression island retained by an external template plan.
///
/// Inputs: a non-path interpolation accepted by template typechecking.
/// Output: source identity, lowered CoreIR expression, and checked result type.
/// Transformation: removes template parser context while preserving the exact
/// executable expression and scalar rendering contract.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreTemplateExpression {
    /// Canonical expression source used by parsed template slots.
    pub source: String,
    /// Backend-neutral checked expression body.
    pub expr: CoreExpr,
    /// Checked scalar result type used to select rendering operations.
    pub ty: CoreType,
}

/// Validated external template tree retained at the CoreIR boundary.
///
/// Inputs: one checked template declaration and its parsed external file.
/// Output: immutable render plan consumed by target-specific lowering.
/// Transformation: binds declaration props to a parser-independent template
/// tree so backends never reopen or parse source templates at runtime.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreTemplateRenderPlan {
    /// Local template declaration name used by `TemplateInstantiate`.
    pub name: String,
    /// Source-relative template path retained for deterministic diagnostics.
    pub source_path: String,
    /// Declaration-order checked property contracts.
    pub props: Vec<CoreTemplateProp>,
    /// Deterministically ordered checked expression islands.
    pub expressions: Vec<CoreTemplateExpression>,
    /// Validated parsed HTML tree.
    pub template: crate::terlan_html::HtmlTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Source identity attached to a CoreIR module.
///
/// Inputs: typed phase source metadata. Output: stable source-kind, optional
/// path, and syntax fingerprint fields. Transformation: carries provenance
/// into CoreIR without embedding parser or backend state.
pub struct CoreSourceIdentity {
    pub source_kind: String,
    pub source_path: Option<String>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoreImportKind {
    Module,
    TypeModule,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Import preserved at the backend-neutral CoreIR boundary.
///
/// Inputs: resolved import metadata. Output: module name plus import kind.
/// Transformation: records only the target-neutral import classification.
pub struct CoreImport {
    pub module: String,
    pub kind: CoreImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Export preserved in CoreIR.
///
/// Inputs: resolved public declaration metadata. Output: export name and kind.
/// Transformation: records declaration visibility without backend export
/// syntax.
pub struct CoreExport {
    pub name: String,
    pub kind: CoreExportKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Core visibility for declarations.
///
/// Inputs: source visibility and opacity modifiers. Output: public/private/
/// opaque tag. Transformation: normalizes visibility for backend validation.
pub enum CoreVisibility {
    Public,
    Private,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Function declaration summarized in CoreIR.
///
/// Inputs: resolved function signature and clauses. Output: typed function
/// summary. Transformation: preserves params, return type, visibility, and
/// clause summaries in backend-neutral form.
pub struct CoreFunction {
    pub name: String,
    /// Source declaration retained independently of generated symbol spelling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CoreFunctionSource>,
    /// Checked trait method implemented by this concrete callable body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trait_method: Option<CoreTraitMethodIdentity>,
    pub arity: usize,
    pub public: bool,
    /// Source-declared generic parameters retained for exact AOT monomorphization.
    pub generic_params: Vec<String>,
    /// Explicit package/native operation selected by `@compiler.native`.
    pub native_operation: Option<String>,
    pub params: Vec<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
    pub clauses: Vec<CoreFunctionClause>,
}

/// Canonical trait identity retained independently of generated function names.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreTraitMethodIdentity {
    pub trait_name: String,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Typed backend-neutral Core expression.
///
/// Inputs: syntax-output expressions after resolver/typechecker lowering.
/// Output: structured expression tree. Transformation: removes parser-specific
/// detail while preserving semantics needed by proof, validation, and backend
/// emitters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        generators: Vec<CoreListComprehensionGenerator>,
        guards: Vec<CoreExpr>,
        lift: Option<String>,
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
        /// Explicit source type arguments retained until monomorphization.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<CoreType>,
        args: Vec<CoreExpr>,
    },
    ConstructorCall {
        constructor: String,
        constructor_identity: Option<String>,
        args: Vec<CoreExpr>,
    },
    Call {
        function: String,
        /// Explicit source type arguments retained until monomorphization.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<CoreType>,
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
    SqlQuery {
        row_type: String,
        bound_sql: String,
        parameters: Vec<CoreExpr>,
        query_kind: String,
        transaction_requirement: String,
        cardinality: String,
        result_type: String,
        /// Typed result retained for alias and nominal-identity resolution.
        result_core_type: CoreType,
        projection_fields: Vec<String>,
    },
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
        /// Explicit source annotations, aligned with parameters when present.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameter_types: Vec<Option<CoreType>>,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Core let binding.
///
/// Inputs: binding pattern and value expression. Output: one local binding.
/// Transformation: represents source let bindings in expression form for
/// backend-neutral lowering.
pub struct CoreLetBinding {
    pub pattern: CorePattern,
    pub value: CoreExpr,
}

/// One ordered generator in a CoreIR list comprehension.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreListComprehensionGenerator {
    pub pattern: CorePattern,
    pub source: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Core if branch.
///
/// Inputs: condition and body expressions. Output: typed if clause.
/// Transformation: stores predicate/body pairs in source order.
pub struct CoreIfClause {
    pub condition: CoreExpr,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Core try cleanup branch.
///
/// Inputs: cleanup trigger and body expressions. Output: typed after branch.
/// Transformation: keeps cleanup semantics explicit for target lowering.
pub struct CoreTryAfter {
    pub trigger: Box<CoreExpr>,
    pub body: Box<CoreExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Constructor declaration summarized in CoreIR.
///
/// Inputs: resolved constructor declaration. Output: constructor signature.
/// Transformation: records public flag, fixed params, optional vararg, return
/// type, typed return shape, and ordinary callable identities for executable bodies.
pub struct CoreConstructorDecl {
    /// Checked source implementation; absent only for layout-only declarations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<CoreConstructorImplementation>,
    pub name: String,
    pub public: bool,
    pub min_arity: usize,
    pub params: Vec<CoreParam>,
    pub vararg: Option<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
}

/// Ordinary typed callable identities implementing a source constructor clause.
///
/// Bodies and default expressions live in CoreModule.functions so existing
/// type substitution, proof evidence and effect analysis visit them normally.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreConstructorImplementation {
    /// Provider-local callable receiving fixed arguments and one packed vararg list.
    pub function: String,
    /// Provider-local default callables, indexed by fixed parameter position.
    /// Each receives the preceding parameters exactly once in declaration order.
    pub defaults: Vec<Option<String>>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
///   Struct `includes` clauses are intentionally excluded because they expand
///   struct shape, not trait conformance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreTraitConformance {
    pub trait_ref: String,
    pub for_type: String,
    pub source: CoreTraitConformanceSource,
    pub public: bool,
}
