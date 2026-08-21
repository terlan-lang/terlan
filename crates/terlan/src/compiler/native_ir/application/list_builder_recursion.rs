//! Tail normalization for recursive list builders in closed native images.

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreExprSummary, CoreFunction, CoreFunctionClause, CoreIfClause,
    CoreModule, CoreParam, CorePattern, CoreProofCoverage, CoreType,
};

/// Rewrites structurally recursive list builders into bounded tail-recursive
/// workers before suspension composition analyzes the application.
pub(super) fn normalize_recursive_list_builders(cores: &mut [CoreModule]) {
    for core in cores {
        let original_count = core.functions.len();
        let mut generated = Vec::new();
        for function_index in 0..original_count {
            let Some(plan) = list_builder_plan(&core.functions[function_index]) else {
                continue;
            };
            let (wrapper, worker, reverse) = apply_plan(&core.functions[function_index], plan);
            core.functions[function_index] = wrapper;
            generated.push(worker);
            generated.push(reverse);
        }
        core.functions.extend(generated);
    }
}

/// Names and rewritten body needed to normalize one list-producing function.
struct ListBuilderPlan {
    accumulator: String,
    worker: String,
    reverse: String,
    worker_body: CoreExpr,
}

/// Derives a normalization plan when every recursive call contributes exactly
/// one list head and can therefore be represented by an accumulator.
fn list_builder_plan(function: &CoreFunction) -> Option<ListBuilderPlan> {
    if function.native_operation.is_some()
        || !matches!(function.core_return_type, Some(CoreType::List(_)))
        || function.params.is_empty()
    {
        return None;
    }
    let [clause] = function.clauses.as_slice() else {
        return None;
    };
    let body = clause.body.core_expr.as_ref()?;
    let accumulator = fresh_local_name(function, "$aot_list_accumulator");
    let worker = format!("$aot_list_builder_{}_{}", function.name, function.arity);
    let reverse = format!("$aot_list_reverse_{}_{}", function.name, function.arity);
    let mut recursive_calls = 0;
    let worker_body = rewrite_result(
        body,
        &function.name,
        function.arity,
        &worker,
        &reverse,
        &accumulator,
        &mut recursive_calls,
    )?;
    (recursive_calls > 0).then_some(ListBuilderPlan {
        accumulator,
        worker,
        reverse,
        worker_body,
    })
}

/// Chooses a generated local that cannot shadow a source parameter.
fn fresh_local_name(function: &CoreFunction, base: &str) -> String {
    let mut candidate = base.to_string();
    let mut ordinal = 0usize;
    while function
        .params
        .iter()
        .any(|parameter| parameter.name == candidate)
    {
        ordinal += 1;
        candidate = format!("{base}_{ordinal}");
    }
    candidate
}

