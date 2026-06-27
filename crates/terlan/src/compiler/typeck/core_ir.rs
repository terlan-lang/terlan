mod intrinsics;
mod module;
mod patterns;
mod proof_payloads;
mod types;

pub use intrinsics::{
    CoreEffectSet, CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic,
    CoreRuntimeCapability,
};
pub use module::{CoreModule, CoreModuleMetadata};
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
    TypeModule,
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

/// Typed backend-neutral Core expression.
///
/// Inputs: syntax-output expressions after resolver/typechecker lowering.
/// Output: structured expression tree. Transformation: removes parser-specific
/// detail while preserving semantics needed by proof, validation, and backend
/// emitters.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    SqlQuery {
        row_type: String,
        bound_sql: String,
        parameter_count: usize,
        cardinality: String,
        result_type: String,
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
/// Inputs: binding pattern and value expression. Output: one local binding.
/// Transformation: represents source let bindings in expression form for
/// backend-neutral lowering.
pub struct CoreLetBinding {
    pub pattern: CorePattern,
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
            CoreExpr::SqlQuery {
                row_type,
                bound_sql,
                parameter_count,
                cardinality,
                result_type,
                projection_fields,
            } => format!(
                "SqlQuery(row_type={row_type};params={parameter_count};cardinality={cardinality};result={result_type};projection={};sql={bound_sql})",
                projection_fields.join(",")
            ),
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
    /// - Serializes the binding pattern and recursively rendered value
    ///   expression without source spans or backend syntax.
    fn contract_text(&self) -> String {
        format!(
            "{}={}",
            self.pattern.contract_text(),
            self.value.contract_text()
        )
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
///   Struct `includes` clauses are intentionally excluded because they expand
///   struct shape, not trait conformance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTraitConformance {
    pub trait_ref: String,
    pub for_type: String,
    pub source: CoreTraitConformanceSource,
    pub public: bool,
}
