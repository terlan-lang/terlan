use super::*;

/// Substitutes fixed projections until a later binding shadows the local name.
pub(super) fn substitute_tail(
    target: &str,
    named_aliases: &HashMap<String, String>,
    indexed_aliases: Option<&[String]>,
    bindings: &mut [CoreLetBinding],
    body: &mut CoreExpr,
    outcome: &mut ProjectionOutcome,
) {
    let mut active = true;
    for binding in bindings {
        if active {
            binding.value = substitute_expr(
                &binding.value,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            );
        }
        if matches!(&binding.pattern, CorePattern::Var(name) if name == target) {
            active = false;
        } else if !matches!(binding.pattern, CorePattern::Var(_)) {
            outcome.direct_use = true;
        }
    }
    if active {
        *body = substitute_expr(body, target, named_aliases, indexed_aliases, outcome);
    }
}

/// Replaces one direct named or indexed projection and detects escaping uses.
fn substitute_expr(
    expr: &CoreExpr,
    target: &str,
    named_aliases: &HashMap<String, String>,
    indexed_aliases: Option<&[String]>,
    outcome: &mut ProjectionOutcome,
) -> CoreExpr {
    match expr {
        CoreExpr::Var(name) if name == target => {
            outcome.direct_use = true;
            expr.clone()
        }
        CoreExpr::FieldAccess { base, field } if matches!(base.as_ref(), CoreExpr::Var(name) if name == target) =>
        {
            let Some(alias) = named_aliases.get(field) else {
                outcome.direct_use = true;
                return expr.clone();
            };
            outcome.projections = outcome.projections.saturating_add(1);
            CoreExpr::Var(alias.clone())
        }
        CoreExpr::Index { base, index } if matches!(base.as_ref(), CoreExpr::Var(name) if name == target) =>
        {
            let Some(aliases) = indexed_aliases else {
                outcome.direct_use = true;
                return expr.clone();
            };
            let CoreExpr::Int(index) = index.as_ref() else {
                outcome.direct_use = true;
                return expr.clone();
            };
            let Some(alias) = usize::try_from(*index)
                .ok()
                .and_then(|index| aliases.get(index))
            else {
                outcome.direct_use = true;
                return expr.clone();
            };
            outcome.projections = outcome.projections.saturating_add(1);
            CoreExpr::Var(alias.clone())
        }
        CoreExpr::Call { function, args }
            if function == "IndexGet.get_at"
                && matches!(args.as_slice(), [CoreExpr::Var(name), _] if name == target) =>
        {
            let Some(aliases) = indexed_aliases else {
                outcome.direct_use = true;
                return expr.clone();
            };
            let [_, CoreExpr::Int(index)] = args.as_slice() else {
                outcome.direct_use = true;
                return expr.clone();
            };
            let Some(alias) = usize::try_from(*index)
                .ok()
                .and_then(|index| aliases.get(index))
            else {
                outcome.direct_use = true;
                return expr.clone();
            };
            outcome.projections = outcome.projections.saturating_add(1);
            CoreExpr::Var(alias.clone())
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => CoreExpr::ConstructorCall {
            constructor: constructor.clone(),
            constructor_identity: constructor_identity.clone(),
            args: substitute_args(args, target, named_aliases, indexed_aliases, outcome),
        },
        CoreExpr::RecordConstruct { name, fields } => CoreExpr::RecordConstruct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = substitute_expr(
                        &field.value,
                        target,
                        named_aliases,
                        indexed_aliases,
                        outcome,
                    );
                    field
                })
                .collect(),
        },
        CoreExpr::RecordUpdate { base, name, fields } if matches!(base.as_ref(), CoreExpr::Var(local) if local == target) =>
        {
            let mut fields = fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = substitute_expr(
                        &field.value,
                        target,
                        named_aliases,
                        indexed_aliases,
                        outcome,
                    );
                    field
                })
                .collect::<Vec<_>>();
            let updated = fields
                .iter()
                .map(|field| field.key.as_str())
                .collect::<std::collections::HashSet<_>>();
            let mut retained = named_aliases
                .iter()
                .filter(|(field, _)| !updated.contains(field.as_str()))
                .collect::<Vec<_>>();
            retained.sort_by_key(|(field, _)| field.as_str());
            fields.extend(
                retained
                    .into_iter()
                    .map(|(field, alias)| CoreRecordExprField {
                        key: field.clone(),
                        required: false,
                        value: CoreExpr::Var(alias.clone()),
                    }),
            );
            outcome.projections = outcome.projections.saturating_add(1);
            CoreExpr::RecordConstruct {
                name: name.clone(),
                fields,
            }
        }
        CoreExpr::RecordUpdate { base, name, fields } => CoreExpr::RecordUpdate {
            base: Box::new(substitute_expr(
                base,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = substitute_expr(
                        &field.value,
                        target,
                        named_aliases,
                        indexed_aliases,
                        outcome,
                    );
                    field
                })
                .collect(),
        },
        CoreExpr::Call { function, args } => CoreExpr::Call {
            function: function.clone(),
            args: substitute_args(args, target, named_aliases, indexed_aliases, outcome),
        },
        CoreExpr::Intrinsic(call) => {
            let mut call = call.clone();
            call.args =
                substitute_args(&call.args, target, named_aliases, indexed_aliases, outcome);
            CoreExpr::Intrinsic(call)
        }
        CoreExpr::FieldAccess { base, field } => CoreExpr::FieldAccess {
            base: Box::new(substitute_expr(
                base,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
            field: field.clone(),
        },
        CoreExpr::UnaryOp { operator, operand } => CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(substitute_expr(
                operand,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(substitute_expr(
                left,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
            right: Box::new(substitute_expr(
                right,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
        },
        CoreExpr::Let { bindings, body } => {
            let mut bindings = bindings.clone();
            let mut body = body.as_ref().clone();
            substitute_tail(
                target,
                named_aliases,
                indexed_aliases,
                &mut bindings,
                &mut body,
                outcome,
            );
            CoreExpr::Let {
                bindings,
                body: Box::new(body),
            }
        }
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = substitute_expr(
                        &clause.condition,
                        target,
                        named_aliases,
                        indexed_aliases,
                        outcome,
                    );
                    clause.body = substitute_expr(
                        &clause.body,
                        target,
                        named_aliases,
                        indexed_aliases,
                        outcome,
                    );
                    clause
                })
                .collect(),
        },
        CoreExpr::Cast {
            expr: cast,
            target_type,
        } => CoreExpr::Cast {
            expr: Box::new(substitute_expr(
                cast,
                target,
                named_aliases,
                indexed_aliases,
                outcome,
            )),
            target_type: target_type.clone(),
        },
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_) => expr.clone(),
        _ => {
            outcome.direct_use = true;
            expr.clone()
        }
    }
}

/// Substitutes constructor projections through one ordered argument vector.
fn substitute_args(
    args: &[CoreExpr],
    target: &str,
    named_aliases: &HashMap<String, String>,
    indexed_aliases: Option<&[String]>,
    outcome: &mut ProjectionOutcome,
) -> Vec<CoreExpr> {
    args.iter()
        .map(|arg| substitute_expr(arg, target, named_aliases, indexed_aliases, outcome))
        .collect()
}
