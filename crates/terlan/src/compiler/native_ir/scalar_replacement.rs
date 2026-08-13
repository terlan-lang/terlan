//! Scalar replacement for fixed managed aggregates.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreLetBinding, CorePattern, CoreRecordExprField, CoreStringPatternSegment,
};

use super::constructors::NativeConstructorLayouts;

/// Replaces nonescaping fixed aggregates with ordinary local bindings.
pub(super) fn scalar_replace_fixed_aggregates(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
) -> CoreExpr {
    let mut ordinal = 0_u64;
    replace_nested(expr, layouts, &mut ordinal)
}

/// Rewrites nested regions before considering constructor bindings in this one.
fn replace_nested(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
    ordinal: &mut u64,
) -> CoreExpr {
    match expr {
        CoreExpr::Let { bindings, body } => {
            let mut bindings = bindings
                .iter()
                .map(|binding| CoreLetBinding {
                    pattern: binding.pattern.clone(),
                    value: replace_nested(&binding.value, layouts, ordinal),
                })
                .collect::<Vec<_>>();
            let body = replace_nested(body, layouts, ordinal);
            if let CoreExpr::Let {
                bindings: nested_bindings,
                body: nested_body,
            } = body
            {
                bindings.extend(nested_bindings);
                replace_let(bindings, *nested_body, layouts, ordinal)
            } else {
                replace_let(bindings, body, layouts, ordinal)
            }
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => CoreExpr::ConstructorCall {
            constructor: constructor.clone(),
            constructor_identity: constructor_identity.clone(),
            args: args
                .iter()
                .map(|arg| replace_nested(arg, layouts, ordinal))
                .collect(),
        },
        CoreExpr::RecordConstruct { name, fields } => CoreExpr::RecordConstruct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = replace_nested(&field.value, layouts, ordinal);
                    field
                })
                .collect(),
        },
        CoreExpr::RecordUpdate { base, name, fields } => CoreExpr::RecordUpdate {
            base: Box::new(replace_nested(base, layouts, ordinal)),
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = replace_nested(&field.value, layouts, ordinal);
                    field
                })
                .collect(),
        },
        CoreExpr::Tuple(items) => CoreExpr::Tuple(
            items
                .iter()
                .map(|item| replace_nested(item, layouts, ordinal))
                .collect(),
        ),
        CoreExpr::FixedArray(items) => CoreExpr::FixedArray(
            items
                .iter()
                .map(|item| replace_nested(item, layouts, ordinal))
                .collect(),
        ),
        CoreExpr::Index { base, index } => {
            let base = replace_nested(base, layouts, ordinal);
            let index = replace_nested(index, layouts, ordinal);
            if direct_fixed_index(&base, &index).is_some() {
                let source = format!("$native_sroa_index_source_{}", *ordinal);
                replace_let(
                    vec![CoreLetBinding {
                        pattern: CorePattern::Var(source.clone()),
                        value: base,
                    }],
                    CoreExpr::Index {
                        base: Box::new(CoreExpr::Var(source)),
                        index: Box::new(index),
                    },
                    layouts,
                    ordinal,
                )
            } else {
                CoreExpr::Index {
                    base: Box::new(base),
                    index: Box::new(index),
                }
            }
        }
        CoreExpr::Call { function, args }
            if function == "IndexGet.get_at" && matches!(args.as_slice(), [_, _]) =>
        {
            let base = replace_nested(&args[0], layouts, ordinal);
            let index = replace_nested(&args[1], layouts, ordinal);
            if direct_fixed_index(&base, &index).is_some() {
                let source = format!("$native_sroa_index_source_{}", *ordinal);
                replace_let(
                    vec![CoreLetBinding {
                        pattern: CorePattern::Var(source.clone()),
                        value: base,
                    }],
                    CoreExpr::Call {
                        function: function.clone(),
                        args: vec![CoreExpr::Var(source), index],
                    },
                    layouts,
                    ordinal,
                )
            } else {
                CoreExpr::Call {
                    function: function.clone(),
                    args: vec![base, index],
                }
            }
        }
        CoreExpr::Call { function, args } => CoreExpr::Call {
            function: function.clone(),
            args: args
                .iter()
                .map(|arg| replace_nested(arg, layouts, ordinal))
                .collect(),
        },
        CoreExpr::Intrinsic(call) => {
            let mut call = call.clone();
            call.args = call
                .args
                .iter()
                .map(|arg| replace_nested(arg, layouts, ordinal))
                .collect();
            CoreExpr::Intrinsic(call)
        }
        CoreExpr::FieldAccess { base, field } => CoreExpr::FieldAccess {
            base: Box::new(replace_nested(base, layouts, ordinal)),
            field: field.clone(),
        },
        CoreExpr::UnaryOp { operator, operand } => CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(replace_nested(operand, layouts, ordinal)),
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(replace_nested(left, layouts, ordinal)),
            right: Box::new(replace_nested(right, layouts, ordinal)),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = replace_nested(&clause.condition, layouts, ordinal);
                    clause.body = replace_nested(&clause.body, layouts, ordinal);
                    clause
                })
                .collect(),
        },
        CoreExpr::Cast { expr, target_type } => CoreExpr::Cast {
            expr: Box::new(replace_nested(expr, layouts, ordinal)),
            target_type: target_type.clone(),
        },
        _ => expr.clone(),
    }
}

