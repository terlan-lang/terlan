use super::{
    super::{
        super::{
            lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind as Kind},
            parse_tree::{
                ConstFunctionDecl, ConstructorClause, ConstructorDecl, ConstructorParam, Decl,
                FunctionClause, FunctionDecl, MethodDecl, Param, Pattern, TypeExpr,
            },
        },
        LalrpopLoweringContext, LalrpopLoweringResult,
    },
    declaration_identity, head_constraint_texts, metadata_bool, metadata_count,
    without_annotations,
};

pub(super) fn lower_function(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    if node
        .text
        .as_deref()
        .is_some_and(|text| text.ends_with("clauses"))
    {
        return lower_clause_group(context, node, docs).map(Decl::Function);
    }
    let children = without_annotations(node);
    let (name, is_public, is_const, is_macro) = declaration_identity(node);
    let generic_count = metadata_count(node, "generics");
    let head_constraint_count = metadata_count(node, "head_constraints");
    let parameter_count = metadata_count(node, "params");
    let constraint_count = metadata_count(node, "constraints");
    if is_const {
        let parameter_count = metadata_count(node, "params");
        let params = children
            .get(..parameter_count)
            .ok_or_else(|| context.error(node, "constant function parameters are malformed"))?
            .iter()
            .map(|parameter| lower_parameter(context, parameter))
            .collect::<LalrpopLoweringResult<Vec<_>>>()?;
        let return_type = children
            .get(parameter_count)
            .map(|child| context.type_expression(child))
            .ok_or_else(|| context.error(node, "constant function return type is missing"))?;
        let body = children
            .get(parameter_count + 1)
            .map(|child| context.expression(child))
            .transpose()?
            .ok_or_else(|| context.error(node, "constant function body is missing"))?;
        return Ok(Decl::ConstFunction(ConstFunctionDecl {
            name,
            params,
            return_type,
            body,
            is_public,
            docs,
            span: node.span,
        }));
    }
    let head_constraints_start = generic_count;
    let params_start = head_constraints_start + head_constraint_count;
    let constraints_start = params_start + parameter_count;
    let return_index = constraints_start + constraint_count;
    let generic_params = text_list(context, children.get(..generic_count).unwrap_or_default());
    let mut params = children
        .get(params_start..constraints_start)
        .ok_or_else(|| context.error(node, "function parameter metadata is malformed"))?
        .iter()
        .map(|parameter| lower_parameter(context, parameter))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    reject_function_varargs(context, &children[params_start..constraints_start])?;
    normalize_structural_parameter_names(context, children, params_start, &mut params);
    validate_parameter_defaults(context, node, &params, children, params_start)?;
    let mut generic_bounds = head_constraint_texts(
        context,
        children
            .get(head_constraints_start..params_start)
            .unwrap_or_default(),
    );
    generic_bounds.extend(text_list(
        context,
        children
            .get(constraints_start..return_index)
            .unwrap_or_default(),
    ));
    let return_type = if metadata_bool(node, "script") {
        TypeExpr {
            text: "Dynamic".to_string(),
            span: node.span,
        }
    } else {
        children
            .get(return_index)
            .map(|child| context.type_expression(child))
            .ok_or_else(|| context.error(node, "function return type is missing"))?
    };
    let body = metadata_bool(node, "body")
        .then(|| {
            children
                .get(return_index + 1)
                .ok_or_else(|| context.error(node, "function body metadata is malformed"))
                .and_then(|body| context.expression(body))
        })
        .transpose()?;
    let clauses = if let Some(body) = body {
        let patterns = children[params_start..constraints_start]
            .iter()
            .zip(&params)
            .map(|(node, parameter)| parameter_pattern(context, node, parameter))
            .collect::<LalrpopLoweringResult<Vec<_>>>()?;
        vec![FunctionClause {
            patterns,
            body,
            span: callable_span(context, node, &name),
            guard: None,
        }]
    } else {
        Vec::new()
    };
    let span = callable_span(context, node, &name);
    Ok(Decl::Function(FunctionDecl {
        name,
        generic_params,
        params,
        return_type,
        is_public,
        is_macro,
        generic_bounds,
        clauses,
        docs,
        span,
    }))
}

