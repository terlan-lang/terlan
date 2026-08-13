//! Compiler-owned NativeIR tail-position classification.

use std::collections::HashSet;

use super::{NativeContinuation, NativeExpr, NativeModule, NativeTransitionOperation};

/// Installs one stable resume entry for every recursive application function.
///
/// The resume entry accepts the next call's already-evaluated arguments and
/// terminally re-enters its target after a VM-owned reduction yield.
pub(super) fn install_reduction_continuations(modules: &mut [NativeModule]) -> Result<(), String> {
    let graph = application_call_graph(modules, false);
    let components = strongly_connected_components(&graph);
    let component_sizes = component_sizes(&components);
    let mut required = HashSet::new();
    for module in modules.iter() {
        for function in &module.functions {
            collect_resume_ids(&function.body, &mut required);
        }
        for continuation in &module.continuations {
            collect_resume_ids(&continuation.body, &mut required);
        }
    }
    let mut existing = modules
        .iter()
        .flat_map(|module| module.continuations.iter().map(|item| item.id))
        .collect::<HashSet<_>>();
    let functions = modules
        .iter()
        .enumerate()
        .flat_map(|(module, native)| {
            native
                .functions
                .iter()
                .enumerate()
                .map(move |(local, function)| (module, local, function.clone()))
        })
        .collect::<Vec<_>>();
    for (index, (module_index, _, function)) in functions.into_iter().enumerate() {
        let component = components[index];
        let recursive =
            component_sizes[component] > 1 || graph[index].binary_search(&index).is_ok();
        let id = super::identity::stable_reduction_continuation_id(
            &modules[module_index].name,
            &function.name,
            function.arity,
        );
        if !recursive && !required.contains(&id) {
            continue;
        }
        if !existing.insert(id) {
            return Err(format!(
                "error[native_ir.reduction_continuation_identity]: duplicate reduction continuation {id}"
            ));
        }
        modules[module_index]
            .continuations
            .push(NativeContinuation {
                id,
                source_module: function.source_module,
                source_function: function.source_function,
                source_arity: function.source_arity,
                source_span: None,
                capture_names: Vec::new(),
                params: function.params.clone(),
                return_type: function.return_type,
                body: NativeExpr::TailCall {
                    function: index,
                    args: (0..function.arity).map(NativeExpr::Param).collect(),
                    yield_continuation_id: None,
                },
            });
    }
    Ok(())
}

/// Attaches installed reduction identities to terminal calls that live in
/// generated continuation bodies and therefore are outside the source
/// function-only SCC graph.
pub(super) fn attach_installed_reduction_yields(modules: &mut [NativeModule]) {
    let functions = modules
        .iter()
        .flat_map(|module| {
            module.functions.iter().map(move |function| {
                super::identity::stable_reduction_continuation_id(
                    &module.name,
                    &function.name,
                    function.arity,
                )
            })
        })
        .collect::<Vec<_>>();
    let installed = modules
        .iter()
        .flat_map(|module| {
            module
                .continuations
                .iter()
                .map(|item| item.id)
                .chain(module.functions.iter().map(|item| item.export_id))
        })
        .collect::<HashSet<_>>();
    for module in modules {
        for continuation in &mut module.continuations {
            if !installed.contains(&continuation.id) {
                attach_reduction_yields(&mut continuation.body, &functions, &installed);
            }
        }
    }
}

/// Rejects dynamic terminal edges inside a statically recursive component.
///
/// An owned closure call remains useful outside recursive SCCs, but its
/// runtime-selected target cannot participate in the compiler-owned bounded
/// dispatcher. Rejecting it here keeps the failure ahead of Cranelift and the
/// native linker instead of silently depending on the host stack.
pub(super) fn validate_recursive_tail_targets(modules: &[NativeModule]) -> Result<(), String> {
    // Synchronous completion edges are direct native calls. They belong to
    // the same constant-stack component as their source recursion; Cranelift
    // keeps precise roots by packing managed and scalar arguments into
    // separate dispatcher lanes.
    let control_graph = application_call_graph(modules, true);
    let control_components = strongly_connected_components(&control_graph);
    let control_component_sizes = component_sizes(&control_components);
    for (function_index, (module, function)) in modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .map(move |function| (module, function))
        })
        .enumerate()
    {
        let component = control_components[function_index];
        let recursive = control_component_sizes[component] > 1
            || control_graph[function_index]
                .binary_search(&function_index)
                .is_ok();
        if recursive && contains_dynamic_tail_target(&function.body) {
            return Err(format!(
                "error[native_ir.dynamic_recursive_tail]: `{}`.`{}`/{} has a terminal dynamically selected target that cannot satisfy the compiler-owned constant-stack contract",
                module.name, function.name, function.arity
            ));
        }
    }
    Ok(())
}

