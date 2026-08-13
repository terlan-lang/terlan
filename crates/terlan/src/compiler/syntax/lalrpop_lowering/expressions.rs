use super::{
    super::{
        lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind},
        parse_tree::{
            BinaryLayoutField, BinaryOp, CaseClause, Expr, FunctionClause, IfClause, LetBinding,
            ListComprehensionGenerator, MapExprField, TryAfterClause, TypeExpr, UnaryOp,
        },
    },
    binary_layout,
    patterns::{lower_pattern, unquote},
    raw_macros, LalrpopLoweringContext, LalrpopLoweringResult,
};

pub(super) fn lower_expression(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    use LalrpopSyntaxNodeKind as Kind;
    match node.kind {
        Kind::Int => parse_int(node_text(context, node))
            .map(Expr::Int)
            .ok_or_else(|| context.error(node, "invalid integer literal")),
        Kind::Float => {
            let value = node_text(context, node)
                .parse::<f64>()
                .map_err(|_| context.error(node, "invalid float literal"))?;
            if value.is_finite() {
                Ok(Expr::Float(value))
            } else {
                Err(context.error(node, "float literal must be finite"))
            }
        }
        Kind::String => Ok(Expr::Binary(node_text(context, node).to_string())),
        Kind::AtomLiteral => lower_atom_literal(context, node),
        Kind::BinaryLiteral => Err(context.error(
            node,
            "Vm binary literal syntax is not valid Terlan source; use a normal string literal",
        )),
        Kind::Binding => Ok(Expr::Var(node_text(context, node).to_string())),
        Kind::Group => only_child(context, node),
        Kind::Tuple => Ok(Expr::Tuple(lower_expressions(context, &node.children)?)),
        Kind::List => Ok(Expr::List(lower_expressions(context, &node.children)?)),
        Kind::FixedArray => Ok(Expr::FixedArray(lower_expressions(
            context,
            &node.children,
        )?)),
        Kind::ListCons => lower_list_cons(context, node),
        Kind::ListComprehension => lower_comprehension(context, node),
        Kind::Map => Ok(Expr::Map(lower_fields(context, &node.children)?)),
        Kind::PatternConstructor => Ok(Expr::RecordConstruct {
            name: node.text.clone().unwrap_or_default(),
            fields: lower_fields(context, &node.children)?,
        }),
        Kind::BinaryLayout => lower_binary_layout(context, node),
        Kind::Unary => lower_unary(context, node),
        Kind::Binary => lower_binary(context, node),
        Kind::Cast => {
            require_children(context, node, 2)?;
            Ok(Expr::Cast {
                expr: Box::new(lower_expression(context, &node.children[0])?),
                target_type: context.type_expression(&node.children[1]),
            })
        }
        Kind::Call => lower_call(context, node),
        Kind::Index => {
            require_children(context, node, 2)?;
            Ok(Expr::Index(
                Box::new(lower_expression(context, &node.children[0])?),
                Box::new(lower_expression(context, &node.children[1])?),
            ))
        }
        Kind::IndexAssign => lower_index_assignment(context, node),
        Kind::FieldAccess => {
            require_children(context, node, 1)?;
            let field = node.text.clone().unwrap_or_default();
            Ok(Expr::FieldAccess {
                value: Box::new(lower_expression(context, &node.children[0])?),
                field,
            })
        }
        Kind::RecordAccess => {
            require_children(context, node, 1)?;
            let (name, field) = node
                .text
                .as_deref()
                .and_then(|text| text.split_once('.'))
                .ok_or_else(|| context.error(node, "record access is missing its identity"))?;
            Ok(Expr::RecordAccess {
                value: Box::new(lower_expression(context, &node.children[0])?),
                name: name.to_string(),
                field: field.to_string(),
            })
        }
        Kind::RecordUpdate => lower_record_update(context, node),
        Kind::Sequence => Ok(scope_sequence(lower_expressions(context, &node.children)?)),
        Kind::Quote => Ok(Expr::Quote(Box::new(only_child(context, node)?))),
        Kind::Unquote => Ok(Expr::Unquote(Box::new(only_child(context, node)?))),
        Kind::Let => lower_let(context, node),
        Kind::Case => lower_case(context, node),
        Kind::Try => lower_try(context, node),
        Kind::If => lower_if(context, node),
        Kind::Lambda => lower_lambda(context, node),
        Kind::MacroCall => Ok(Expr::MacroCall {
            name: node.text.clone().unwrap_or_default(),
            args: lower_expressions(context, &node.children)?,
        }),
        Kind::RawMacro => raw_macros::lower(context, node),
        Kind::ConstructorChain => {
            require_children(context, node, 2)?;
            Ok(Expr::ConstructorChain {
                base: Box::new(lower_expression(context, &node.children[0])?),
                record: Box::new(lower_expression(context, &node.children[1])?),
            })
        }
        _ => Err(context.error(
            node,
            format!("generated node {:?} is not an expression", node.kind),
        )),
    }
}

