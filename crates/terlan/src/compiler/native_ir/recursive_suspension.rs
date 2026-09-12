//! Bounded native transitions for non-tail recursive suspension.
//!
//! The VM already owns a growable, precisely rooted completion stack. A native
//! recursive call must yield before entering the next invocation: otherwise
//! its eventual suspension would flatten an unbounded number of caller frames
//! into one fixed native transition buffer. Shared entry thunks transfer these
//! backedges to the scheduler without cloning continuation bodies or changing
//! the runtime ABI. Ordinary calls and optimized tail loops remain unchanged.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::{
    call_composition::walk_native_expr, identity::stable_export_id,
    tail_position::strongly_connected_components, NativeCallResume, NativeContinuation,
    NativeDynamicCallResume, NativeExpr, NativeFunction, NativeModule, NativeTransitionOperation,
};

const MODULE: &str = "$terlan.recursive_suspension";

#[derive(Clone, Copy)]
struct DeferredEntry {
    function: usize,
    continuation: u64,
    captures: usize,
}

pub(super) fn defer_recursive_calls(modules: &mut Vec<NativeModule>) -> Result<(), String> {
    let functions = modules
        .iter()
        .flat_map(|module| module.functions.iter())
        .collect::<Vec<_>>();
    let graph = call_graph(&functions)?;
    let components = strongly_connected_components(&graph);
    let mut targets = BTreeMap::new();
    let mut expected = 0;
    for (caller, function) in functions.iter().enumerate() {
        walk_native_expr(&function.body, &mut |expr| {
            if let NativeExpr::CallThen { function, .. } = expr {
                if components[caller] == components[*function] {
                    targets.entry(*function).or_insert(None);
                    expected += 1;
                }
            }
        });
    }
    if targets.is_empty() {
        return Ok(());
    }
    let mut identities = functions
        .iter()
        .map(|function| function.export_id)
        .chain(modules.iter().flat_map(|module| {
            module
                .continuations
                .iter()
                .map(|continuation| continuation.id)
        }))
        .collect::<HashSet<_>>();
    let mut deferred = NativeModule {
        name: MODULE.to_string(),
        functions: Vec::new(),
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    };
    for (target, entry) in &mut targets {
        let original = functions[*target];
        let name = format!("$defer_{}", original.export_id);
        let continuation = stable_export_id(
            MODULE,
            &format!("$resume_{}", original.export_id),
            original.arity,
        );
        let export = stable_export_id(MODULE, &name, original.arity);
        if !identities.insert(continuation) || !identities.insert(export) {
            return Err(
                "error[native_ir.recursive_suspend_identity]: generated identity collision"
                    .to_string(),
            );
        }
        *entry = Some(DeferredEntry {
            function: functions.len() + deferred.functions.len(),
            continuation,
            captures: original.params.len(),
        });
        let arguments = (0..original.params.len())
            .map(NativeExpr::Param)
            .collect::<Vec<_>>();
        deferred.continuations.push(NativeContinuation {
            id: continuation,
            source_module: original.source_module.clone(),
            source_function: original.source_function.clone(),
            source_arity: original.source_arity,
            source_span: None,
            capture_names: Vec::new(),
            params: original.params.clone(),
            return_type: original.return_type,
            body: NativeExpr::TailCall {
                function: *target,
                args: arguments.clone(),
                yield_continuation_id: None,
            },
        });
        deferred.functions.push(NativeFunction {
            export_id: export,
            name,
            public: false,
            arity: original.arity,
            source_module: original.source_module.clone(),
            source_function: original.source_function.clone(),
            source_arity: original.source_arity,
            callable_captures: Vec::new(),
            params: original.params.clone(),
            return_type: original.return_type,
            body: NativeExpr::Suspend {
                operation: NativeTransitionOperation::Yield,
                arguments: Vec::new(),
                continuation_id: continuation,
                values: arguments,
            },
        });
    }
    let mut rewritten = 0;
    for (caller, function) in modules
        .iter_mut()
        .flat_map(|module| &mut module.functions)
        .enumerate()
    {
        rewritten += rewrite_returning_calls(&mut function.body, caller, &components, &targets);
    }
    if rewritten != expected {
        return Err("error[native_ir.recursive_suspend_shape]: recursive suspension is not in a supported return context".to_string());
    }
    modules.push(deferred);
    refresh_entry_contracts(modules)?;
    Ok(())
}