fn callable_span(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    name: &str,
) -> crate::terlan_syntax::span::Span {
    let start = context
        .text(node.span)
        .find(name)
        .map_or(node.span.start, |offset| node.span.start + offset);
    let source = context.text(node.span);
    let trimmed = source.trim_end();
    let end = trimmed
        .strip_suffix('.')
        .map_or(node.span.end, |without_dot| {
            node.span.start + without_dot.len()
        });
    crate::terlan_syntax::span::Span::new(start, end)
}

fn lower_clause_group(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<FunctionDecl> {
    let first = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "function clause group is empty"))?;
    let name = clause_name(first);
    let clauses = node
        .children
        .iter()
        .map(|clause| lower_clause(context, clause))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let arity = metadata_count(first, "patterns");
    for clause in node.children.iter().skip(1) {
        if clause_name(clause) != name {
            return Err(context.error(clause, "expected Dot"));
        }
        let clause_arity = metadata_count(clause, "patterns");
        if clause_arity != arity {
            return Err(context.error(
                clause,
                format!("clause for {name} has arity {clause_arity}, expected {arity}"),
            ));
        }
    }
    Ok(FunctionDecl {
        name,
        generic_params: Vec::new(),
        params: (0..arity)
            .map(|index| Param {
                name: format!("arg{index}"),
                annotation: dynamic_type(node),
                is_mutable: false,
                default: None,
                span: node.span,
            })
            .collect(),
        return_type: dynamic_type(node),
        is_public: node
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with("pub ")),
        is_macro: false,
        generic_bounds: Vec::new(),
        clauses,
        docs,
        span: node.span,
    })
}