/// Rewrites recursive-component calls in result-forwarding positions to terminal calls.
///
/// Function identities in NativeIR are application-global indexes. The pass
/// therefore runs after continuation materialization has frozen the complete
/// application order. It deliberately does not rewrite calls nested in
/// argument evaluation, bindings, conditions, constructors, operators, or
/// cleanup expressions.
pub(super) fn lower_recursive_tail_calls(modules: &mut [NativeModule]) {
    let graph = application_call_graph(modules, true);
    let forwarding_completions = forwarding_completion_ids(modules);
    let components = strongly_connected_components(&graph);
    let component_sizes = components
        .iter()
        .copied()
        .max()
        .map_or(Vec::new(), |maximum| {
            let mut sizes = vec![0_usize; maximum.saturating_add(1)];
            for component in &components {
                sizes[*component] = sizes[*component].saturating_add(1);
            }
            sizes
        });
    let mut function_index = 0;
    let installed = modules
        .iter()
        .flat_map(|module| module.continuations.iter().map(|item| item.id))
        .collect::<HashSet<_>>();
    let yield_ids = modules
        .iter()
        .flat_map(|module| {
            module.functions.iter().map(|function| {
                let id = super::identity::stable_reduction_continuation_id(
                    &module.name,
                    &function.name,
                    function.arity,
                );
                installed.contains(&id).then_some(id)
            })
        })
        .collect::<Vec<_>>();
    for module in modules {
        for function in &mut module.functions {
            let component = components[function_index];
            let recursive = component_sizes[component] > 1
                || graph[function_index].binary_search(&function_index).is_ok();
            if recursive {
                lower_tail_position(
                    &mut function.body,
                    component,
                    &components,
                    &yield_ids,
                    &forwarding_completions,
                );
            }
            function_index = function_index.saturating_add(1);
        }
    }
}

/// Returns application-global mutually recursive tail-call components.
pub(super) fn mutual_tail_components(modules: &[NativeModule]) -> Vec<Vec<usize>> {
    let graph = application_call_graph(modules, true);
    let components = strongly_connected_components(&graph);
    let mut members = components
        .iter()
        .copied()
        .max()
        .map_or(Vec::new(), |maximum| {
            vec![Vec::<usize>::new(); maximum.saturating_add(1)]
        });
    for (function, component) in components.into_iter().enumerate() {
        members[component].push(function);
    }
    members.retain(|component| component.len() > 1);
    members.sort();
    members
}

fn lower_tail_position(
    expr: &mut NativeExpr,
    current_component: usize,
    components: &[usize],
    yield_ids: &[Option<u64>],
    forwarding_completions: &HashSet<u64>,
) {
    match expr {
        NativeExpr::Call { function, args }
            if components.get(*function).copied() == Some(current_component) =>
        {
            let args = std::mem::take(args);
            *expr = NativeExpr::TailCall {
                function: *function,
                args,
                yield_continuation_id: yield_ids.get(*function).copied().flatten(),
            };
        }
        NativeExpr::CallThen {
            function,
            args,
            completion_continuation_id,
            values,
            ..
        } if values.is_empty()
            && forwarding_completions.contains(completion_continuation_id)
            && components.get(*function).copied() == Some(current_component) =>
        {
            let function = *function;
            let args = std::mem::take(args);
            *expr = NativeExpr::TailCall {
                function,
                args,
                yield_continuation_id: yield_ids.get(function).copied().flatten(),
            };
        }
        NativeExpr::Let { body, .. } => {
            lower_tail_position(
                body,
                current_component,
                components,
                yield_ids,
                forwarding_completions,
            );
        }
        NativeExpr::If { clauses } => {
            for (_, body) in clauses {
                lower_tail_position(
                    body,
                    current_component,
                    components,
                    yield_ids,
                    forwarding_completions,
                );
            }
        }
        NativeExpr::Try {
            success,
            failure,
            cleanup,
            ..
        } if cleanup.is_empty() => {
            lower_tail_position(
                success,
                current_component,
                components,
                yield_ids,
                forwarding_completions,
            );
            lower_tail_position(
                failure,
                current_component,
                components,
                yield_ids,
                forwarding_completions,
            );
        }
        _ => {}
    }
}