/// A deferred entry is outwardly visible through every caller, including a
/// nonrecursive caller outside the transformed component. Propagate identities
/// with a worklist rather than repeatedly cloning the whole function catalog.
fn refresh_entry_contracts(modules: &mut [NativeModule]) -> Result<(), String> {
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let graph = call_graph(&functions)?;
    let mut parents = vec![Vec::new(); functions.len()];
    let exports = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.export_id, index))
        .collect::<HashMap<_, _>>();
    for (caller, callees) in graph.iter().enumerate() {
        for callee in callees {
            parents[*callee].push(caller);
        }
    }
    let mut entries = functions
        .iter()
        .map(|function| {
            let mut ids = HashSet::new();
            walk_native_expr(&function.body, &mut |expr| match expr {
                NativeExpr::Suspend {
                    continuation_id, ..
                } => {
                    ids.insert(*continuation_id);
                }
                NativeExpr::TailCall {
                    yield_continuation_id: Some(id),
                    ..
                } => {
                    ids.insert(*id);
                }
                _ => {}
            });
            ids
        })
        .collect::<Vec<_>>();
    let mut queued = entries
        .iter()
        .map(|entries| !entries.is_empty())
        .collect::<Vec<_>>();
    let mut pending = queued
        .iter()
        .enumerate()
        .filter_map(|(index, queued)| queued.then_some(index))
        .collect::<VecDeque<_>>();
    while let Some(callee) = pending.pop_front() {
        queued[callee] = false;
        let found = entries[callee].clone();
        for caller in &parents[callee] {
            let before = entries[*caller].len();
            entries[*caller].extend(found.iter().copied());
            if entries[*caller].len() != before && !queued[*caller] {
                queued[*caller] = true;
                pending.push_back(*caller);
            }
        }
    }
    let captures = modules
        .iter()
        .flat_map(|module| &module.continuations)
        .map(|entry| (entry.id, entry.params.len()))
        .collect::<HashMap<_, _>>();
    for function in modules.iter_mut().flat_map(|module| &mut module.functions) {
        refresh_returning_calls(&mut function.body, &entries, &captures, &exports)?;
    }
    Ok(())
}

