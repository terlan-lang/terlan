//! Admission and evaluation-context extraction for bounded suspending calls.

use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::{
    contains_process_yield, expr_calls_suspending, free_variables, is_process_transition,
    NativeContinuation, NativeExpr, NativeType,
};

mod analysis;
mod contracts;
mod region;

pub(super) use analysis::*;
#[cfg(test)]
pub(super) use contracts::validate_call_then_contracts;
pub(super) use contracts::{
    refresh_recursive_call_contract, validate_call_then_contracts_with_destinations,
};
pub(super) use region::composed_call_region;

// Large generated services and typed repository tools can legitimately compose
// several thousand suspension sites. Keep a finite compiler resource budget
// against pathological graphs while admitting the measured self-validation
// workload with greater than two-times headroom.
const MAX_COMPOSED_CALL_CONTINUATIONS: usize = 16_384;

pub(super) fn suspending_call_count(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
) -> usize {
    let own = match expr {
        CoreExpr::Call { function, args } => {
            usize::from(suspending.contains(&(function.clone(), args.len())))
        }
        CoreExpr::FunctionCall { .. } => 1,
        _ => 0,
    };
    own + match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .map(|arg| suspending_call_count(arg, suspending))
            .sum(),
        CoreExpr::FunctionCall { callee, args } => {
            suspending_call_count(callee, suspending)
                + args
                    .iter()
                    .map(|arg| suspending_call_count(arg, suspending))
                    .sum::<usize>()
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter()
            .map(|item| suspending_call_count(item, suspending))
            .sum(),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => suspending_call_count(head, suspending) + suspending_call_count(tail, suspending),
        CoreExpr::Map(fields) => fields
            .iter()
            .map(|field| suspending_call_count(&field.value, suspending))
            .sum(),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .map(|field| suspending_call_count(&field.value, suspending))
                .sum()
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            suspending_call_count(base, suspending)
                + fields
                    .iter()
                    .map(|field| suspending_call_count(&field.value, suspending))
                    .sum::<usize>()
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            suspending_call_count(base, suspending)
        }
        CoreExpr::RemoteCall { args, .. } => args
            .iter()
            .map(|arg| suspending_call_count(arg, suspending))
            .sum(),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            suspending_call_count(receiver, suspending)
                + args
                    .iter()
                    .map(|arg| suspending_call_count(arg, suspending))
                    .sum::<usize>()
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .map(|arg| suspending_call_count(arg, suspending))
                .sum::<usize>()
                + suspending_call_count(record, suspending)
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            suspending_call_count(expr, suspending)
                + generators
                    .iter()
                    .map(|generator| suspending_call_count(&generator.source, suspending))
                    .sum::<usize>()
                + guards
                    .iter()
                    .map(|guard| suspending_call_count(guard, suspending))
                    .sum::<usize>()
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            suspending_call_count(operand, suspending)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            suspending_call_count(left, suspending) + suspending_call_count(right, suspending)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| suspending_call_count(&binding.value, suspending))
                .sum::<usize>()
                + suspending_call_count(body, suspending)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .map(|clause| {
                suspending_call_count(&clause.condition, suspending)
                    + suspending_call_count(&clause.body, suspending)
            })
            .sum(),
        CoreExpr::Case { scrutinee, clauses } => {
            suspending_call_count(scrutinee, suspending)
                + clauses
                    .iter()
                    .map(|clause| {
                        clause
                            .guard
                            .as_ref()
                            .map_or(0, |guard| suspending_call_count(guard, suspending))
                            + suspending_call_count(&clause.body, suspending)
                    })
                    .sum::<usize>()
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            suspending_call_count(body, suspending)
                + of_clauses
                    .iter()
                    .chain(catch_clauses)
                    .map(|clause| {
                        clause
                            .guard
                            .as_ref()
                            .map_or(0, |guard| suspending_call_count(guard, suspending))
                            + suspending_call_count(&clause.body, suspending)
                    })
                    .sum::<usize>()
                + after_clause.as_ref().map_or(0, |after| {
                    suspending_call_count(&after.trigger, suspending)
                        + suspending_call_count(&after.body, suspending)
                })
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .map(|parameter| suspending_call_count(parameter, suspending))
            .sum(),
        _ => 0,
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct ComposedCallProfile {
    pub(super) continuations: Vec<ComposedContinuationProfile>,
    pub(super) entries: Vec<u64>,
    pub(super) tail_entries: HashMap<usize, Vec<(u64, usize)>>,
}

/// The bounded ABI metadata needed to call a shared continuation body.
#[derive(Clone, Eq, PartialEq)]
pub(super) struct ComposedContinuationProfile {
    pub(super) id: u64,
    pub(super) source_span: Option<Span>,
    pub(super) params: Vec<NativeType>,
    pub(super) body: NativeExpr,
    pub(super) completion_result: bool,
}

pub(super) struct RecursiveReductionMember {
    pub(super) module: String,
    pub(super) function_name: String,
    pub(super) arity: usize,
    pub(super) function: usize,
    pub(super) params: Vec<NativeType>,
}

#[derive(Clone, Debug)]
pub(super) struct CallRegion {
    pub(super) prefix: Vec<CoreLetBinding>,
    pub(super) target: CallTarget,
    pub(super) args: Vec<CoreExpr>,
    pub(super) resume: CoreExpr,
    pub(super) result_name: String,
    pub(super) gates: Vec<CallGate>,
    pub(super) join: Option<CallJoin>,
}

#[derive(Clone, Debug)]
pub(super) enum CallTarget {
    Direct(String),
    Dynamic(Box<CoreExpr>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct DynamicCallSignature {
    pub(super) parameters: Vec<NativeType>,
    pub(super) result: NativeType,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct DynamicTargetProfile {
    pub(super) export_id: u64,
    pub(super) source: String,
    pub(super) profile: ComposedCallProfile,
}

pub(super) type DynamicCallProfiles = HashMap<DynamicCallSignature, Vec<DynamicTargetProfile>>;

#[derive(Clone, Debug)]
pub(super) struct CallJoin {
    pub(super) result_name: String,
    pub(super) resume: CoreExpr,
}

/// One short-circuit decision that must be evaluated before entering a call.
#[derive(Clone, Debug)]
pub(super) struct CallGate {
    pub(super) condition: CoreExpr,
    pub(super) call_when_true: bool,
    pub(super) prefix: Vec<CoreLetBinding>,
    pub(super) bypass_resume: CoreExpr,
}

fn unique_prefix_name(
    base: &str,
    region: &CallRegion,
    extra: &[CoreLetBinding],
    reserved: &HashSet<String>,
) -> String {
    let mut used = free_variables(&region.resume);
    for gate in &region.gates {
        used.extend(free_variables(&gate.condition));
        used.extend(free_variables(&gate.bypass_resume));
    }
    used.extend(reserved.iter().cloned());
    used.extend(
        region
            .prefix
            .iter()
            .chain(extra)
            .filter_map(|binding| match &binding.pattern {
                CorePattern::Var(name) => Some(name.clone()),
                _ => None,
            }),
    );
    (0usize..)
        .map(|index| format!("{base}_{index}"))
        .find(|name| !used.contains(name))
        .expect("generated prefix name space is unbounded")
}

fn unconditional_true_prefix(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
) -> Option<Vec<CoreLetBinding>> {
    match expr {
        CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "true" => Some(Vec::new()),
        CoreExpr::Let { bindings, body }
            if bindings.iter().all(|binding| {
                matches!(binding.pattern, CorePattern::Var(_))
                    && !expr_calls_suspending(&binding.value, suspending)
                    && !contains_process_yield(&binding.value)
            }) =>
        {
            let mut prefix = bindings.clone();
            prefix.extend(unconditional_true_prefix(body, suspending)?);
            Some(prefix)
        }
        _ => None,
    }
}

pub(super) fn is_composable_suspending_body(
    body: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
) -> bool {
    let continuation_count =
        process_yield_count(body).saturating_add(suspending_call_count(body, suspending));
    (1..=MAX_COMPOSED_CALL_CONTINUATIONS).contains(&continuation_count)
        && suspending_calls_are_composable(body, suspending, composable)
}

pub(super) fn composable_suspension_gap_reason(
    body: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
) -> String {
    let continuation_count =
        process_yield_count(body).saturating_add(suspending_call_count(body, suspending));
    if continuation_count == 0 {
        return "suspension reachability and continuation counting disagree: no continuation-producing expression was counted".to_string();
    }
    if continuation_count > MAX_COMPOSED_CALL_CONTINUATIONS {
        return format!(
            "candidate requires {continuation_count} continuation records; maximum is {MAX_COMPOSED_CALL_CONTINUATIONS}"
        );
    }
    if !suspending_calls_are_composable(body, suspending, composable) {
        return "candidate reaches a suspension target outside the closed composable set"
            .to_string();
    }
    "candidate failed composable suspension admission without a reported invariant violation"
        .to_string()
}

fn suspending_calls_are_composable(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
) -> bool {
    if let CoreExpr::Call { function, args } = expr {
        let identity = (function.clone(), args.len());
        if suspending.contains(&identity) && !composable.contains(&identity) {
            return false;
        }
    }
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .all(|arg| suspending_calls_are_composable(arg, suspending, composable)),
        CoreExpr::FunctionCall { callee, args } => {
            suspending_calls_are_composable(callee, suspending, composable)
                && args
                    .iter()
                    .all(|arg| suspending_calls_are_composable(arg, suspending, composable))
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter()
            .all(|item| suspending_calls_are_composable(item, suspending, composable)),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            suspending_calls_are_composable(head, suspending, composable)
                && suspending_calls_are_composable(tail, suspending, composable)
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .all(|field| suspending_calls_are_composable(&field.value, suspending, composable)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .all(|field| suspending_calls_are_composable(&field.value, suspending, composable))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            suspending_calls_are_composable(base, suspending, composable)
                && fields.iter().all(|field| {
                    suspending_calls_are_composable(&field.value, suspending, composable)
                })
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            suspending_calls_are_composable(receiver, suspending, composable)
                && args
                    .iter()
                    .all(|arg| suspending_calls_are_composable(arg, suspending, composable))
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .all(|arg| suspending_calls_are_composable(arg, suspending, composable))
                && suspending_calls_are_composable(record, suspending, composable)
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            suspending_calls_are_composable(expr, suspending, composable)
                && generators.iter().all(|generator| {
                    suspending_calls_are_composable(&generator.source, suspending, composable)
                })
                && guards
                    .iter()
                    .all(|guard| suspending_calls_are_composable(guard, suspending, composable))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            suspending_calls_are_composable(base, suspending, composable)
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            suspending_calls_are_composable(operand, suspending, composable)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            suspending_calls_are_composable(left, suspending, composable)
                && suspending_calls_are_composable(right, suspending, composable)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                suspending_calls_are_composable(&binding.value, suspending, composable)
            }) && suspending_calls_are_composable(body, suspending, composable)
        }
        CoreExpr::If { clauses } => clauses.iter().all(|clause| {
            suspending_calls_are_composable(&clause.condition, suspending, composable)
                && suspending_calls_are_composable(&clause.body, suspending, composable)
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            suspending_calls_are_composable(scrutinee, suspending, composable)
                && clauses.iter().all(|clause| {
                    clause.guard.as_ref().is_none_or(|guard| {
                        suspending_calls_are_composable(guard, suspending, composable)
                    }) && suspending_calls_are_composable(&clause.body, suspending, composable)
                })
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            suspending_calls_are_composable(body, suspending, composable)
                && of_clauses.iter().chain(catch_clauses).all(|clause| {
                    clause.guard.as_ref().is_none_or(|guard| {
                        suspending_calls_are_composable(guard, suspending, composable)
                    }) && suspending_calls_are_composable(&clause.body, suspending, composable)
                })
                && after_clause.as_ref().is_none_or(|after| {
                    suspending_calls_are_composable(&after.trigger, suspending, composable)
                        && suspending_calls_are_composable(&after.body, suspending, composable)
                })
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .all(|parameter| suspending_calls_are_composable(parameter, suspending, composable)),
        _ => true,
    }
}

impl ComposedCallProfile {
    pub(super) fn pure() -> Self {
        Self {
            continuations: Vec::new(),
            entries: Vec::new(),
            tail_entries: HashMap::new(),
        }
    }

    /// Describes the cyclic resume graph of one direct self-recursive
    /// reduction loop.
    ///
    /// The VM may enter the reduction continuation repeatedly before the
    /// function completes. `CallThen` keeps the outer caller's captures on
    /// every parked edge and routes synchronous completion through a distinct
    /// result node.
    pub(super) fn recursive_component(
        entry_module: &str,
        entry_function: &str,
        entry_arity: usize,
        result: NativeType,
        members: Vec<RecursiveReductionMember>,
    ) -> Self {
        let completion_id = super::identity::stable_reduction_completion_id(
            entry_module,
            entry_function,
            entry_arity,
        );
        let reductions = members
            .iter()
            .map(|member| {
                (
                    super::identity::stable_reduction_continuation_id(
                        &member.module,
                        &member.function_name,
                        member.arity,
                    ),
                    member,
                )
            })
            .collect::<Vec<_>>();
        let resumes = reductions
            .iter()
            .map(|(id, member)| super::NativeCallResume {
                callee_continuation_id: *id,
                callee_capture_count: member.params.len(),
                continuation_id: *id,
                caller_value_start: 0,
            })
            .collect::<Vec<_>>();
        let mut continuations = reductions
            .iter()
            .map(|(id, member)| ComposedContinuationProfile {
                id: *id,
                source_span: None,
                params: member.params.clone(),
                body: NativeExpr::CallThen {
                    function: member.function,
                    args: (0..member.params.len()).map(NativeExpr::Param).collect(),
                    resumes: resumes.clone(),
                    completion_continuation_id: completion_id,
                    completion_function: None,
                    values: Vec::new(),
                },
                completion_result: false,
            })
            .collect::<Vec<_>>();
        let completion = ComposedContinuationProfile {
            id: completion_id,
            source_span: None,
            params: vec![result],
            body: NativeExpr::Param(0),
            completion_result: true,
        };
        continuations.push(completion);
        Self {
            entries: reductions.iter().map(|(id, _)| *id).collect(),
            continuations,
            tail_entries: HashMap::new(),
        }
    }

    pub(super) fn new(
        function_body: &NativeExpr,
        continuations: &[NativeContinuation],
        terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    ) -> Option<Self> {
        Self::build(function_body, continuations, terminal_profiles).ok()
    }

    /// Builds one profile while retaining the exact failed graph invariant for
    /// fixed-point diagnostics.
    fn build(
        function_body: &NativeExpr,
        continuations: &[NativeContinuation],
        terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    ) -> super::NativeIrResult<Self> {
        let mut continuation_pool = continuations.to_vec();
        let mut tail_entries = HashMap::new();
        let mut pending_targets = direct_tail_targets(function_body);
        pending_targets.extend(
            continuation_pool
                .iter()
                .flat_map(|continuation| direct_tail_targets(&continuation.body)),
        );
        let mut visited_targets = HashSet::new();
        while let Some(target) = pending_targets.pop() {
            if !visited_targets.insert(target) {
                continue;
            }
            let profile = terminal_profiles.get(&target).ok_or_else(|| {
                format!("tail target {target} has no available suspension profile")
            })?;
            let by_id = profile
                .continuations
                .iter()
                .map(|continuation| (continuation.id, continuation))
                .collect::<HashMap<_, _>>();
            let entries = profile
                .entries
                .iter()
                .map(|entry| {
                    by_id
                        .get(entry)
                        .map(|continuation| (*entry, continuation.params.len()))
                        .ok_or_else(|| {
                            format!("tail target {target} advertises absent continuation {entry}")
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            tail_entries.insert(target, entries);
            for (nested_target, entries) in &profile.tail_entries {
                tail_entries
                    .entry(*nested_target)
                    .or_insert_with(|| entries.clone());
                pending_targets.push(*nested_target);
            }
            // Tail callees retain ownership of their continuation bodies.  A
            // caller profile needs only the outward entry identities and
            // capture shapes; copying the complete graph here makes profile
            // width grow transitively with every call edge.
            for entry in &profile.entries {
                let continuation = by_id.get(entry).copied().ok_or_else(|| {
                    format!("tail target {target} advertises absent continuation {entry}")
                })?;
                if continuation_pool
                    .iter()
                    .any(|existing| existing.id == *entry)
                {
                    continue;
                }
                continuation_pool.push(opaque_profile_entry(continuation));
            }
        }
        // `CallThen` also exposes callee-owned continuation identities.  Add
        // shape-only records so fixed-point admission can validate those
        // edges without materializing another copy of their bodies.
        let mut outward_ids = direct_suspend_ids(function_body);
        outward_ids.extend(
            continuation_pool
                .iter()
                .flat_map(|continuation| direct_suspend_ids(&continuation.body)),
        );
        outward_ids.sort_unstable();
        outward_ids.dedup();
        for id in outward_ids {
            if continuation_pool
                .iter()
                .any(|continuation| continuation.id == id)
            {
                continue;
            }
            let source = terminal_profiles
                .values()
                .flat_map(|profile| profile.continuations.iter())
                .find(|continuation| continuation.id == id)
                .ok_or_else(|| {
                    format!("outward continuation {id} has no available suspension profile")
                })?;
            continuation_pool.push(NativeContinuation {
                id,
                source_module: String::new(),
                source_function: String::new(),
                source_arity: 0,
                source_span: source.source_span,
                capture_names: Vec::new(),
                params: source.params.clone(),
                return_type: source.params.last().copied().unwrap_or(NativeType::Unit),
                body: NativeExpr::Unit,
            });
        }
        if continuation_pool.is_empty() || continuation_pool.len() > MAX_COMPOSED_CALL_CONTINUATIONS
        {
            return Err(format!(
                "profile continuation width {} is outside 1..={MAX_COMPOSED_CALL_CONTINUATIONS}",
                continuation_pool.len()
            )
            .into());
        }

        let by_id = continuation_pool
            .iter()
            .map(|continuation| (continuation.id, continuation))
            .collect::<std::collections::HashMap<_, _>>();
        if by_id.len() != continuation_pool.len() {
            return Err("profile contains duplicate continuation identities".into());
        }
        let completion_ids = continuation_pool
            .iter()
            .flat_map(|continuation| direct_completion_ids(&continuation.body))
            .chain(direct_completion_ids(function_body))
            .collect::<HashSet<_>>();
        let yield_ids = continuation_pool
            .iter()
            .flat_map(|continuation| profile_entry_ids(&continuation.body, &tail_entries))
            .chain(profile_entry_ids(function_body, &tail_entries))
            .collect::<HashSet<_>>();
        if completion_ids.iter().any(|id| yield_ids.contains(id)) {
            let mut overlap = completion_ids
                .intersection(&yield_ids)
                .copied()
                .collect::<Vec<_>>();
            overlap.sort_unstable();
            let sites = overlap
                .iter()
                .map(|id| {
                    let mut completion_sites = Vec::new();
                    let mut yield_sites = Vec::new();
                    if direct_completion_ids(function_body).contains(id) {
                        completion_sites.push("function".to_string());
                    }
                    if profile_entry_ids(function_body, &tail_entries).contains(id) {
                        yield_sites.push("function".to_string());
                    }
                    for continuation in &continuation_pool {
                        if direct_completion_ids(&continuation.body).contains(id) {
                            completion_sites.push(format!("continuation {}", continuation.id));
                        }
                        if profile_entry_ids(&continuation.body, &tail_entries).contains(id) {
                            yield_sites.push(format!("continuation {}", continuation.id));
                        }
                    }
                    format!("{id}: completion={completion_sites:?}, outward={yield_sites:?}")
                })
                .collect::<Vec<_>>();
            return Err(format!(
                "continuations {overlap:?} are both completion and outward-yield entries ({})",
                sites.join("; ")
            )
            .into());
        }
        let initial_entries = profile_entry_ids(function_body, &tail_entries);
        if initial_entries.is_empty() {
            return Err("function body exposes no initial suspension entry".into());
        }
        let mut next_ids = initial_entries;

        let mut ordered = Vec::with_capacity(continuation_pool.len());
        let mut visited = HashSet::with_capacity(continuation_pool.len());
        while let Some(id) = next_ids.pop() {
            if !visited.insert(id) {
                continue;
            }
            let continuation = by_id.get(&id).copied().ok_or_else(|| {
                format!("reachable suspension edge references absent continuation {id}")
            })?;
            ordered.push(ComposedContinuationProfile {
                id,
                source_span: continuation.source_span,
                params: continuation.params.clone(),
                body: continuation.body.clone(),
                completion_result: completion_ids.contains(&id),
            });
            next_ids.extend(unique_profile_edges(&continuation.body, &tail_entries));
        }
        // Structured control lowering can retain branch-local continuation
        // records after a condition was folded or a shared entry was
        // interned. They are not reachable from this function's primary
        // entry, but may still be referenced by the lowered module until
        // application-wide continuation materialization. Preserve those
        // records in the profile without advertising them as call entries.
        for continuation in &continuation_pool {
            if visited.insert(continuation.id) {
                ordered.push(ComposedContinuationProfile {
                    id: continuation.id,
                    source_span: continuation.source_span,
                    params: continuation.params.clone(),
                    body: continuation.body.clone(),
                    completion_result: completion_ids.contains(&continuation.id),
                });
            }
        }
        // A composed call can complete several inner calls synchronously
        // before its first externally visible yield. After the caller resumes
        // that yield, later stages can likewise expose any other reachable
        // yield in the continuation graph. Advertise the complete reachable
        // outward-yield set so every generated call contract accepts every
        // continuation identity that the callee can legitimately return.
        let mut entries = ordered
            .iter()
            .filter(|continuation| yield_ids.contains(&continuation.id))
            .map(|continuation| continuation.id)
            .collect::<Vec<_>>();
        entries.sort_unstable();
        entries.dedup();
        Ok(Self {
            continuations: ordered,
            entries,
            tail_entries,
        })
    }
}

fn opaque_profile_entry(continuation: &ComposedContinuationProfile) -> NativeContinuation {
    NativeContinuation {
        id: continuation.id,
        source_module: String::new(),
        source_function: String::new(),
        source_arity: 0,
        source_span: continuation.source_span,
        capture_names: Vec::new(),
        params: continuation.params.clone(),
        return_type: continuation
            .params
            .last()
            .copied()
            .unwrap_or(NativeType::Unit),
        body: NativeExpr::Unit,
    }
}

pub(super) fn profile_gap_reason(
    function_body: &NativeExpr,
    continuations: &[NativeContinuation],
    terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    function_labels: &HashMap<usize, String>,
    unavailable_profiles: &HashMap<usize, String>,
) -> String {
    let mut tails = direct_tail_targets(function_body);
    tails.extend(
        continuations
            .iter()
            .flat_map(|continuation| direct_tail_targets(&continuation.body)),
    );
    tails.sort_unstable();
    tails.dedup();
    if let Some(target) = tails
        .into_iter()
        .find(|target| !terminal_profiles.contains_key(target))
    {
        let label = function_labels
            .get(&target)
            .map_or_else(|| target.to_string(), |label| format!("{target} ({label})"));
        let cause = unavailable_profiles
            .get(&target)
            .map_or(String::new(), |reason| format!(": {reason}"));
        return format!("tail target {label} has no converged suspension profile{cause}");
    }
    if continuations.is_empty() {
        return "lowered body has no continuation records".to_string();
    }
    if continuations.len() > MAX_COMPOSED_CALL_CONTINUATIONS {
        return format!(
            "lowered body has {} continuation records; maximum is {MAX_COMPOSED_CALL_CONTINUATIONS}",
            continuations.len()
        );
    }
    let known = continuations
        .iter()
        .map(|continuation| continuation.id)
        .collect::<HashSet<_>>();
    let mut entries = direct_suspend_ids(function_body);
    entries.sort_unstable();
    entries.dedup();
    if entries.is_empty() {
        return "lowered body has no outward suspension entry".to_string();
    }
    if let Some(entry) = entries.into_iter().find(|entry| !known.contains(entry)) {
        return format!("suspension entry continuation {entry} is absent from the lowered pool");
    }
    ComposedCallProfile::build(function_body, continuations, terminal_profiles)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| {
            "continuation graph is not closed under reachable suspension edges".to_string()
        })
}

fn unique_profile_edges(
    body: &NativeExpr,
    tail_entries: &HashMap<usize, Vec<(u64, usize)>>,
) -> Vec<u64> {
    let mut ids = profile_entry_ids(body, tail_entries);
    ids.extend(direct_completion_ids(body));
    ids.extend(direct_tail_continuation_ids(body));
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn profile_entry_ids(
    body: &NativeExpr,
    tail_entries: &HashMap<usize, Vec<(u64, usize)>>,
) -> Vec<u64> {
    let mut ids = direct_suspend_ids(body);
    for target in direct_tail_targets(body) {
        ids.extend(
            tail_entries
                .get(&target)
                .into_iter()
                .flatten()
                .map(|(id, _)| *id),
        );
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn direct_tail_targets(body: &NativeExpr) -> Vec<usize> {
    let mut targets = Vec::new();
    walk_native_expr(body, &mut |expr| {
        if let NativeExpr::TailCall { function, .. } = expr {
            targets.push(*function);
        }
    });
    targets
}

/// Returns whether an expression still contains an ordinary call to a
/// suspension-capable function.
///
/// Call profiles are built as a dependency fixed point. A caller can be
/// lowered before one of its callee profiles exists, in which case lowering
/// temporarily leaves an ordinary `Call` in a generated continuation. Such a
/// profile must not be cached: the suspension ABI adds transition arguments
/// that an ordinary call neither supplies nor knows how to resume. Deferring
/// admission lets the next fixed-point pass compose the call after its callee
/// profile becomes available.
mod yield_analysis;

use yield_analysis::process_yield_count;
