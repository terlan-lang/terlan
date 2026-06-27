use super::*;
use crate::terlan_hir::ModuleInterface;

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

    /// Returns whether this Core module requires the SQL runtime boundary.
    ///
    /// Inputs:
    /// - `self`: CoreIR module produced by the formal compiler path.
    ///
    /// Output:
    /// - `true` when any function guard/body contains a `CoreExpr::SqlQuery`.
    ///
    /// Transformation:
    /// - Walks CoreIR summaries and typed Core expression payloads without
    ///   reparsing source syntax, so backend runtime helper emission follows
    ///   the formal compiler payload rather than raw macro text.
    pub fn uses_sql_runtime_boundary(&self) -> bool {
        self.functions.iter().any(|function| {
            function.clauses.iter().any(|clause| {
                clause
                    .guard
                    .as_ref()
                    .is_some_and(core_expr_summary_uses_sql_runtime)
                    || core_expr_summary_uses_sql_runtime(&clause.body)
            })
        })
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
            CoreImportKind::TypeModule => format!("import=type:{}", import.module),
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

/// Returns whether a Core expression summary requires the SQL runtime boundary.
///
/// Inputs:
/// - `summary`: expression summary that may contain typed Core payload and
///   child summaries.
///
/// Output:
/// - `true` when this summary or any nested child uses `CoreExpr::SqlQuery`.
///
/// Transformation:
/// - Checks both typed payloads and summary children so partially covered
///   expressions still preserve runtime-helper discovery.
fn core_expr_summary_uses_sql_runtime(summary: &CoreExprSummary) -> bool {
    summary
        .core_expr
        .as_ref()
        .is_some_and(core_expr_uses_sql_runtime)
        || summary
            .children
            .iter()
            .any(core_expr_summary_uses_sql_runtime)
}

/// Returns whether a typed Core expression requires the SQL runtime boundary.
///
/// Inputs:
/// - `expr`: typed Core expression payload.
///
/// Output:
/// - `true` when the expression is or contains `CoreExpr::SqlQuery`.
///
/// Transformation:
/// - Recursively walks container, call, control-flow, and operator expression
///   children while treating scalar/runtime-boundary leaves as terminal.
fn core_expr_uses_sql_runtime(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::SqlQuery { .. } => true,
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().any(core_expr_uses_sql_runtime)
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .any(|field| core_expr_uses_sql_runtime(&field.value)),
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => fields
            .iter()
            .any(|field| core_expr_uses_sql_runtime(&field.value)),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => core_expr_uses_sql_runtime(head) || core_expr_uses_sql_runtime(tail),
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => core_expr_uses_sql_runtime(base),
        CoreExpr::ListComprehension {
            expr,
            source,
            guard,
            ..
        } => {
            core_expr_uses_sql_runtime(expr)
                || core_expr_uses_sql_runtime(source)
                || guard
                    .as_ref()
                    .is_some_and(|guard| core_expr_uses_sql_runtime(guard))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| core_expr_uses_sql_runtime(&binding.value))
                || core_expr_uses_sql_runtime(body)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().any(core_expr_uses_sql_runtime) || core_expr_uses_sql_runtime(record)
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => args.iter().any(core_expr_uses_sql_runtime),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            core_expr_uses_sql_runtime(receiver) || args.iter().any(core_expr_uses_sql_runtime)
        }
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_uses_sql_runtime(callee) || args.iter().any(core_expr_uses_sql_runtime)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_uses_sql_runtime(scrutinee)
                || clauses.iter().any(core_case_clause_uses_sql_runtime)
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            core_expr_uses_sql_runtime(body)
                || of_clauses.iter().any(core_case_clause_uses_sql_runtime)
                || catch_clauses.iter().any(core_case_clause_uses_sql_runtime)
                || after_clause.as_ref().is_some_and(|clause| {
                    core_expr_uses_sql_runtime(&clause.trigger)
                        || core_expr_uses_sql_runtime(&clause.body)
                })
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            core_expr_uses_sql_runtime(&clause.condition)
                || core_expr_uses_sql_runtime(&clause.body)
        }),
        CoreExpr::Lam { body, .. } => core_expr_uses_sql_runtime(body),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::Intrinsic(_) => false,
    }
}

/// Returns whether a Core case clause requires the SQL runtime boundary.
///
/// Inputs:
/// - `clause`: Core case/try clause.
///
/// Output:
/// - `true` when the guard or body contains `CoreExpr::SqlQuery`.
///
/// Transformation:
/// - Keeps shared SQL-runtime discovery for case, try, and related
///   control-flow forms.
fn core_case_clause_uses_sql_runtime(clause: &CoreCaseClause) -> bool {
    clause
        .guard
        .as_ref()
        .is_some_and(core_expr_uses_sql_runtime)
        || core_expr_uses_sql_runtime(&clause.body)
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