fn scope_sequence(mut expressions: Vec<Expr>) -> Expr {
    if expressions.len() == 1 {
        return expressions.remove(0);
    }
    let first = expressions.remove(0);
    match first {
        Expr::Let {
            bindings,
            else_clauses,
            body: Some(body),
        } => {
            let mut scoped = Vec::with_capacity(expressions.len() + 1);
            scoped.push(*body);
            scoped.extend(expressions);
            Expr::Let {
                bindings,
                else_clauses,
                body: Some(Box::new(scope_sequence(scoped))),
            }
        }
        first => {
            let mut sequence = Vec::with_capacity(expressions.len() + 1);
            sequence.push(first);
            match scope_sequence(expressions) {
                Expr::Sequence(rest) => sequence.extend(rest),
                scoped => sequence.push(scoped),
            }
            Expr::Sequence(sequence)
        }
    }
}

fn lower_call(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let callee = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "call is missing its callee"))?;
    if callee.kind == LalrpopSyntaxNodeKind::FieldAccess
        && callee
            .text
            .as_deref()
            .and_then(|field| field.chars().next())
            .is_some_and(|character| character.is_ascii_uppercase())
    {
        return Err(context.error(callee, "expected lower-case remote function name"));
    }
    let metadata = node.text.as_deref().unwrap_or_default();
    let type_arg_count = metadata_count(metadata, "generic").unwrap_or(0);
    if node.children.len() < type_arg_count + 1 {
        return Err(context.error(node, "generic call has an invalid type-argument count"));
    }
    let type_args = node.children[1..1 + type_arg_count]
        .iter()
        .map(|node| context.type_expression(node))
        .collect();
    let mut args = Vec::new();
    let mut arg_names = Vec::new();
    let mut saw_named = false;
    for argument in &node.children[1 + type_arg_count..] {
        if argument.kind == LalrpopSyntaxNodeKind::IndexAssign
            && argument
                .children
                .first()
                .is_some_and(|left| left.kind == LalrpopSyntaxNodeKind::Binding)
        {
            require_children(context, argument, 2)?;
            saw_named = true;
            arg_names.push(argument.children[0].text.clone());
            args.push(lower_expression(context, &argument.children[1])?);
        } else {
            if saw_named {
                return Err(context.error(
                    argument,
                    "positional arguments must come before named arguments",
                ));
            }
            arg_names.push(None);
            args.push(lower_expression(context, argument)?);
        }
    }
    let explicit_remote = metadata
        .split(';')
        .find_map(|part| part.strip_prefix("remote:"))
        .map(str::to_string);
    let grouped_callee = callee.kind == LalrpopSyntaxNodeKind::Group;
    let lowered_callee = lower_expression(context, callee)?;
    let (callee, remote, is_fun_value) = if let Some(remote) = explicit_remote {
        let function = match lowered_callee {
            Expr::Var(function) | Expr::Atom(function) => function,
            _ => return Err(context.error(node, "remote call has an invalid function name")),
        };
        (Expr::Atom(function), Some(remote), false)
    } else if let Expr::FieldAccess { value, field } = lowered_callee {
        let remote_path = expression_path(&value).filter(|path| {
            path.rsplit('.')
                .next()
                .and_then(|segment| segment.chars().next())
                .is_some_and(char::is_uppercase)
        });
        if let Some(remote) = remote_path {
            (Expr::Atom(field), Some(remote), false)
        } else {
            (Expr::FieldAccess { value, field }, None, false)
        }
    } else {
        let is_fun_value = grouped_callee || !matches!(lowered_callee, Expr::Var(_));
        (lowered_callee, None, is_fun_value)
    };
    Ok(Expr::Call {
        callee: Box::new(callee),
        type_args,
        args,
        arg_names,
        remote,
        is_fun_value,
    })
}