/// Rewrites result-producing control flow while rejecting recursive calls in
/// positions that cannot be represented by a list accumulator.
fn rewrite_result(
    expression: &CoreExpr,
    function: &str,
    arity: usize,
    worker: &str,
    reverse: &str,
    accumulator: &str,
    recursive_calls: &mut usize,
) -> Option<CoreExpr> {
    if let CoreExpr::ListCons { head, tail } = expression {
        if let CoreExpr::Call {
            function: target,
            args,
        } = tail.as_ref()
        {
            if call_matches(target, function) && args.len() == arity {
                if contains_call(head, function, arity)
                    || args
                        .iter()
                        .any(|argument| contains_call(argument, function, arity))
                {
                    return None;
                }
                *recursive_calls += 1;
                let mut worker_args = args.clone();
                worker_args.push(CoreExpr::ListCons {
                    head: head.clone(),
                    tail: Box::new(CoreExpr::Var(accumulator.to_string())),
                });
                return Some(CoreExpr::Call {
                    function: worker.to_string(),
                    args: worker_args,
                });
            }
        }
    }

    match expression {
        CoreExpr::Case { scrutinee, clauses } => {
            if contains_call(scrutinee, function, arity)
                || clauses.iter().any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| contains_call(guard, function, arity))
                })
            {
                return None;
            }
            Some(CoreExpr::Case {
                scrutinee: scrutinee.clone(),
                clauses: clauses
                    .iter()
                    .map(|clause| {
                        Some(CoreCaseClause {
                            pattern: clause.pattern.clone(),
                            guard: clause.guard.clone(),
                            body: rewrite_result(
                                &clause.body,
                                function,
                                arity,
                                worker,
                                reverse,
                                accumulator,
                                recursive_calls,
                            )?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            })
        }
        CoreExpr::If { clauses } => {
            if clauses
                .iter()
                .any(|clause| contains_call(&clause.condition, function, arity))
            {
                return None;
            }
            Some(CoreExpr::If {
                clauses: clauses
                    .iter()
                    .map(|clause| {
                        Some(CoreIfClause {
                            condition: clause.condition.clone(),
                            body: rewrite_result(
                                &clause.body,
                                function,
                                arity,
                                worker,
                                reverse,
                                accumulator,
                                recursive_calls,
                            )?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            })
        }
        CoreExpr::Let { bindings, body } => {
            if bindings
                .iter()
                .any(|binding| contains_call(&binding.value, function, arity))
            {
                return None;
            }
            Some(CoreExpr::Let {
                bindings: bindings.clone(),
                body: Box::new(rewrite_result(
                    body,
                    function,
                    arity,
                    worker,
                    reverse,
                    accumulator,
                    recursive_calls,
                )?),
            })
        }
        _ if contains_call(expression, function, arity) => None,
        _ => Some(CoreExpr::Call {
            function: reverse.to_string(),
            args: vec![CoreExpr::Var(accumulator.to_string()), expression.clone()],
        }),
    }
}

/// Reports whether an expression reaches the function being normalized.
fn contains_call(expression: &CoreExpr, function: &str, arity: usize) -> bool {
    let mut found = false;
    super::dynamic_targets::walk_calls(expression, &mut |target, args| {
        found |= call_matches(target, function) && args.len() == arity;
    });
    found
}

/// Matches local and already-qualified spellings of one call target.
fn call_matches(target: &str, function: &str) -> bool {
    target == function
        || target
            .rsplit_once('.')
            .is_some_and(|(_, name)| name == function)
}

/// Materializes the public wrapper and its two private tail-recursive workers.
fn apply_plan(
    function: &CoreFunction,
    plan: ListBuilderPlan,
) -> (CoreFunction, CoreFunction, CoreFunction) {
    let list_type = function
        .core_return_type
        .clone()
        .expect("list-builder plans require a typed return");
    let mut wrapper = function.clone();
    wrapper.clauses[0].body.core_expr = Some(CoreExpr::Call {
        function: plan.worker.clone(),
        args: function
            .params
            .iter()
            .map(|parameter| CoreExpr::Var(parameter.name.clone()))
            .chain(std::iter::once(CoreExpr::List(Vec::new())))
            .collect(),
    });

    let mut worker = function.clone();
    worker.name = plan.worker.clone();
    worker.arity += 1;
    worker.public = false;
    worker.params.push(CoreParam {
        name: plan.accumulator.clone(),
        ty: function.return_type.clone(),
        core_ty: Some(list_type.clone()),
    });
    append_clause_parameter(&mut worker.clauses[0], &plan.accumulator);
    worker.clauses[0].body.core_expr = Some(plan.worker_body);

    let pending = "$aot_list_pending";
    let output = "$aot_list_output";
    let item = "$aot_list_item";
    let rest = "$aot_list_rest";
    let reverse = CoreFunction {
        name: plan.reverse.clone(),
        arity: 2,
        public: false,
        generic_params: Vec::new(),
        native_operation: None,
        params: vec![
            CoreParam {
                name: pending.to_string(),
                ty: function.return_type.clone(),
                core_ty: Some(list_type.clone()),
            },
            CoreParam {
                name: output.to_string(),
                ty: function.return_type.clone(),
                core_ty: Some(list_type),
            },
        ],
        return_type: function.return_type.clone(),
        core_return_type: function.core_return_type.clone(),
        clauses: vec![CoreFunctionClause {
            patterns: vec![pending.to_string(), output.to_string()],
            core_patterns: vec![
                Some(CorePattern::Var(pending.to_string())),
                Some(CorePattern::Var(output.to_string())),
            ],
            pattern_proof_coverage: vec![
                CoreProofCoverage::LeanCovered,
                CoreProofCoverage::LeanCovered,
            ],
            pattern_checked_preservation_evidence: vec![None, None],
            guard: None,
            body: generated_summary(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var(pending.to_string())),
                clauses: vec![
                    CoreCaseClause {
                        pattern: CorePattern::List(Vec::new()),
                        guard: None,
                        body: CoreExpr::Var(output.to_string()),
                    },
                    CoreCaseClause {
                        pattern: CorePattern::ListCons {
                            head: Box::new(CorePattern::Var(item.to_string())),
                            tail: Box::new(CorePattern::Var(rest.to_string())),
                        },
                        guard: None,
                        body: CoreExpr::Call {
                            function: plan.reverse,
                            args: vec![
                                CoreExpr::Var(rest.to_string()),
                                CoreExpr::ListCons {
                                    head: Box::new(CoreExpr::Var(item.to_string())),
                                    tail: Box::new(CoreExpr::Var(output.to_string())),
                                },
                            ],
                        },
                    },
                ],
            }),
        }],
    };
    (wrapper, worker, reverse)
}

/// Appends one generated variable parameter to an existing function clause.
fn append_clause_parameter(clause: &mut CoreFunctionClause, name: &str) {
    clause.patterns.push(name.to_string());
    clause
        .core_patterns
        .push(Some(CorePattern::Var(name.to_string())));
    clause
        .pattern_proof_coverage
        .push(CoreProofCoverage::LeanCovered);
    clause.pattern_checked_preservation_evidence.push(None);
}

/// Wraps one generated Core expression in deterministic proof metadata.
fn generated_summary(expression: CoreExpr) -> CoreExprSummary {
    CoreExprSummary {
        kind: "native-list-builder-normalization".to_string(),
        core_expr: Some(expression),
        checked_preservation_evidence: None,
        proof_coverage: CoreProofCoverage::LeanCovered,
        text: None,
        remote: None,
        operator: None,
        arity: 0,
        children: Vec::new(),
    }
}