/// Rewrites each eligible binding while retaining source evaluation order.
fn replace_let(
    mut bindings: Vec<CoreLetBinding>,
    mut body: CoreExpr,
    layouts: &NativeConstructorLayouts,
    ordinal: &mut u64,
) -> CoreExpr {
    let mut index = 0;
    while index < bindings.len() {
        if let CorePattern::Var(name) = &bindings[index].pattern {
            if let Some((consumer, leaves)) =
                local_fixed_destructuring(&bindings, &body, index, name, &bindings[index].value)
            {
                replace_local_fixed_destructuring(&mut bindings, index, consumer, leaves, *ordinal);
                *ordinal = ordinal.saturating_add(1);
                continue;
            }
        }
        if let Some(replacement) = scalar_pattern_bindings(&bindings[index], *ordinal) {
            *ordinal = ordinal.saturating_add(1);
            bindings.splice(index..=index, replacement);
            continue;
        }
        let CorePattern::Var(name) = &bindings[index].pattern else {
            index += 1;
            continue;
        };
        let Some(layout) = projection_layout(&bindings[index].value, layouts) else {
            index += 1;
            continue;
        };
        let alias_names = layout
            .arguments
            .iter()
            .enumerate()
            .map(|(field_index, _)| format!("$native_sroa_{}_{}", *ordinal, field_index))
            .collect::<Vec<_>>();
        let named_aliases = layout
            .field_names
            .iter()
            .zip(&alias_names)
            .filter_map(|(field, alias)| field.as_ref().map(|field| (field.clone(), alias.clone())))
            .collect::<HashMap<_, _>>();
        let indexed_aliases = layout.indexed.then_some(alias_names.as_slice());
        if named_aliases.is_empty() && indexed_aliases.is_none() {
            index += 1;
            continue;
        }
        let mut rewritten_tail = bindings[index + 1..].to_vec();
        let mut outcome = ProjectionOutcome::default();
        substitute_tail(
            name,
            &named_aliases,
            indexed_aliases,
            &mut rewritten_tail,
            &mut body,
            &mut outcome,
        );
        if outcome.direct_use || outcome.projections == 0 {
            index += 1;
            continue;
        }
        let replacement = layout
            .arguments
            .into_iter()
            .zip(alias_names)
            .map(|(value, alias)| CoreLetBinding {
                pattern: CorePattern::Var(alias),
                value,
            })
            .collect::<Vec<_>>();
        *ordinal = ordinal.saturating_add(1);
        bindings.splice(index.., replacement.into_iter().chain(rewritten_tail));
    }
    CoreExpr::Let {
        bindings,
        body: Box::new(body),
    }
}

/// Finds one later irrefutable destructuring that is the fixed local's only use.
fn local_fixed_destructuring(
    bindings: &[CoreLetBinding],
    body: &CoreExpr,
    producer: usize,
    target: &str,
    value: &CoreExpr,
) -> Option<(usize, Vec<(CorePattern, CoreExpr)>)> {
    if !matches!(value, CoreExpr::Tuple(_) | CoreExpr::ConstructorCall { .. }) {
        return None;
    }
    for (consumer, binding) in bindings.iter().enumerate().skip(producer + 1) {
        if matches!(&binding.value, CoreExpr::Var(name) if name == target) {
            let mut leaves = Vec::new();
            flatten_fixed_pattern(&binding.pattern, value, &mut leaves)?;
            if local_observed_after_binding(bindings, body, consumer, target) {
                return None;
            }
            return Some((consumer, leaves));
        }
        if expr_observes_local(&binding.value, target)
            || pattern_binds_local(&binding.pattern, target)
        {
            return None;
        }
    }
    None
}

/// Reports whether an outer local remains observable after one binding executes.
fn local_observed_after_binding(
    bindings: &[CoreLetBinding],
    body: &CoreExpr,
    binding_index: usize,
    target: &str,
) -> bool {
    let mut active = !pattern_binds_local(&bindings[binding_index].pattern, target);
    for binding in &bindings[binding_index + 1..] {
        if active && expr_observes_local(&binding.value, target) {
            return true;
        }
        if active && pattern_binds_local(&binding.pattern, target) {
            active = false;
        }
    }
    active && expr_observes_local(body, target)
}