fn expression_path(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Var(name) => Some(name.clone()),
        Expr::FieldAccess { value, field } => {
            expression_path(value).map(|path| format!("{path}.{field}"))
        }
        Expr::Index(value, index) => {
            let value = expression_path(value)?;
            let index = expression_path(index)?;
            Some(format!("{value}[{index}]"))
        }
        _ => None,
    }
}

fn lower_atom_literal(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let text = node_text(context, node);
    let value = if text.starts_with('"') {
        unquote(text)
    } else if text.starts_with('\'') && text.ends_with('\'') {
        Some(text[1..text.len() - 1].to_string())
    } else {
        Some(text.to_string())
    }
    .filter(|value| !value.is_empty())
    .ok_or_else(|| context.error(node, "expected non-empty atom string literal"))?;
    Ok(Expr::AtomLiteral(value))
}

fn lower_index_assignment(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    require_children(context, node, 2)?;
    let index = &node.children[0];
    require_children(context, index, 2)?;
    Ok(Expr::IndexAssign {
        collection: Box::new(lower_expression(context, &index.children[0])?),
        index: Box::new(lower_expression(context, &index.children[1])?),
        value: Box::new(lower_expression(context, &node.children[1])?),
    })
}

fn lower_record_update(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let value = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "record update is missing its value"))?;
    Ok(Expr::RecordUpdate {
        value: Box::new(lower_expression(context, value)?),
        name: node.text.clone().unwrap_or_default(),
        fields: lower_fields(context, &node.children[1..])?,
    })
}

fn lower_list_cons(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    require_children(context, node, 2)?;
    Ok(Expr::ListCons(
        Box::new(lower_expression(context, &node.children[0])?),
        Box::new(lower_expression(context, &node.children[1])?),
    ))
}

fn lower_comprehension(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let value = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "list comprehension is missing its value"))?;
    let mut generators = Vec::new();
    let mut guards = Vec::new();
    let mut saw_guard = false;
    for qualifier in &node.children[1..] {
        if qualifier.kind == LalrpopSyntaxNodeKind::Generator {
            if saw_guard {
                return Err(context.error(
                    qualifier,
                    "list comprehension generators must precede filter expressions",
                ));
            }
            require_children(context, qualifier, 2)?;
            generators.push(ListComprehensionGenerator {
                pattern: lower_pattern(context, &qualifier.children[0])?,
                source: Box::new(lower_expression(context, &qualifier.children[1])?),
            });
        } else {
            saw_guard = true;
            guards.push(lower_expression(context, qualifier)?);
        }
    }
    Ok(Expr::ListComprehension {
        expr: Box::new(lower_expression(context, value)?),
        generators,
        guards,
    })
}

fn lower_let(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    if node.text.as_deref() != Some("refutable_group") {
        require_children(context, node, 3)?;
        let first = LetBinding {
            pattern: lower_pattern(context, &node.children[0])?,
            value: lower_expression(context, &node.children[1])?,
        };
        let body = lower_expression(context, &node.children[2])?;
        if let Expr::Let {
            mut bindings,
            else_clauses,
            body,
        } = body
        {
            if else_clauses.is_empty() {
                bindings.insert(0, first);
                return Ok(Expr::Let {
                    bindings,
                    else_clauses,
                    body,
                });
            }
            return Ok(Expr::Let {
                bindings: vec![first],
                else_clauses: Vec::new(),
                body: Some(Box::new(Expr::Let {
                    bindings,
                    else_clauses,
                    body,
                })),
            });
        }
        return Ok(Expr::Let {
            bindings: vec![first],
            else_clauses: Vec::new(),
            body: Some(Box::new(body)),
        });
    }
    let body_index = node
        .children
        .iter()
        .rposition(|child| child.kind != LalrpopSyntaxNodeKind::Clause)
        .ok_or_else(|| context.error(node, "refutable let is missing its body"))?;
    let mut bindings = Vec::new();
    let mut clauses = Vec::new();
    for child in &node.children[..body_index] {
        if child.kind == LalrpopSyntaxNodeKind::Let {
            require_children(context, child, 2)?;
            bindings.push(LetBinding {
                pattern: lower_pattern(context, &child.children[0])?,
                value: lower_expression(context, &child.children[1])?,
            });
        } else if child.kind == LalrpopSyntaxNodeKind::Clause {
            clauses.push(lower_case_clause(context, child)?);
        }
    }
    Ok(Expr::Let {
        bindings,
        else_clauses: clauses,
        body: Some(Box::new(lower_expression(
            context,
            &node.children[body_index],
        )?)),
    })
}