fn refresh_returning_calls(
    expr: &mut NativeExpr,
    entries: &[HashSet<u64>],
    captures: &HashMap<u64, usize>,
    exports: &HashMap<u64, usize>,
) -> Result<(), String> {
    match expr {
        NativeExpr::CallThen {
            function,
            resumes,
            completion_continuation_id,
            values,
            ..
        } => {
            let forwards = values.is_empty()
                && !resumes.is_empty()
                && resumes
                    .iter()
                    .all(|resume| resume.continuation_id == resume.callee_continuation_id);
            for id in &entries[*function] {
                if resumes
                    .iter()
                    .any(|resume| resume.callee_continuation_id == *id)
                {
                    continue;
                }
                let count = captures.get(id).copied().ok_or_else(|| format!("error[native_ir.recursive_suspend_entry]: continuation {id} has no capture layout"))?;
                resumes.push(NativeCallResume {
                    callee_continuation_id: *id,
                    callee_capture_count: count,
                    continuation_id: if forwards {
                        *id
                    } else {
                        *completion_continuation_id
                    },
                    caller_value_start: 0,
                });
            }
            resumes.sort_by_key(|resume| resume.callee_continuation_id);
        }
        NativeExpr::InvokeClosureThen {
            resumes,
            completion_continuation_id,
            ..
        } => {
            let targets = resumes
                .iter()
                .map(|resume| resume.callee_export_id)
                .collect::<HashSet<_>>();
            for export in targets {
                let function = exports.get(&export).ok_or_else(|| {
                    format!(
                        "error[native_ir.recursive_suspend_target]: unavailable callback {export}"
                    )
                })?;
                for id in &entries[*function] {
                    if resumes.iter().any(|resume| {
                        resume.callee_export_id == export && resume.callee_continuation_id == *id
                    }) {
                        continue;
                    }
                    let count = captures.get(id).copied().ok_or_else(|| format!("error[native_ir.recursive_suspend_entry]: continuation {id} has no capture layout"))?;
                    resumes.push(NativeDynamicCallResume {
                        callee_export_id: export,
                        callee_continuation_id: *id,
                        callee_capture_count: count,
                        continuation_id: *completion_continuation_id,
                    });
                }
            }
            resumes.sort_by_key(|resume| (resume.callee_export_id, resume.callee_continuation_id));
        }
        NativeExpr::Let { body, .. } => refresh_returning_calls(body, entries, captures, exports)?,
        NativeExpr::If { clauses } => {
            for (_, body) in clauses {
                refresh_returning_calls(body, entries, captures, exports)?;
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } if cleanup.is_empty() => {
            for body in [protected, success, failure] {
                refresh_returning_calls(body, entries, captures, exports)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn call_graph(functions: &[&NativeFunction]) -> Result<Vec<Vec<usize>>, String> {
    let exports = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.export_id, index))
        .collect::<HashMap<_, _>>();
    functions
        .iter()
        .map(|function| {
            let mut calls = Vec::new();
            walk_native_expr(&function.body, &mut |expr| match expr {
                NativeExpr::Call { function, .. } | NativeExpr::TailCall { function, .. } => {
                    calls.push(*function)
                }
                NativeExpr::CallThen {
                    function,
                    completion_function,
                    ..
                } => {
                    calls.push(*function);
                    calls.extend(completion_function);
                }
                NativeExpr::InvokeClosureThen {
                    completion_function,
                    resumes,
                    ..
                } => {
                    calls.extend(completion_function);
                    calls.extend(resumes.iter().map(|resume| {
                        exports
                            .get(&resume.callee_export_id)
                            .copied()
                            .unwrap_or(functions.len())
                    }));
                }
                _ => {}
            });
            calls.sort_unstable();
            calls.dedup();
            if calls.iter().any(|target| *target >= functions.len()) {
                return Err(
                    "error[native_ir.recursive_suspend_target]: unavailable call target"
                        .to_string(),
                );
            }
            Ok(calls)
        })
        .collect()
}

/// CallThen is only legal as a terminal entry expression. Count every rewrite
/// against the full expression inventory so a new unsupported nesting shape
/// cannot silently retain an unbounded recursive transition.
fn rewrite_returning_calls(
    expr: &mut NativeExpr,
    caller: usize,
    components: &[usize],
    targets: &BTreeMap<usize, Option<DeferredEntry>>,
) -> usize {
    match expr {
        NativeExpr::CallThen {
            function,
            resumes,
            completion_continuation_id,
            ..
        } if components[caller] == components[*function] => {
            let Some(Some(entry)) = targets.get(function) else {
                return 0;
            };
            *function = entry.function;
            *resumes = vec![NativeCallResume {
                callee_continuation_id: entry.continuation,
                callee_capture_count: entry.captures,
                continuation_id: *completion_continuation_id,
                caller_value_start: 0,
            }];
            1
        }
        NativeExpr::Let { body, .. } => rewrite_returning_calls(body, caller, components, targets),
        NativeExpr::If { clauses } => clauses
            .iter_mut()
            .map(|(_, body)| rewrite_returning_calls(body, caller, components, targets))
            .sum(),
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } if cleanup.is_empty() => [protected, success, failure]
            .into_iter()
            .map(|body| rewrite_returning_calls(body, caller, components, targets))
            .sum(),
        _ => 0,
    }
}
