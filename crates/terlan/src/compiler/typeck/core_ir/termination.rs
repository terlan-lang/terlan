//! Backend-independent termination and actor-productivity evidence.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    CoreCaseClause, CoreExpr, CoreFunction, CoreIfClause, CoreIntrinsicId, CoreModule, CorePattern,
    CorePrimitiveIntrinsic,
};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

pub const CORE_TERMINATION_EVIDENCE_SCHEMA: &str = "terlan.core_termination.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
/// Variants representing core termination state.
pub enum CoreTerminationState {
    Proven,
    Unproven,
    IntentionalPersistent,
    ProductivePersistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
/// Variants representing core actor behavior.
pub enum CoreActorBehavior {
    NotActor,
    FiniteWorker,
    Persistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
/// Variants representing core termination reason.
pub enum CoreTerminationReason {
    NonRecursive,
    StructuralDescent,
    GuardedIntegerDescent,
    LexicographicDescent,
    MutualSizeChange,
    RecursiveEdgeNotDecreasing,
    UnsupportedRecursiveShape,
    ActorCycleMissingProductivityBoundary,
    ActorCycleProductive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
/// Variants representing core decrease kind.
pub enum CoreDecreaseKind {
    Unknown,
    NonIncreasing,
    Structural,
    GuardedInteger,
}

impl CoreDecreaseKind {
    fn is_strict(self) -> bool {
        matches!(self, Self::Structural | Self::GuardedInteger)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NonIncreasing => "non_increasing",
            Self::Structural => "structural",
            Self::GuardedInteger => "guarded_integer",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
/// Variants representing core productivity boundary.
pub enum CoreProductivityBoundary {
    ReductionSafepoint,
    Receive,
    Yield,
    TimerWait,
    SchedulerHandoff,
    AsyncCapability,
}

impl CoreProductivityBoundary {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReductionSafepoint => "reduction_safepoint",
            Self::Receive => "receive",
            Self::Yield => "yield",
            Self::TimerWait => "timer_wait",
            Self::SchedulerHandoff => "scheduler_handoff",
            Self::AsyncCapability => "async_capability",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Data describing core recursive call evidence.
pub struct CoreRecursiveCallEvidence {
    pub caller: String,
    pub caller_arity: usize,
    pub callee: String,
    pub callee_arity: usize,
    pub argument_relations: Vec<CoreDecreaseKind>,
    pub tail_position: bool,
    pub productivity_boundaries: Vec<CoreProductivityBoundary>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Data describing core function termination evidence.
pub struct CoreFunctionTerminationEvidence {
    pub function: String,
    pub arity: usize,
    pub state: CoreTerminationState,
    pub reason: CoreTerminationReason,
    pub actor_behavior: CoreActorBehavior,
    pub component: Vec<String>,
    pub measure: Vec<usize>,
    pub recursive_calls: Vec<CoreRecursiveCallEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Data describing core termination evidence.
pub struct CoreTerminationEvidence {
    pub schema: String,
    pub functions: Vec<CoreFunctionTerminationEvidence>,
}

impl Default for CoreTerminationEvidence {
    fn default() -> Self {
        Self {
            schema: CORE_TERMINATION_EVIDENCE_SCHEMA.to_string(),
            functions: Vec::new(),
        }
    }
}

impl CoreTerminationEvidence {
    /// Returns function.
    pub fn function(&self, name: &str, arity: usize) -> Option<&CoreFunctionTerminationEvidence> {
        self.functions
            .iter()
            .find(|item| item.function == name && item.arity == arity)
    }

    /// Enforces a total-only compiler context without using a depth heuristic.
    pub fn require_total(&self, name: &str, arity: usize) -> Result<(), BoundaryError> {
        self.require_total_untyped(name, arity).map_err(|error| {
            BoundaryError::message(
                ErrorDomain::CompilerPhase,
                "require total CoreIR function",
                error,
            )
        })
    }

    fn require_total_untyped(&self, name: &str, arity: usize) -> Result<(), String> {
        let evidence = self.function(name, arity).ok_or_else(|| {
            format!("error[termination.evidence_missing]: no evidence for `{name}/{arity}`")
        })?;
        if evidence.state == CoreTerminationState::Proven {
            Ok(())
        } else {
            Err(format!(
                "error[termination.total_required]: `{name}/{arity}` is {:?}: {:?}",
                evidence.state, evidence.reason
            ))
        }
    }

    pub(crate) fn contract_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("termination_schema={}", self.schema)];
        lines.extend(self.functions.iter().map(|item| {
            let measure = item
                .measure
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "termination={}/{} state={:?} reason={:?} actor={:?} component={} measure={}",
                item.function,
                item.arity,
                item.state,
                item.reason,
                item.actor_behavior,
                item.component.join(","),
                measure
            )
        }));
        lines.extend(self.functions.iter().flat_map(|item| {
            item.recursive_calls.iter().map(|edge| {
                let relations = edge
                    .argument_relations
                    .iter()
                    .map(|relation| relation.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                let boundaries = edge
                    .productivity_boundaries
                    .iter()
                    .map(|boundary| boundary.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "termination_edge={}/{}->{}/{} relations={} tail={} productivity={}",
                    edge.caller,
                    edge.caller_arity,
                    edge.callee,
                    edge.callee_arity,
                    relations,
                    edge.tail_position,
                    boundaries
                )
            })
        }));
        lines
    }
}

#[derive(Clone, Copy)]
struct Origin {
    parameter: usize,
    relation: CoreDecreaseKind,
}

#[derive(Default)]
struct AnalysisEnvironment {
    origins: BTreeMap<String, Origin>,
    lower_bounded: BTreeSet<usize>,
    boundaries: BTreeSet<CoreProductivityBoundary>,
}

impl Clone for AnalysisEnvironment {
    fn clone(&self) -> Self {
        Self {
            origins: self.origins.clone(),
            lower_bounded: self.lower_bounded.clone(),
            boundaries: self.boundaries.clone(),
        }
    }
}

#[derive(Default)]
struct FunctionFacts {
    calls: Vec<CoreRecursiveCallEvidence>,
    actor_operations: bool,
}

/// Analyzes core termination.
pub fn analyze_core_termination(module: &CoreModule) -> CoreTerminationEvidence {
    analyze_core_function_termination(&module.functions)
}

pub(crate) fn analyze_core_function_termination(
    functions: &[CoreFunction],
) -> CoreTerminationEvidence {
    let keys = functions.iter().map(function_key).collect::<Vec<_>>();
    let known = keys.iter().cloned().collect::<BTreeSet<_>>();
    let facts = functions
        .iter()
        .map(|function| analyze_function(function, &known))
        .collect::<Vec<_>>();
    let reachability = call_reachability(&keys, &facts);
    let mut evidence = Vec::with_capacity(functions.len());

    for (index, function) in functions.iter().enumerate() {
        let component_indices = (0..functions.len())
            .filter(|other| reachability[index][*other] && reachability[*other][index])
            .collect::<Vec<_>>();
        let component_keys = component_indices
            .iter()
            .map(|member| keys[*member].clone())
            .collect::<BTreeSet<_>>();
        let recursive = component_indices.len() > 1
            || facts[index]
                .calls
                .iter()
                .any(|edge| edge.callee == function.name && edge.callee_arity == function.arity);
        let mut recursive_calls = component_indices
            .iter()
            .flat_map(|member| facts[*member].calls.iter())
            .filter(|edge| component_keys.contains(&(edge.callee.clone(), edge.callee_arity)))
            .cloned()
            .collect::<Vec<_>>();
        recursive_calls.sort_by(edge_order);
        let actor_cycle = component_indices
            .iter()
            .any(|member| facts[*member].actor_operations);
        let (state, reason, measure) = classify_component(
            recursive,
            actor_cycle,
            component_indices.len(),
            function.arity,
            &recursive_calls,
        );
        evidence.push(CoreFunctionTerminationEvidence {
            function: function.name.clone(),
            arity: function.arity,
            state,
            reason,
            actor_behavior: if actor_cycle {
                CoreActorBehavior::Persistent
            } else if facts[index].actor_operations {
                CoreActorBehavior::FiniteWorker
            } else {
                CoreActorBehavior::NotActor
            },
            component: component_indices
                .iter()
                .map(|member| format!("{}/{}", keys[*member].0, keys[*member].1))
                .collect(),
            measure,
            recursive_calls,
        });
    }
    evidence.sort_by(|left, right| {
        left.function
            .cmp(&right.function)
            .then_with(|| left.arity.cmp(&right.arity))
    });
    CoreTerminationEvidence {
        schema: CORE_TERMINATION_EVIDENCE_SCHEMA.to_string(),
        functions: evidence,
    }
}

/// Rejects missing, stale, or forged evidence by recomputing it from CoreIR.
pub fn validate_core_termination_evidence(module: &CoreModule) -> Result<(), BoundaryError> {
    validate_core_termination_evidence_untyped(module).map_err(|error| {
        BoundaryError::message(
            ErrorDomain::CompilerPhase,
            "validate CoreIR termination evidence",
            error,
        )
    })
}

fn validate_core_termination_evidence_untyped(module: &CoreModule) -> Result<(), String> {
    let expected = analyze_core_termination(module);
    if module.termination == expected {
        Ok(())
    } else {
        Err(
            "error[termination.evidence_invalid]: attached evidence does not match checked CoreIR"
                .to_string(),
        )
    }
}

fn function_key(function: &CoreFunction) -> (String, usize) {
    (function.name.clone(), function.arity)
}

fn analyze_function(function: &CoreFunction, known: &BTreeSet<(String, usize)>) -> FunctionFacts {
    let mut facts = FunctionFacts::default();
    for clause in &function.clauses {
        let mut environment = AnalysisEnvironment::default();
        for (index, param) in function.params.iter().enumerate() {
            environment.origins.insert(
                param.name.clone(),
                Origin {
                    parameter: index,
                    relation: CoreDecreaseKind::NonIncreasing,
                },
            );
        }
        for (index, pattern) in clause.core_patterns.iter().enumerate() {
            if let Some(pattern) = pattern {
                bind_pattern(
                    pattern,
                    index,
                    CoreDecreaseKind::NonIncreasing,
                    &mut environment,
                );
            }
        }
        if let Some(guard) = clause
            .guard
            .as_ref()
            .and_then(|guard| guard.core_expr.as_ref())
        {
            add_integer_constraints(guard, &mut environment);
            collect_expr(
                guard,
                function,
                known,
                &mut environment.clone(),
                &mut facts,
                false,
            );
        }
        if let Some(body) = clause.body.core_expr.as_ref() {
            collect_expr(body, function, known, &mut environment, &mut facts, true);
        }
    }
    facts
}

fn bind_pattern(
    pattern: &CorePattern,
    parameter: usize,
    relation: CoreDecreaseKind,
    environment: &mut AnalysisEnvironment,
) {
    match pattern {
        CorePattern::Var(name) => {
            environment.origins.insert(
                name.clone(),
                Origin {
                    parameter,
                    relation,
                },
            );
        }
        CorePattern::Alias { alias, pattern } => {
            environment.origins.insert(
                alias.clone(),
                Origin {
                    parameter,
                    relation,
                },
            );
            bind_pattern(pattern, parameter, relation, environment);
        }
        CorePattern::ListCons { head, tail } => {
            bind_pattern(head, parameter, CoreDecreaseKind::Structural, environment);
            bind_pattern(tail, parameter, CoreDecreaseKind::Structural, environment);
        }
        CorePattern::Constructor { args, .. } => {
            for argument in args {
                bind_pattern(
                    argument,
                    parameter,
                    CoreDecreaseKind::Structural,
                    environment,
                );
            }
        }
        _ => {}
    }
}

fn collect_expr(
    expr: &CoreExpr,
    caller: &CoreFunction,
    known: &BTreeSet<(String, usize)>,
    environment: &mut AnalysisEnvironment,
    facts: &mut FunctionFacts,
    tail_position: bool,
) {
    match expr {
        CoreExpr::Call { function, args } => {
            for argument in args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
            let key = (function.clone(), args.len());
            if known.contains(&key) {
                let mut boundaries = environment.boundaries.clone();
                if tail_position {
                    boundaries.insert(CoreProductivityBoundary::ReductionSafepoint);
                }
                facts.calls.push(CoreRecursiveCallEvidence {
                    caller: caller.name.clone(),
                    caller_arity: caller.arity,
                    callee: function.clone(),
                    callee_arity: args.len(),
                    argument_relations: args
                        .iter()
                        .enumerate()
                        .map(|(index, argument)| argument_relation(argument, index, environment))
                        .collect(),
                    tail_position,
                    productivity_boundaries: boundaries.into_iter().collect(),
                });
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_expr(&binding.value, caller, known, environment, facts, false);
                if let CoreExpr::Var(name) = &binding.value {
                    if let Some(origin) = environment.origins.get(name).copied() {
                        bind_pattern(
                            &binding.pattern,
                            origin.parameter,
                            origin.relation,
                            environment,
                        );
                    }
                }
            }
            collect_expr(body, caller, known, environment, facts, tail_position);
        }
        CoreExpr::Case { scrutinee, clauses } => {
            collect_expr(scrutinee, caller, known, environment, facts, false);
            for clause in clauses {
                collect_case_clause(
                    clause,
                    scrutinee,
                    caller,
                    known,
                    environment,
                    facts,
                    tail_position,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_if_clause(clause, caller, known, environment, facts, tail_position);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_expr(body, caller, known, environment, facts, false);
            for clause in of_clauses.iter().chain(catch_clauses) {
                collect_case_clause(clause, body, caller, known, environment, facts, false);
            }
            if let Some(after) = after_clause {
                collect_expr(&after.trigger, caller, known, environment, facts, false);
                collect_expr(&after.body, caller, known, environment, facts, false);
            }
        }
        CoreExpr::Intrinsic(call) => {
            for argument in &call.args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
            if let Some(boundary) = intrinsic_boundary(&call.id) {
                facts.actor_operations = true;
                environment.boundaries.insert(boundary);
            } else if intrinsic_is_actor_operation(&call.id) {
                facts.actor_operations = true;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                collect_expr(item, caller, known, environment, facts, false);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            collect_expr(head, caller, known, environment, facts, false);
            collect_expr(tail, caller, known, environment, facts, false);
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            collect_expr(left, caller, known, environment, facts, false);
            let right_is_tail = tail_position && matches!(operator.as_str(), "and" | "or");
            collect_expr(right, caller, known, environment, facts, right_is_tail);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            for generator in generators {
                collect_expr(&generator.source, caller, known, environment, facts, false);
            }
            for guard in guards {
                collect_expr(guard, caller, known, environment, facts, false);
            }
            collect_expr(expr, caller, known, environment, facts, false);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                collect_expr(&field.value, caller, known, environment, facts, false);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                collect_expr(&field.value, caller, known, environment, facts, false);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            collect_expr(base, caller, known, environment, facts, false);
            for field in fields {
                collect_expr(&field.value, caller, known, environment, facts, false);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            collect_expr(base, caller, known, environment, facts, false);
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for argument in args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
            collect_expr(record, caller, known, environment, facts, false);
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            for argument in args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_expr(receiver, caller, known, environment, facts, false);
            for argument in args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_expr(callee, caller, known, environment, facts, false);
            for argument in args {
                collect_expr(argument, caller, known, environment, facts, false);
            }
        }
        CoreExpr::Lam { .. } => {}
        CoreExpr::SqlQuery { parameters, .. } => {
            environment
                .boundaries
                .insert(CoreProductivityBoundary::AsyncCapability);
            for parameter in parameters {
                collect_expr(parameter, caller, known, environment, facts, false);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn collect_case_clause(
    clause: &CoreCaseClause,
    scrutinee: &CoreExpr,
    caller: &CoreFunction,
    known: &BTreeSet<(String, usize)>,
    environment: &AnalysisEnvironment,
    facts: &mut FunctionFacts,
    tail_position: bool,
) {
    let mut branch = environment.clone();
    if let CoreExpr::Var(name) = scrutinee {
        if let Some(origin) = branch.origins.get(name).copied() {
            bind_pattern(
                &clause.pattern,
                origin.parameter,
                origin.relation,
                &mut branch,
            );
        }
    }
    if let Some(guard) = &clause.guard {
        add_integer_constraints(guard, &mut branch);
        collect_expr(guard, caller, known, &mut branch, facts, false);
    }
    collect_expr(
        &clause.body,
        caller,
        known,
        &mut branch,
        facts,
        tail_position,
    );
}

fn collect_if_clause(
    clause: &CoreIfClause,
    caller: &CoreFunction,
    known: &BTreeSet<(String, usize)>,
    environment: &AnalysisEnvironment,
    facts: &mut FunctionFacts,
    tail_position: bool,
) {
    let mut branch = environment.clone();
    add_integer_constraints(&clause.condition, &mut branch);
    collect_expr(&clause.condition, caller, known, &mut branch, facts, false);
    collect_expr(
        &clause.body,
        caller,
        known,
        &mut branch,
        facts,
        tail_position,
    );
}

fn add_integer_constraints(expr: &CoreExpr, environment: &mut AnalysisEnvironment) {
    let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = expr
    else {
        return;
    };
    if operator == "and" {
        add_integer_constraints(left, environment);
        add_integer_constraints(right, environment);
        return;
    }
    let lower_bounded_name = match (operator.as_str(), left.as_ref(), right.as_ref()) {
        (">" | ">=", CoreExpr::Var(name), CoreExpr::Int(_)) => Some(name),
        ("<" | "<=", CoreExpr::Int(_), CoreExpr::Var(name)) => Some(name),
        _ => None,
    };
    if let Some(name) = lower_bounded_name {
        if let Some(origin) = environment.origins.get(name) {
            environment.lower_bounded.insert(origin.parameter);
        }
    }
}

fn argument_relation(
    expr: &CoreExpr,
    target_parameter: usize,
    environment: &AnalysisEnvironment,
) -> CoreDecreaseKind {
    if let CoreExpr::Var(name) = expr {
        return environment
            .origins
            .get(name)
            .filter(|origin| origin.parameter == target_parameter)
            .map_or(CoreDecreaseKind::Unknown, |origin| origin.relation);
    }
    let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = expr
    else {
        return CoreDecreaseKind::Unknown;
    };
    let (name, decreasing) = match (operator.as_str(), left.as_ref(), right.as_ref()) {
        ("-", CoreExpr::Var(name), CoreExpr::Int(amount)) if *amount > 0 => (name, true),
        ("+", CoreExpr::Var(name), CoreExpr::Int(amount)) if *amount < 0 => (name, true),
        _ => return CoreDecreaseKind::Unknown,
    };
    let Some(origin) = environment.origins.get(name) else {
        return CoreDecreaseKind::Unknown;
    };
    if origin.parameter == target_parameter
        && decreasing
        && environment.lower_bounded.contains(&origin.parameter)
    {
        CoreDecreaseKind::GuardedInteger
    } else {
        CoreDecreaseKind::Unknown
    }
}

fn intrinsic_is_actor_operation(id: &CoreIntrinsicId) -> bool {
    match id {
        CoreIntrinsicId::VmProcessSendMessage(_)
        | CoreIntrinsicId::VmProcessReceiveMessage(_)
        | CoreIntrinsicId::VmProcessSpawn(_)
        | CoreIntrinsicId::VmProcessCurrent(_)
        | CoreIntrinsicId::VmProcessLink(_)
        | CoreIntrinsicId::VmProcessMonitor(_)
        | CoreIntrinsicId::VmProcessAcquireResource(_)
        | CoreIntrinsicId::VmProcessCancel(_) => true,
        CoreIntrinsicId::Primitive(primitive) => matches!(
            primitive,
            CorePrimitiveIntrinsic::VmProcessYield
                | CorePrimitiveIntrinsic::VmProcessSendInt
                | CorePrimitiveIntrinsic::VmProcessReceiveInt
                | CorePrimitiveIntrinsic::VmProcessSendString
                | CorePrimitiveIntrinsic::VmProcessReceiveString
                | CorePrimitiveIntrinsic::VmProcessSendBytes
                | CorePrimitiveIntrinsic::VmProcessReceiveBytes
                | CorePrimitiveIntrinsic::VmProcessSendBinary
                | CorePrimitiveIntrinsic::VmProcessReceiveBinary
                | CorePrimitiveIntrinsic::VmProcessSendAtom
                | CorePrimitiveIntrinsic::VmProcessReceiveAtom
                | CorePrimitiveIntrinsic::VmProcessSleep
                | CorePrimitiveIntrinsic::VmProcessFail
                | CorePrimitiveIntrinsic::VmProcessSchedule
        ),
        CoreIntrinsicId::MemoryLayoutOf(_)
        | CoreIntrinsicId::MemoryShallowSize(_)
        | CoreIntrinsicId::MemoryRetainedSize(_)
        | CoreIntrinsicId::VmProcessEntry(_)
        | CoreIntrinsicId::NativeOperation { .. }
        | CoreIntrinsicId::Runtime(_) => false,
    }
}

fn intrinsic_boundary(id: &CoreIntrinsicId) -> Option<CoreProductivityBoundary> {
    match id {
        CoreIntrinsicId::VmProcessReceiveMessage(_) => Some(CoreProductivityBoundary::Receive),
        CoreIntrinsicId::VmProcessAcquireResource(_)
        | CoreIntrinsicId::NativeOperation { .. }
        | CoreIntrinsicId::Runtime(_) => Some(CoreProductivityBoundary::AsyncCapability),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessYield) => {
            Some(CoreProductivityBoundary::Yield)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSleep) => {
            Some(CoreProductivityBoundary::TimerWait)
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::VmProcessReceiveInt
            | CorePrimitiveIntrinsic::VmProcessReceiveString
            | CorePrimitiveIntrinsic::VmProcessReceiveBytes
            | CorePrimitiveIntrinsic::VmProcessReceiveBinary
            | CorePrimitiveIntrinsic::VmProcessReceiveAtom,
        ) => Some(CoreProductivityBoundary::Receive),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSchedule) => {
            Some(CoreProductivityBoundary::SchedulerHandoff)
        }
        _ => None,
    }
}

fn call_reachability(keys: &[(String, usize)], facts: &[FunctionFacts]) -> Vec<Vec<bool>> {
    let indices = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = vec![vec![false; keys.len()]; keys.len()];
    for (caller, fact) in facts.iter().enumerate() {
        for edge in &fact.calls {
            if let Some(callee) = indices.get(&(edge.callee.clone(), edge.callee_arity)) {
                reachable[caller][*callee] = true;
            }
        }
    }
    for via in 0..keys.len() {
        for from in 0..keys.len() {
            for to in 0..keys.len() {
                reachable[from][to] |= reachable[from][via] && reachable[via][to];
            }
        }
    }
    reachable
}

fn classify_component(
    recursive: bool,
    actor_cycle: bool,
    component_len: usize,
    arity: usize,
    edges: &[CoreRecursiveCallEvidence],
) -> (CoreTerminationState, CoreTerminationReason, Vec<usize>) {
    if !recursive {
        return (
            CoreTerminationState::Proven,
            CoreTerminationReason::NonRecursive,
            Vec::new(),
        );
    }
    if actor_cycle {
        if !edges.is_empty()
            && edges
                .iter()
                .all(|edge| !edge.productivity_boundaries.is_empty())
        {
            return (
                CoreTerminationState::ProductivePersistent,
                CoreTerminationReason::ActorCycleProductive,
                Vec::new(),
            );
        }
        return (
            CoreTerminationState::IntentionalPersistent,
            CoreTerminationReason::ActorCycleMissingProductivityBoundary,
            Vec::new(),
        );
    }
    let Some(measure) = find_lexicographic_measure(arity, edges) else {
        let reason = if edges
            .iter()
            .any(|edge| edge.argument_relations.iter().any(|item| item.is_strict()))
        {
            CoreTerminationReason::RecursiveEdgeNotDecreasing
        } else {
            CoreTerminationReason::UnsupportedRecursiveShape
        };
        return (CoreTerminationState::Unproven, reason, Vec::new());
    };
    let strict_kinds = edges
        .iter()
        .flat_map(|edge| edge.argument_relations.iter().copied())
        .filter(|relation| relation.is_strict())
        .collect::<BTreeSet<_>>();
    let reason = if component_len > 1 {
        CoreTerminationReason::MutualSizeChange
    } else if measure.len() > 1 {
        CoreTerminationReason::LexicographicDescent
    } else if strict_kinds == BTreeSet::from([CoreDecreaseKind::Structural]) {
        CoreTerminationReason::StructuralDescent
    } else {
        CoreTerminationReason::GuardedIntegerDescent
    };
    (CoreTerminationState::Proven, reason, measure)
}

fn find_lexicographic_measure(
    arity: usize,
    edges: &[CoreRecursiveCallEvidence],
) -> Option<Vec<usize>> {
    if edges.is_empty()
        || edges
            .iter()
            .any(|edge| edge.argument_relations.len() != arity)
    {
        return None;
    }
    for index in 0..arity {
        let singleton = [index];
        if lexicographically_decreases(&singleton, edges) {
            return Some(singleton.to_vec());
        }
    }
    let mut order = (0..arity).collect::<Vec<_>>();
    if lexicographically_decreases(&order, edges) {
        return Some(order);
    }
    if arity <= 7 && permute_measure(0, &mut order, edges) {
        return Some(order);
    }
    None
}

fn permute_measure(start: usize, order: &mut [usize], edges: &[CoreRecursiveCallEvidence]) -> bool {
    if start == order.len() {
        return lexicographically_decreases(order, edges);
    }
    for index in start..order.len() {
        order.swap(start, index);
        if permute_measure(start + 1, order, edges) {
            return true;
        }
        order.swap(start, index);
    }
    false
}

fn lexicographically_decreases(order: &[usize], edges: &[CoreRecursiveCallEvidence]) -> bool {
    edges.iter().all(|edge| {
        order.iter().any(|index| {
            let relation = edge.argument_relations[*index];
            relation.is_strict()
                && order
                    .iter()
                    .take_while(|candidate| *candidate != index)
                    .all(|previous| {
                        edge.argument_relations[*previous] == CoreDecreaseKind::NonIncreasing
                    })
        })
    })
}

fn edge_order(
    left: &CoreRecursiveCallEvidence,
    right: &CoreRecursiveCallEvidence,
) -> std::cmp::Ordering {
    left.caller
        .cmp(&right.caller)
        .then_with(|| left.caller_arity.cmp(&right.caller_arity))
        .then_with(|| left.callee.cmp(&right.callee))
        .then_with(|| left.callee_arity.cmp(&right.callee_arity))
        .then_with(|| left.argument_relations.cmp(&right.argument_relations))
}

#[cfg(test)]
#[path = "termination_test.rs"]
#[cfg(test)]
mod termination_test;