fn lower_case(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let scrutinee = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "case expression is missing its scrutinee"))?;
    Ok(Expr::Case {
        scrutinee: Box::new(lower_expression(context, scrutinee)?),
        clauses: node.children[1..]
            .iter()
            .map(|clause| lower_case_clause(context, clause))
            .collect::<LalrpopLoweringResult<Vec<_>>>()?,
    })
}

fn lower_case_clause(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<CaseClause> {
    let (pattern_count, has_guard) = clause_metadata(node)?;
    if pattern_count != 1 {
        return Err(context.error(node, "case clause must contain exactly one pattern"));
    }
    let body_index = pattern_count + usize::from(has_guard);
    let body = node
        .children
        .get(body_index)
        .ok_or_else(|| context.error(node, "case clause is missing its body"))?;
    Ok(CaseClause {
        pattern: lower_pattern(context, &node.children[0])?,
        guard: has_guard
            .then(|| lower_expression(context, &node.children[pattern_count]).map(Box::new))
            .transpose()?,
        body: lower_expression(context, body)?,
    })
}

fn lower_try(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let metadata = node.text.as_deref().unwrap_or_default();
    let of_count = metadata_count(metadata, "of").unwrap_or(0);
    let catch_count = metadata_count(metadata, "catch").unwrap_or(0);
    let has_after = metadata
        .split(';')
        .find_map(|part| part.strip_prefix("after:"))
        == Some("true");
    let body = node
        .children
        .first()
        .ok_or_else(|| context.error(node, "try expression is missing its body"))?;
    let clauses_end = 1 + of_count + catch_count;
    if node.children.len() < clauses_end + usize::from(has_after) * 2 {
        return Err(context.error(node, "try expression metadata does not match its children"));
    }
    let of_clauses = node.children[1..1 + of_count]
        .iter()
        .map(|clause| lower_case_clause(context, clause))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let catch_clauses = node.children[1 + of_count..clauses_end]
        .iter()
        .map(|clause| lower_case_clause(context, clause))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let after_clause = has_after
        .then(|| {
            Ok(TryAfterClause {
                trigger: Box::new(lower_expression(context, &node.children[clauses_end])?),
                body: Box::new(lower_expression(context, &node.children[clauses_end + 1])?),
            })
        })
        .transpose()?;
    Ok(Expr::Try {
        body: Box::new(lower_expression(context, body)?),
        of_clauses,
        catch_clauses,
        after_clause,
    })
}

fn lower_if(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    Ok(Expr::If {
        clauses: node
            .children
            .iter()
            .map(|clause| {
                require_children(context, clause, 2)?;
                let mut condition = lower_expression(context, &clause.children[0])?;
                if matches!(&condition, Expr::Var(name) if name == "_") {
                    condition = Expr::Var("true".to_string());
                }
                Ok(IfClause {
                    condition,
                    body: lower_expression(context, &clause.children[1])?,
                })
            })
            .collect::<LalrpopLoweringResult<Vec<_>>>()?,
    })
}

fn lower_lambda(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let body_index = node
        .children
        .len()
        .checked_sub(1)
        .ok_or_else(|| context.error(node, "lambda expression is missing its body"))?;
    Ok(Expr::Fun {
        clauses: vec![FunctionClause {
            patterns: node.children[..body_index]
                .iter()
                .map(|pattern| lower_pattern(context, pattern))
                .collect::<LalrpopLoweringResult<Vec<_>>>()?,
            body: lower_expression(context, &node.children[body_index])?,
            span: node.span,
            guard: None,
        }],
    })
}

fn lower_unary(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let op = match node.text.as_deref() {
        Some("-") => UnaryOp::Neg,
        Some("not") => UnaryOp::Not,
        Some("!") => UnaryOp::Bang,
        _ => return Err(context.error(node, "unknown unary operator")),
    };
    Ok(Expr::UnaryOp {
        op,
        expr: Box::new(only_child(context, node)?),
    })
}

fn lower_binary(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    require_children(context, node, 2)?;
    let op = match node.text.as_deref() {
        Some("+") => BinaryOp::Add,
        Some("-") => BinaryOp::Sub,
        Some("*") => BinaryOp::Mul,
        Some("/") => BinaryOp::Div,
        Some("==") | Some("===") => BinaryOp::EqEq,
        Some("!=") | Some("!==") => BinaryOp::NotEq,
        Some("<") => BinaryOp::Lt,
        Some(">") => BinaryOp::Gt,
        Some("<=") => BinaryOp::LtEq,
        Some(">=") => BinaryOp::GtEq,
        Some("div") => BinaryOp::DivRem,
        Some("rem") => BinaryOp::Rem,
        Some("..") => BinaryOp::Range,
        Some("in") => BinaryOp::In,
        Some("and") => BinaryOp::And,
        Some("or") => BinaryOp::Or,
        Some("|>") => BinaryOp::PipeForward,
        _ => return Err(context.error(node, "unknown binary operator")),
    };
    Ok(Expr::BinaryOp {
        op,
        left: Box::new(lower_expression(context, &node.children[0])?),
        right: Box::new(lower_expression(context, &node.children[1])?),
    })
}

fn lower_binary_layout(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let endian = node
        .text
        .as_deref()
        .and_then(|text| text.split_once('['))
        .and_then(|(_, tail)| tail.strip_suffix(']'))
        .unwrap_or_default()
        .to_string();
    let fields = node
        .children
        .iter()
        .map(|field| {
            let descriptor = field.children.first().ok_or_else(|| {
                context.error(field, "binary layout field is missing its descriptor")
            })?;
            Ok(BinaryLayoutField {
                name: field.text.clone().unwrap_or_default(),
                descriptor: TypeExpr {
                    text: context.text(descriptor.span).to_string(),
                    span: descriptor.span,
                },
            })
        })
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    binary_layout::validate(context, node, &endian, &fields)?;
    Ok(Expr::BinaryLayout { endian, fields })
}

fn lower_fields(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<Vec<MapExprField>> {
    nodes
        .iter()
        .map(|field| {
            let value = field
                .children
                .first()
                .ok_or_else(|| context.error(field, "map field is missing its value"))?;
            let raw_key = field.text.clone().unwrap_or_default();
            Ok(MapExprField {
                key: unquote(&raw_key).unwrap_or(raw_key),
                value: Box::new(lower_expression(context, value)?),
                required: true,
            })
        })
        .collect()
}

fn lower_expressions(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<Vec<Expr>> {
    nodes
        .iter()
        .map(|node| lower_expression(context, node))
        .collect()
}

fn only_child(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    require_children(context, node, 1)?;
    lower_expression(context, &node.children[0])
}

fn require_children(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    expected: usize,
) -> LalrpopLoweringResult<()> {
    if node.children.len() == expected {
        Ok(())
    } else {
        Err(context.error(
            node,
            format!(
                "generated {:?} node has {} children; expected {expected}",
                node.kind,
                node.children.len()
            ),
        ))
    }
}

fn clause_metadata(node: &LalrpopSyntaxNode) -> LalrpopLoweringResult<(usize, bool)> {
    let metadata = node.text.as_deref().unwrap_or_default();
    let patterns =
        metadata_count(metadata, "patterns").ok_or_else(|| super::LalrpopLoweringError {
            message: "clause is missing pattern-count metadata".to_string(),
            span: node.span,
        })?;
    let guard = metadata
        .split(';')
        .find_map(|part| part.strip_prefix("guard:"))
        == Some("true");
    Ok((patterns, guard))
}

fn metadata_count(metadata: &str, key: &str) -> Option<usize> {
    metadata
        .split(';')
        .find_map(|part| part.strip_prefix(&format!("{key}:")))
        .and_then(|value| value.parse().ok())
}

fn node_text<'a>(context: &'a LalrpopLoweringContext<'_>, node: &'a LalrpopSyntaxNode) -> &'a str {
    node.text
        .as_deref()
        .unwrap_or_else(|| context.text(node.span))
}

fn parse_int(text: &str) -> Option<i64> {
    if let Some(value) = text.strip_prefix("0b") {
        i64::from_str_radix(value, 2).ok()
    } else if let Some(value) = text.strip_prefix("0x") {
        i64::from_str_radix(value, 16).ok()
    } else if let Some(value) = text.strip_prefix("0o") {
        i64::from_str_radix(value, 8).ok()
    } else {
        text.parse().ok()
    }
}
