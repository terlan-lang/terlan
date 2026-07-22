fn lower_checked_valued_union_parsing(
    module: &mut SyntaxModuleOutput,
    values: &HashMap<String, ConstValue>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    let mut unions = HashMap::<String, Vec<ConstValue>>::new();
    for (qualified, value) in values {
        let ConstValue::Union { .. } = value else {
            continue;
        };
        let Some((owner, arm)) = qualified.rsplit_once('.') else {
            continue;
        };
        if owner.contains('.') || arm.is_empty() {
            continue;
        }
        unions.entry(owner.to_string()).or_default().push(value.clone());
    }
    for arms in unions.values_mut() {
        arms.sort_by_key(ConstValue::stable_text);
    }

    for declaration in &mut module.declarations {
        visit_declaration_exprs_mut(&mut declaration.payload, |expr| {
            lower_checked_parse_expr(expr, &unions, diagnostics);
        });
    }
}

fn lower_checked_parse_expr(
    expr: &mut SyntaxExprOutput,
    unions: &HashMap<String, Vec<ConstValue>>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    if !matches!(expr.kind, SyntaxExprKind::Call | SyntaxExprKind::FunctionCall) {
        return;
    }
    let Some(callee) = expr.children.first() else {
        return;
    };
    let owner = if expr.remote.is_some() && callee.text.as_deref() == Some("parse") {
        expr.remote.clone()
    } else if callee.kind == SyntaxExprKind::FieldAccess
        && callee.text.as_deref() == Some("parse")
    {
        callee.children.first().and_then(qualified_expr_name)
    } else {
        None
    };
    let Some(owner) = owner else {
        return;
    };
    let Some(arms) = unions.get(&owner) else {
        return;
    };
    if expr.children.len() != 2 {
        diagnostics.push(diagnostic(
            "VALUED_UNION_PARSE_ARITY",
            format!("`{owner}.parse` expects exactly one representation value"),
            expr.span,
        ));
        return;
    }

    let mut clauses = arms
        .iter()
        .filter_map(|value| checked_parse_clause(value, expr.span))
        .collect::<Vec<_>>();
    if let Some(failure) = checked_parse_failure_clause(arms, expr.span) {
        clauses.push(failure);
    }
    let scrutinee = expr.children[1].clone();
    *expr = SyntaxExprOutput {
        kind: SyntaxExprKind::Case,
        arity: clauses.len(),
        text: None,
        span: expr.span,
        raw: Some(format!("checked_valued_union_parse:{owner}")),
        comprehension_lift: None,
        type_args: Vec::new(),
        operator: None,
        remote: None,
        arg_names: Vec::new(),
        children: vec![scrutinee],
        patterns: Vec::new(),
        let_guards: Vec::new(),
        fields: Vec::new(),
        clauses,
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    };
}

fn checked_parse_failure_clause(
    arms: &[ConstValue],
    span: EbnfSourceSpan,
) -> Option<SyntaxClauseOutput> {
    let first = arms.first()?;
    let ConstValue::Union { representation, .. } = first else {
        return None;
    };
    let invalid_pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Var,
        arity: 0,
        text: Some("invalid_representation".to_string()),
        children: Vec::new(),
        fields: Vec::new(),
    };
    let invalid_value = empty_checked_expr(
        SyntaxExprKind::Var,
        Some("invalid_representation".to_string()),
        span,
    );
    let mut assertion = empty_checked_expr(SyntaxExprKind::Let, None, span);
    assertion.arity = 1;
    assertion.raw = Some("checked_valued_union_parse_failure".to_string());
    assertion.children = vec![invalid_value, value_to_expr(first, span)];
    assertion.patterns = vec![value_to_pattern(representation)?];
    assertion.let_guards = vec![None];
    Some(SyntaxClauseOutput {
        patterns: vec![invalid_pattern],
        guard: None,
        body: Box::new(assertion),
    })
}

fn empty_checked_expr(
    kind: SyntaxExprKind,
    text: Option<String>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind,
        arity: 0,
        text,
        span,
        raw: None,
        comprehension_lift: None,
        type_args: Vec::new(),
        operator: None,
        remote: None,
        arg_names: Vec::new(),
        children: Vec::new(),
        patterns: Vec::new(),
        let_guards: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

fn checked_parse_clause(
    value: &ConstValue,
    span: EbnfSourceSpan,
) -> Option<SyntaxClauseOutput> {
    let ConstValue::Union { representation, .. } = value else {
        return None;
    };
    Some(SyntaxClauseOutput {
        patterns: vec![value_to_pattern(representation)?],
        guard: None,
        body: Box::new(value_to_expr(value, span)),
    })
}