fn contains_dynamic_tail_target(expr: &NativeExpr) -> bool {
    match expr {
        NativeExpr::InvokeClosure { .. } => true,
        NativeExpr::Let { body, .. } => contains_dynamic_tail_target(body),
        NativeExpr::If { clauses } => clauses
            .iter()
            .any(|(_, body)| contains_dynamic_tail_target(body)),
        NativeExpr::Try {
            success,
            failure,
            cleanup,
            ..
        } if cleanup.is_empty() => {
            contains_dynamic_tail_target(success) || contains_dynamic_tail_target(failure)
        }
        _ => false,
    }
}

fn application_call_graph(
    modules: &[NativeModule],
    include_completion_edges: bool,
) -> Vec<Vec<usize>> {
    let forwarding_completions = forwarding_completion_ids(modules);
    let mut graph = modules
        .iter()
        .flat_map(|module| module.functions.iter())
        .map(|function| {
            let mut calls = Vec::new();
            collect_calls(
                &function.body,
                &forwarding_completions,
                include_completion_edges,
                &mut calls,
            );
            calls.sort_unstable();
            calls.dedup();
            calls
        })
        .collect::<Vec<_>>();
    let function_count = graph.len();
    for calls in &mut graph {
        calls.retain(|function| *function < function_count);
    }
    graph
}

fn forwarding_completion_ids(modules: &[NativeModule]) -> HashSet<u64> {
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    modules
        .iter()
        .flat_map(|module| &module.continuations)
        .filter_map(|continuation| {
            if continuation.params.len() != 1 {
                return None;
            }
            let forwards_result = match &continuation.body {
                NativeExpr::Param(0) => true,
                NativeExpr::TailCall { function, args, .. } => {
                    args.as_slice() == [NativeExpr::Param(0)]
                        && functions
                            .get(*function)
                            .is_some_and(|function| matches!(function.body, NativeExpr::Param(0)))
                }
                _ => false,
            };
            forwards_result.then_some(continuation.id)
        })
        .collect()
}

fn component_sizes(components: &[usize]) -> Vec<usize> {
    components
        .iter()
        .copied()
        .max()
        .map_or(Vec::new(), |maximum| {
            let mut sizes = vec![0_usize; maximum.saturating_add(1)];
            for component in components {
                sizes[*component] = sizes[*component].saturating_add(1);
            }
            sizes
        })
}