fn lower_clause(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<FunctionClause> {
    let pattern_count = metadata_count(node, "patterns");
    let has_guard = metadata_bool(node, "guard");
    let body_index = pattern_count + usize::from(has_guard);
    let patterns = node
        .children
        .get(..pattern_count)
        .ok_or_else(|| context.error(node, "function clause patterns are malformed"))?
        .iter()
        .map(|pattern| context.pattern(pattern))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let guard = has_guard
        .then(|| {
            node.children
                .get(pattern_count)
                .ok_or_else(|| context.error(node, "function clause guard is missing"))
                .and_then(|guard| context.expression(guard).map(Box::new))
        })
        .transpose()?;
    let body = node
        .children
        .get(body_index)
        .ok_or_else(|| context.error(node, "function clause body is missing"))
        .and_then(|body| context.expression(body))?;
    Ok(FunctionClause {
        patterns,
        body,
        span: node.span,
        guard,
    })
}

pub(super) fn lower_method(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let receiver = children
        .first()
        .ok_or_else(|| context.error(node, "method receiver is missing"))?;
    let receiver_type = receiver
        .children
        .first()
        .map(|child| context.type_expression(child))
        .ok_or_else(|| context.error(receiver, "method receiver type is missing"))?;
    let receiver_text = receiver.text.as_deref().unwrap_or_default();
    let receiver_mutable = receiver_text.starts_with("mut ");
    let receiver_name = receiver_text
        .strip_prefix("mut ")
        .unwrap_or(receiver_text)
        .to_string();
    let generic_count = metadata_count(node, "generics");
    let head_constraint_count = metadata_count(node, "head_constraints");
    let parameter_count = metadata_count(node, "params");
    let constraint_count = metadata_count(node, "constraints");
    let head_constraints_start = 1 + generic_count;
    let params_start = head_constraints_start + head_constraint_count;
    let constraints_start = params_start + parameter_count;
    let return_index = constraints_start + constraint_count;
    let mut params = children[params_start..constraints_start]
        .iter()
        .map(|parameter| lower_parameter(context, parameter))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    reject_function_varargs(context, &children[params_start..constraints_start])?;
    normalize_structural_parameter_names(context, children, params_start, &mut params);
    validate_parameter_defaults(context, node, &params, children, params_start)?;
    let (name, is_public, _, _) = declaration_identity(node);
    Ok(Decl::Method(MethodDecl {
        receiver: Param {
            name: receiver_name,
            annotation: receiver_type,
            is_mutable: receiver_mutable,
            default: None,
            span: receiver.span,
        },
        name,
        generic_params: text_list(context, &children[1..1 + generic_count]),
        params: params.clone(),
        return_type: context.type_expression(
            children
                .get(return_index)
                .ok_or_else(|| context.error(node, "method return type is missing"))?,
        ),
        is_public,
        generic_bounds: {
            let mut bounds =
                head_constraint_texts(context, &children[head_constraints_start..params_start]);
            bounds.extend(text_list(
                context,
                &children[constraints_start..return_index],
            ));
            bounds
        },
        clauses: metadata_bool(node, "body")
            .then(|| {
                Ok(FunctionClause {
                    patterns: children[params_start..constraints_start]
                        .iter()
                        .zip(&params)
                        .map(|(node, parameter)| parameter_pattern(context, node, parameter))
                        .collect::<LalrpopLoweringResult<Vec<_>>>()?,
                    body: context.expression(
                        children
                            .get(return_index + 1)
                            .ok_or_else(|| context.error(node, "method body is missing"))?,
                    )?,
                    span: node.span,
                    guard: None,
                })
            })
            .transpose()?
            .into_iter()
            .collect(),
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_constructor(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let parameter_count = metadata_count(node, "params");
    let (name, is_public, _, _) = declaration_identity(node);
    let clauses = children[parameter_count..]
        .iter()
        .map(|clause| {
            let count = metadata_count(clause, "params");
            let params = clause.children[..count]
                .iter()
                .map(|parameter| lower_constructor_parameter(context, parameter))
                .collect::<LalrpopLoweringResult<Vec<_>>>()?;
            validate_constructor_parameters(context, clause, &params)?;
            Ok(ConstructorClause {
                params,
                return_type: context.type_expression(&clause.children[count]),
                body: context.expression(&clause.children[count + 1])?,
                span: clause.span,
            })
        })
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    validate_constructor_clauses(context, node, &clauses)?;
    Ok(Decl::Constructor(ConstructorDecl {
        name,
        params: text_list(context, &children[..parameter_count]),
        clauses,
        is_public,
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_parameter(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Param> {
    let text = node.text.as_deref().unwrap_or_default();
    let is_mutable = text.starts_with("mut ") || text.starts_with("...mut ");
    let name = text
        .strip_prefix("...")
        .unwrap_or(text)
        .strip_prefix("mut ")
        .unwrap_or(text.strip_prefix("...").unwrap_or(text))
        .to_string();
    let annotation_index = usize::from(
        node.children
            .first()
            .is_some_and(|child| !is_type_kind(child.kind)),
    );
    let annotation = node
        .children
        .get(annotation_index)
        .filter(|child| is_type_kind(child.kind))
        .map(|child| context.type_expression(child))
        .unwrap_or_else(|| dynamic_type(node));
    let default = node
        .children
        .get(annotation_index + 1)
        .map(|child| context.expression(child))
        .transpose()?;
    Ok(Param {
        name,
        annotation,
        is_mutable,
        default,
        span: node.span,
    })
}

fn lower_constructor_parameter(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<ConstructorParam> {
    let parameter = lower_parameter(context, node)?;
    Ok(ConstructorParam {
        name: parameter.name,
        annotation: parameter.annotation,
        default: parameter.default,
        is_varargs: node
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with("...")),
        span: node.span,
    })
}

fn reject_function_varargs(
    context: &LalrpopLoweringContext<'_>,
    parameters: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<()> {
    if let Some(parameter) = parameters.iter().find(|parameter| {
        parameter
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with("..."))
    }) {
        return Err(context.error(
            parameter,
            "function varargs parameters are not supported in Terlan 0.0.1",
        ));
    }
    Ok(())
}

fn is_type_kind(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Type
            | Kind::TypeUnion
            | Kind::TypeArrow
            | Kind::TypeExistential
            | Kind::TypeTuple
            | Kind::TypeMap
            | Kind::TypeList
    )
}

fn text_list(context: &LalrpopLoweringContext<'_>, nodes: &[LalrpopSyntaxNode]) -> Vec<String> {
    nodes.iter().map(|node| context.type_text(node)).collect()
}

fn dynamic_type(node: &LalrpopSyntaxNode) -> TypeExpr {
    TypeExpr {
        text: "Dynamic".to_string(),
        span: node.span,
    }
}

fn clause_name(node: &LalrpopSyntaxNode) -> String {
    node.text
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn normalize_structural_parameter_names(
    context: &LalrpopLoweringContext<'_>,
    children: &[LalrpopSyntaxNode],
    start: usize,
    params: &mut [Param],
) {
    for (index, (node, parameter)) in children[start..start + params.len()]
        .iter()
        .zip(params)
        .enumerate()
    {
        if node
            .children
            .first()
            .is_some_and(|child| !is_type_kind(child.kind))
            && node.text.as_deref() == node.children.first().map(|child| context.text(child.span))
        {
            parameter.name = format!("_Arg{}", index + 1);
        }
    }
}

fn parameter_pattern(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    parameter: &Param,
) -> LalrpopLoweringResult<Pattern> {
    let Some(pattern) = node
        .children
        .first()
        .filter(|child| !is_type_kind(child.kind))
    else {
        return Ok(Pattern::Var(parameter.name.clone()));
    };
    let pattern = context.pattern(pattern)?;
    if parameter.name.starts_with("_Arg") {
        Ok(pattern)
    } else {
        Ok(Pattern::Alias {
            alias: parameter.name.clone(),
            pattern: Box::new(pattern),
        })
    }
}

fn validate_parameter_defaults(
    context: &LalrpopLoweringContext<'_>,
    owner: &LalrpopSyntaxNode,
    params: &[Param],
    children: &[LalrpopSyntaxNode],
    start: usize,
) -> LalrpopLoweringResult<()> {
    let mut saw_default = false;
    for (parameter, node) in params.iter().zip(&children[start..start + params.len()]) {
        if parameter.default.is_some()
            && node
                .children
                .first()
                .is_some_and(|child| !is_type_kind(child.kind))
        {
            return Err(context.error(
                node,
                "function-head pattern parameters do not support defaults in 0.0.7; use plain named parameters for defaults",
            ));
        }
        if saw_default && parameter.default.is_none() {
            return Err(context.error(owner, "default parameters must be trailing"));
        }
        saw_default |= parameter.default.is_some();
    }
    Ok(())
}

fn validate_constructor_parameters(
    context: &LalrpopLoweringContext<'_>,
    clause: &LalrpopSyntaxNode,
    params: &[ConstructorParam],
) -> LalrpopLoweringResult<()> {
    if params
        .iter()
        .any(|parameter| parameter.is_varargs && parameter.default.is_some())
    {
        return Err(context.error(
            clause,
            "constructor varargs parameters cannot have defaults",
        ));
    }
    if params
        .iter()
        .enumerate()
        .any(|(index, parameter)| parameter.is_varargs && index + 1 != params.len())
    {
        return Err(context.error(clause, "constructor varargs parameter must be last"));
    }
    let mut saw_default = false;
    for parameter in params {
        if saw_default && parameter.default.is_none() && !parameter.is_varargs {
            return Err(context.error(clause, "constructor default parameters must be trailing"));
        }
        saw_default |= parameter.default.is_some();
    }
    Ok(())
}

fn validate_constructor_clauses(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    clauses: &[ConstructorClause],
) -> LalrpopLoweringResult<()> {
    for (index, left) in clauses.iter().enumerate() {
        for right in &clauses[index + 1..] {
            let (left_min, left_max, left_varargs) = constructor_arity_range(left);
            let (right_min, right_max, right_varargs) = constructor_arity_range(right);
            if left_min <= right_max && right_min <= left_max {
                if left_varargs && right_varargs {
                    return Err(context.error(node, "constructor has ambiguous varargs clauses"));
                }
                if !left_varargs && !right_varargs {
                    return Err(context.error(node, "constructor has ambiguous arity clauses"));
                }
            }
        }
    }
    Ok(())
}

fn constructor_arity_range(clause: &ConstructorClause) -> (usize, usize, bool) {
    let varargs = clause
        .params
        .last()
        .is_some_and(|parameter| parameter.is_varargs);
    let minimum = clause
        .params
        .iter()
        .filter(|parameter| parameter.default.is_none() && !parameter.is_varargs)
        .count();
    let maximum = if varargs {
        usize::MAX
    } else {
        clause.params.len()
    };
    (minimum, maximum, varargs)
}