/// Splits one aggregate producer and its later consumer into scalar bindings.
fn replace_local_fixed_destructuring(
    bindings: &mut Vec<CoreLetBinding>,
    producer: usize,
    consumer: usize,
    leaves: Vec<(CorePattern, CoreExpr)>,
    ordinal: u64,
) {
    let aliases = leaves
        .iter()
        .enumerate()
        .map(|(index, _)| format!("$native_sroa_local_{ordinal}_{index}"))
        .collect::<Vec<_>>();
    let consumer_bindings = leaves
        .iter()
        .zip(&aliases)
        .filter_map(|((pattern, _), alias)| match pattern {
            CorePattern::Var(name) => Some(CoreLetBinding {
                pattern: CorePattern::Var(name.clone()),
                value: CoreExpr::Var(alias.clone()),
            }),
            CorePattern::Wildcard => None,
            _ => unreachable!("fixed local destructuring emits only scalar leaves"),
        })
        .collect::<Vec<_>>();
    let producer_bindings = leaves
        .into_iter()
        .zip(aliases)
        .map(|((_, value), alias)| CoreLetBinding {
            pattern: CorePattern::Var(alias),
            value,
        })
        .collect::<Vec<_>>();
    bindings.splice(consumer..=consumer, consumer_bindings);
    bindings.splice(producer..=producer, producer_bindings);
}

/// Fixed aggregate arguments and the projections its representation supports.
struct ProjectionLayout {
    /// Source expressions evaluated once in representation order.
    arguments: Vec<CoreExpr>,
    /// Optional source-level field name for each representation slot.
    field_names: Vec<Option<String>>,
    /// Whether constant positional indexing is valid for this aggregate.
    indexed: bool,
}

/// Resolves source arguments and projections for one fixed aggregate value.
fn projection_layout(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
) -> Option<ProjectionLayout> {
    match expr {
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => {
            let identity = constructor_identity.as_deref().unwrap_or(constructor);
            let layout = layouts.get(&(identity.to_owned(), args.len()))?;
            let fields = layout
                .descriptor
                .fields()
                .iter()
                .map(|field| field.name().map(str::to_owned))
                .collect::<Vec<_>>();
            (fields.len() == args.len()).then(|| ProjectionLayout {
                arguments: args.clone(),
                field_names: fields,
                indexed: false,
            })
        }
        CoreExpr::Tuple(items) | CoreExpr::FixedArray(items) => Some(ProjectionLayout {
            arguments: items.clone(),
            field_names: vec![None; items.len()],
            indexed: true,
        }),
        CoreExpr::RecordConstruct { fields, .. } => Some(ProjectionLayout {
            arguments: fields.iter().map(|field| field.value.clone()).collect(),
            field_names: fields.iter().map(|field| Some(field.key.clone())).collect(),
            indexed: false,
        }),
        CoreExpr::Cast { expr, .. } => projection_layout(expr, layouts),
        _ => None,
    }
}

/// Resolves one in-bounds compile-time index into a fixed aggregate slot.
fn direct_fixed_index(base: &CoreExpr, index: &CoreExpr) -> Option<usize> {
    let CoreExpr::Int(index) = index else {
        return None;
    };
    let index = usize::try_from(*index).ok()?;
    let length = match base {
        CoreExpr::Tuple(items) | CoreExpr::FixedArray(items) => items.len(),
        _ => return None,
    };
    (index < length).then_some(index)
}

mod substitution;

use substitution::substitute_tail;

/// Replaces one statically irrefutable fixed-pattern binding with scalar locals.
fn scalar_pattern_bindings(binding: &CoreLetBinding, ordinal: u64) -> Option<Vec<CoreLetBinding>> {
    if !matches!(
        binding.pattern,
        CorePattern::Tuple(_) | CorePattern::Constructor { .. }
    ) {
        return None;
    }
    let mut leaves = Vec::new();
    flatten_fixed_pattern(&binding.pattern, &binding.value, &mut leaves)?;
    Some(
        leaves
            .into_iter()
            .enumerate()
            .map(|(index, (pattern, value))| CoreLetBinding {
                pattern: match pattern {
                    CorePattern::Var(name) => CorePattern::Var(name),
                    CorePattern::Wildcard => {
                        CorePattern::Var(format!("$native_sroa_pattern_{ordinal}_{index}"))
                    }
                    _ => unreachable!("fixed-pattern flattening emits only scalar leaves"),
                },
                value,
            })
            .collect(),
    )
}