fn collect_calls(
    expr: &NativeExpr,
    forwarding_completions: &HashSet<u64>,
    include_completion_edges: bool,
    calls: &mut Vec<usize>,
) {
    match expr {
        NativeExpr::Call { function, .. } | NativeExpr::TailCall { function, .. } => {
            calls.push(*function);
        }
        NativeExpr::CallThen {
            function,
            completion_continuation_id,
            completion_function,
            values,
            ..
        } => {
            // A suspending call has two control successors: the callee while
            // it is active and the materialized completion after its VM frame
            // is popped.  Completion edges are required to discover recursive
            // SCCs that cross generated continuation functions.
            if values.is_empty() && forwarding_completions.contains(completion_continuation_id) {
                calls.push(*function);
            }
            if include_completion_edges {
                if let Some(completion) = completion_function {
                    calls.push(*completion);
                }
            }
        }
        NativeExpr::InvokeClosureThen {
            completion_function: Some(completion),
            ..
        } if include_completion_edges => calls.push(*completion),
        NativeExpr::Let { body, .. } => collect_calls(
            body,
            forwarding_completions,
            include_completion_edges,
            calls,
        ),
        NativeExpr::If { clauses } => {
            for (_, body) in clauses {
                collect_calls(
                    body,
                    forwarding_completions,
                    include_completion_edges,
                    calls,
                );
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } if cleanup.is_empty() => {
            collect_calls(
                success,
                forwarding_completions,
                include_completion_edges,
                calls,
            );
            collect_calls(
                failure,
                forwarding_completions,
                include_completion_edges,
                calls,
            );
        }
        _ => {}
    }
}

fn collect_resume_ids(expr: &NativeExpr, identities: &mut HashSet<u64>) {
    match expr {
        NativeExpr::CallThen {
            args,
            resumes,
            values,
            ..
        } => {
            identities.extend(resumes.iter().map(|resume| resume.callee_continuation_id));
            for value in args.iter().chain(values) {
                collect_resume_ids(value, identities);
            }
        }
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. }
        | NativeExpr::ContinuationTailCall { args: fields, .. } => {
            for field in fields {
                collect_resume_ids(field, identities);
            }
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            collect_resume_ids(callee, identities);
            for arg in args {
                collect_resume_ids(arg, identities);
            }
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            resumes,
            values,
            ..
        } => {
            identities.extend(resumes.iter().map(|resume| resume.callee_continuation_id));
            collect_resume_ids(callee, identities);
            for value in args.iter().chain(values) {
                collect_resume_ids(value, identities);
            }
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => collect_resume_ids(value, identities),
        NativeExpr::Binary { left, right, .. } => {
            collect_resume_ids(left, identities);
            collect_resume_ids(right, identities);
        }
        NativeExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_resume_ids(binding, identities);
            }
            collect_resume_ids(body, identities);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                collect_resume_ids(condition, identities);
                collect_resume_ids(body, identities);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            collect_resume_ids(protected, identities);
            collect_resume_ids(success, identities);
            collect_resume_ids(failure, identities);
            for value in cleanup {
                collect_resume_ids(value, identities);
            }
        }
        NativeExpr::Suspend {
            arguments, values, ..
        } => {
            for value in arguments.iter().chain(values) {
                collect_resume_ids(value, identities);
            }
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
}

fn attach_reduction_yields(
    expr: &mut NativeExpr,
    function_identities: &[u64],
    installed: &HashSet<u64>,
) {
    match expr {
        NativeExpr::TailCall {
            function,
            args,
            yield_continuation_id,
        } => {
            for arg in args.iter_mut() {
                attach_reduction_yields(arg, function_identities, installed);
            }
            if let Some(id) = function_identities.get(*function).copied() {
                if installed.contains(&id) && yield_continuation_id.is_none() {
                    let values = std::mem::take(args);
                    *expr = NativeExpr::Suspend {
                        operation: NativeTransitionOperation::Yield,
                        arguments: Vec::new(),
                        continuation_id: id,
                        values,
                    };
                }
            }
        }
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::ContinuationTailCall { args: fields, .. } => {
            for field in fields {
                attach_reduction_yields(field, function_identities, installed);
            }
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            attach_reduction_yields(callee, function_identities, installed);
            for arg in args {
                attach_reduction_yields(arg, function_identities, installed);
            }
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            attach_reduction_yields(callee, function_identities, installed);
            for value in args.iter_mut().chain(values) {
                attach_reduction_yields(value, function_identities, installed);
            }
        }
        NativeExpr::CallThen { args, values, .. }
        | NativeExpr::Suspend {
            arguments: args,
            values,
            ..
        } => {
            for value in args.iter_mut().chain(values) {
                attach_reduction_yields(value, function_identities, installed);
            }
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => {
            attach_reduction_yields(value, function_identities, installed);
        }
        NativeExpr::Binary { left, right, .. } => {
            attach_reduction_yields(left, function_identities, installed);
            attach_reduction_yields(right, function_identities, installed);
        }
        NativeExpr::Let { bindings, body } => {
            for binding in bindings {
                attach_reduction_yields(binding, function_identities, installed);
            }
            attach_reduction_yields(body, function_identities, installed);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                attach_reduction_yields(condition, function_identities, installed);
                attach_reduction_yields(body, function_identities, installed);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            attach_reduction_yields(protected, function_identities, installed);
            attach_reduction_yields(success, function_identities, installed);
            attach_reduction_yields(failure, function_identities, installed);
            for value in cleanup {
                attach_reduction_yields(value, function_identities, installed);
            }
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
}

/// Computes canonical SCC identities with iterative Kosaraju traversals.
fn strongly_connected_components(graph: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; graph.len()];
    let mut finish_order = Vec::with_capacity(graph.len());
    for start in 0..graph.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0_usize)];
        while let Some((node, edge)) = stack.last_mut() {
            if *edge < graph[*node].len() {
                let next = graph[*node][*edge];
                *edge = edge.saturating_add(1);
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }

    let mut reverse = vec![Vec::new(); graph.len()];
    for (source, targets) in graph.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }
    for predecessors in &mut reverse {
        predecessors.sort_unstable();
        predecessors.dedup();
    }

    let mut components = vec![usize::MAX; graph.len()];
    let mut component = 0_usize;
    while let Some(start) = finish_order.pop() {
        if components[start] != usize::MAX {
            continue;
        }
        components[start] = component;
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for predecessor in &reverse[node] {
                if components[*predecessor] == usize::MAX {
                    components[*predecessor] = component;
                    stack.push(*predecessor);
                }
            }
        }
        component = component.saturating_add(1);
    }
    components
}
