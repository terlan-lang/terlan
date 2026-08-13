use super::*;

pub(crate) fn core_function_clause_summary(
    clause: &crate::terlan_syntax::SyntaxFunctionClauseOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
    function_value_locals: &HashSet<String>,
) -> CoreFunctionClause {
    let patterns = clause
        .patterns
        .iter()
        .map(core_pattern_summary_text)
        .collect();
    let core_patterns: Vec<Option<CorePattern>> = clause
        .patterns
        .iter()
        .map(core_pattern_from_syntax)
        .collect();
    let pattern_proof_coverage = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(pattern, core_pattern)| core_pattern_proof_coverage(pattern, core_pattern.as_ref()))
        .collect();
    let pattern_checked_preservation_evidence = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(_, core_pattern)| {
            core_pattern
                .as_ref()
                .and_then(core_pattern_checked_preservation_evidence)
        })
        .collect();
    CoreFunctionClause {
        patterns,
        core_patterns,
        pattern_proof_coverage,
        pattern_checked_preservation_evidence,
        guard: clause.guard.as_ref().map(|guard| {
            core_expr_summary(
                guard,
                receiver_methods,
                template_prop_order,
                function_value_locals,
            )
        }),
        body: core_expr_summary(
            &clause.body,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        ),
    }
}

pub(crate) fn function_value_parameter_names(
    params: &[crate::terlan_syntax::SyntaxParamOutput],
) -> HashSet<String> {
    let mut names = HashSet::new();
    for param in params {
        let mut vars = HashMap::new();
        let mut next_var = 0;
        if matches!(
            parse_type_expr(
                &param.annotation.text,
                &HashSet::new(),
                &mut vars,
                &mut next_var
            ),
            Some(Type::Function { .. })
        ) {
            names.insert(param.name.clone());
        }
    }
    names
}

/// Converts a syntax expression into a recursive CoreIR expression summary.
///
/// Inputs:
/// - `expr`: syntax-output expression.
///
/// Output:
/// - Core expression summary.
///
/// Transformation:
/// - Preserves semantic expression kind, arity, text, remote target, operator,
///   and recursively summarized child expressions while intentionally omitting
///   backend rendering details.
fn core_expr_summary(
    expr: &SyntaxExprOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
    function_value_locals: &HashSet<String>,
) -> CoreExprSummary {
    let mut children = expr
        .children
        .iter()
        .map(|child| {
            core_expr_summary(
                child,
                receiver_methods,
                template_prop_order,
                function_value_locals,
            )
        })
        .collect::<Vec<_>>();
    children.extend(expr.fields.iter().map(|field| {
        core_expr_summary(
            &field.value,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        )
    }));
    children.extend(expr.clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(
                guard,
                receiver_methods,
                template_prop_order,
                function_value_locals,
            ));
        }
        clause_children.push(core_expr_summary(
            &clause.body,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        ));
        clause_children
    }));
    children.extend(expr.catch_clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(
                guard,
                receiver_methods,
                template_prop_order,
                function_value_locals,
            ));
        }
        clause_children.push(core_expr_summary(
            &clause.body,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        ));
        clause_children
    }));
    if let Some(after) = &expr.try_after {
        children.push(core_expr_summary(
            &after.trigger,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        ));
        children.push(core_expr_summary(
            &after.body,
            receiver_methods,
            template_prop_order,
            function_value_locals,
        ));
    }
    let core_expr = core_mutable_receiver_call_expr_from_syntax(expr, receiver_methods)
        .or_else(|| core_template_call_expr_from_syntax(expr, template_prop_order))
        .or_else(|| {
            core_function_value_parameter_call_expr_from_syntax(expr, function_value_locals)
        })
        .or_else(|| core_expr_from_syntax(expr));
    let checked_preservation_evidence = core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    let proof_coverage = core_expr_proof_coverage(expr, core_expr.as_ref());

    CoreExprSummary {
        kind: format!("{:?}", expr.kind),
        core_expr,
        checked_preservation_evidence,
        proof_coverage,
        text: expr.text.clone(),
        remote: expr.remote.clone(),
        operator: expr.operator.clone(),
        arity: expr.arity,
        children,
    }
}

fn core_function_value_parameter_call_expr_from_syntax(
    expr: &SyntaxExprOutput,
    function_value_locals: &HashSet<String>,
) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Var => callee.text.as_deref()?,
        _ => return None,
    };
    if !function_value_locals.contains(name) {
        return None;
    }

    Some(CoreExpr::FunctionCall {
        callee: Box::new(core_expr_from_syntax(callee)?),
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Converts a direct generated template function call into CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be a direct template call.
/// - `template_prop_order`: template names mapped to declaration-order props.
///
/// Output:
/// - `Some(CoreExpr::TemplateInstantiate)` when `expr` is a local direct call
///   to a declared template and all provided argument values lower to Core.
/// - `None` for non-template calls or unsupported argument expressions.
///
/// Transformation:
/// - Maps positional call arguments to declaration-order props and named
///   arguments to exact prop keys, preserving the same backend-neutral shape as
///   `Page{...}` template instantiation.
fn core_template_call_expr_from_syntax(
    expr: &SyntaxExprOutput,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref()?,
        _ => return None,
    };
    let prop_order = template_prop_order.get(name)?;
    let mut fields = Vec::with_capacity(args.len());
    let mut next_positional_index = 0;
    for (index, arg) in args.iter().enumerate() {
        let key = if let Some(arg_name) = expr.arg_names.get(index).and_then(Option::as_ref) {
            arg_name.clone()
        } else {
            let prop_name = prop_order.get(next_positional_index)?;
            next_positional_index += 1;
            prop_name.clone()
        };
        fields.push(CoreRecordExprField {
            key,
            required: true,
            value: core_expr_from_syntax(arg)?,
        });
    }

    Some(CoreExpr::TemplateInstantiate {
        name: name.to_string(),
        fields,
    })
}