/// Pairs variable and wildcard leaves from one known fixed aggregate value.
fn flatten_fixed_pattern(
    pattern: &CorePattern,
    value: &CoreExpr,
    leaves: &mut Vec<(CorePattern, CoreExpr)>,
) -> Option<()> {
    match (pattern, value) {
        (CorePattern::Tuple(patterns), CoreExpr::Tuple(values))
            if patterns.len() == values.len() =>
        {
            for (pattern, value) in patterns.iter().zip(values) {
                flatten_fixed_pattern(pattern, value, leaves)?;
            }
            Some(())
        }
        (
            CorePattern::Constructor {
                name,
                constructor_identity: pattern_identity,
                args: patterns,
            },
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity: value_identity,
                args: values,
            },
        ) if patterns.len() == values.len()
            && fixed_identity(name, pattern_identity.as_deref())
                == fixed_identity(constructor, value_identity.as_deref()) =>
        {
            for (pattern, value) in patterns.iter().zip(values) {
                flatten_fixed_pattern(pattern, value, leaves)?;
            }
            Some(())
        }
        (pattern @ (CorePattern::Var(_) | CorePattern::Wildcard), value)
            if !matches!(value, CoreExpr::Tuple(_)) =>
        {
            leaves.push((pattern.clone(), value.clone()));
            Some(())
        }
        _ => None,
    }
}

/// Selects the resolved identity used to prove two fixed constructors equal.
fn fixed_identity<'a>(name: &'a str, resolved: Option<&'a str>) -> &'a str {
    resolved.unwrap_or(name)
}

/// Reports whether evaluating one supported expression observes an outer local.
fn expr_observes_local(expr: &CoreExpr, target: &str) -> bool {
    match expr {
        CoreExpr::Var(name) => name == target,
        CoreExpr::Int(_) | CoreExpr::Float(_) | CoreExpr::Binary(_) | CoreExpr::Atom(_) => false,
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().any(|item| expr_observes_local(item, target))
        }
        CoreExpr::ListCons { head, tail } => {
            expr_observes_local(head, target) || expr_observes_local(tail, target)
        }
        CoreExpr::Index { base, index } => {
            expr_observes_local(base, target) || expr_observes_local(index, target)
        }
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. } => args
            .iter()
            .any(|argument| expr_observes_local(argument, target)),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .any(|field| expr_observes_local(&field.value, target)),
        CoreExpr::Intrinsic(call) => call
            .args
            .iter()
            .any(|argument| expr_observes_local(argument, target)),
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_observes_local(base, target)
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            expr_observes_local(operand, target)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_observes_local(left, target) || expr_observes_local(right, target)
        }
        CoreExpr::Let { bindings, body } => {
            let mut active = true;
            for binding in bindings {
                if active && expr_observes_local(&binding.value, target) {
                    return true;
                }
                if active && pattern_binds_local(&binding.pattern, target) {
                    active = false;
                }
            }
            active && expr_observes_local(body, target)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expr_observes_local(&clause.condition, target)
                || expr_observes_local(&clause.body, target)
        }),
        _ => true,
    }
}

/// Reports whether one Core pattern introduces a local with the target name.
fn pattern_binds_local(pattern: &CorePattern, target: &str) -> bool {
    match pattern {
        CorePattern::Var(name) => name == target,
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => patterns
            .iter()
            .any(|pattern| pattern_binds_local(pattern, target)),
        CorePattern::Alias { alias, pattern } => {
            alias == target || pattern_binds_local(pattern, target)
        }
        CorePattern::ListCons { head, tail } => {
            pattern_binds_local(head, target) || pattern_binds_local(tail, target)
        }
        CorePattern::Map(fields) => fields
            .iter()
            .any(|field| pattern_binds_local(&field.value, target)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .any(|field| pattern_binds_local(&field.value, target)),
        CorePattern::Constructor { args, .. } => args
            .iter()
            .any(|pattern| pattern_binds_local(pattern, target)),
        CorePattern::StringPattern(segments) => segments.iter().any(|segment| {
            matches!(segment, CoreStringPatternSegment::Capture(capture) if capture.name == target)
        }),
        CorePattern::BinaryLayout { fields, .. } => {
            fields.iter().any(|field| field.name == target)
        }
        CorePattern::Wildcard
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::Atom(_) => false,
    }
}

/// Counts replacements and records any use requiring the aggregate identity.
#[derive(Default)]
struct ProjectionOutcome {
    /// Whether the aggregate identity is observed outside a known projection.
    direct_use: bool,
    /// Number of direct fixed projections replaced in this lexical tail.
    projections: usize,
}
