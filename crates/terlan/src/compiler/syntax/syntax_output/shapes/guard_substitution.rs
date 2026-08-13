use super::*;

pub(super) fn substitute_guard_expr(
    mut expression: SyntaxExprOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<SyntaxExprOutput> {
    if guard_uses_only_child_expressions(&expression) {
        return substitute_plain_guard_expr(expression, substitutions, shape_name);
    }
    if expression.kind == SyntaxExprKind::Var {
        if let Some((param, pattern)) = expression
            .text
            .as_ref()
            .and_then(|name| substitutions.get_key_value(name))
        {
            return guard_value_from_pattern(pattern).ok_or_else(|| {
                EbnfCompileError::Serialize(format!(
                    "shape `{shape_name}` guard references parameter `{param}` with a non-value pattern argument"
                ))
            });
        }
    }
    expression.children = expression
        .children
        .into_iter()
        .map(|child| substitute_guard_expr(child, substitutions, shape_name))
        .collect::<EbnfCompileResult<Vec<_>>>()?;
    expression.let_guards = expression
        .let_guards
        .into_iter()
        .map(|guard| {
            guard
                .map(|guard| substitute_guard_expr(*guard, substitutions, shape_name).map(Box::new))
                .transpose()
        })
        .collect::<EbnfCompileResult<Vec<_>>>()?;
    for field in &mut expression.fields {
        *field.value = substitute_guard_expr((*field.value).clone(), substitutions, shape_name)?;
    }
    for clause in &mut expression.clauses {
        substitute_clause_guard_expr(clause, substitutions, shape_name)?;
    }
    for clause in &mut expression.catch_clauses {
        substitute_clause_guard_expr(clause, substitutions, shape_name)?;
    }
    if let Some(after) = &mut expression.try_after {
        *after.trigger =
            substitute_guard_expr((*after.trigger).clone(), substitutions, shape_name)?;
        *after.body = substitute_guard_expr((*after.body).clone(), substitutions, shape_name)?;
    }
    for node in &mut expression.html_nodes {
        substitute_html_guard_expr(node, substitutions, shape_name)?;
    }
    Ok(expression)
}

/// Reports whether guard substitution only needs the ordinary child vector.
fn guard_uses_only_child_expressions(expression: &SyntaxExprOutput) -> bool {
    let mut pending = vec![expression];
    while let Some(current) = pending.pop() {
        if !current.let_guards.is_empty()
            || !current.fields.is_empty()
            || !current.clauses.is_empty()
            || !current.catch_clauses.is_empty()
            || current.try_after.is_some()
            || !current.html_nodes.is_empty()
        {
            return false;
        }
        pending.extend(&current.children);
    }
    true
}

/// Substitutes plain guard variables with an explicit traversal stack.
fn substitute_plain_guard_expr(
    mut expression: SyntaxExprOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<SyntaxExprOutput> {
    let mut pending = vec![Vec::<usize>::new()];
    while let Some(path) = pending.pop() {
        let mut current = &mut expression;
        for index in &path {
            current = &mut current.children[*index];
        }
        if current.kind == SyntaxExprKind::Var {
            if let Some((param, pattern)) = current
                .text
                .as_ref()
                .and_then(|name| substitutions.get_key_value(name))
            {
                *current = guard_value_from_pattern(pattern).ok_or_else(|| {
                    EbnfCompileError::Serialize(format!(
                        "shape `{shape_name}` guard references parameter `{param}` with a non-value pattern argument"
                    ))
                })?;
                continue;
            }
        }
        for index in (0..current.children.len()).rev() {
            let mut child = path.clone();
            child.push(index);
            pending.push(child);
        }
    }
    Ok(expression)
}

fn substitute_clause_guard_expr(
    clause: &mut SyntaxClauseOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    if let Some(guard) = &mut clause.guard {
        **guard = substitute_guard_expr((**guard).clone(), substitutions, shape_name)?;
    }
    *clause.body = substitute_guard_expr((*clause.body).clone(), substitutions, shape_name)?;
    Ok(())
}

fn substitute_html_guard_expr(
    node: &mut SyntaxHtmlNodeOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => Ok(()),
        SyntaxHtmlNodeOutput::Expr { expr } => {
            **expr = substitute_guard_expr((**expr).clone(), substitutions, shape_name)?;
            Ok(())
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &mut slot.children {
                substitute_html_guard_expr(child, substitutions, shape_name)?;
            }
            Ok(())
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &mut element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &mut attr.value {
                    **expr = substitute_guard_expr((**expr).clone(), substitutions, shape_name)?;
                }
            }
            for child in &mut element.children {
                substitute_html_guard_expr(child, substitutions, shape_name)?;
            }
            Ok(())
        }
    }
}
