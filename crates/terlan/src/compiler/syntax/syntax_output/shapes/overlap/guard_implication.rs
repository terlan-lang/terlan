use std::collections::BTreeMap;

use crate::terlan_syntax::{
    SyntaxExprKind, SyntaxExprOutput, SyntaxPatternKind, SyntaxPatternOutput,
};

mod predicates;
mod relations;
use super::GuardPredicateDefinitions;
use predicates::{merge_predicate_constraints, predicate_constraints_imply, PredicateFact};
use relations::{relation_constraints_imply, RelationFact};

const MAX_INTERVAL_BRANCHES: usize = 64;
const MAX_PREDICATE_EXPANSIONS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardRelation {
    Equivalent,
    Implied,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
struct Bound {
    value: i64,
    inclusive: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct IntRange {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

#[derive(Debug, Clone, Default)]
struct Constraints {
    ranges: BTreeMap<String, IntRange>,
    relations: Vec<RelationFact>,
    predicates: Vec<PredicateFact>,
    impossible: bool,
}

pub(super) fn guard_relation(
    earlier: Option<&SyntaxExprOutput>,
    earlier_patterns: &[SyntaxPatternOutput],
    later: Option<&SyntaxExprOutput>,
    later_patterns: &[SyntaxPatternOutput],
    guard_predicates: &GuardPredicateDefinitions,
) -> GuardRelation {
    let (Some(earlier), Some(later)) = (earlier, later) else {
        return GuardRelation::Unknown;
    };
    if exceeds_logical_branch_budget(earlier) || exceeds_logical_branch_budget(later) {
        return GuardRelation::Unknown;
    }
    if !is_simple_guard(earlier) || !is_simple_guard(later) {
        return GuardRelation::Unknown;
    }
    let earlier = canonical_guard(earlier, earlier_patterns, guard_predicates);
    let later = canonical_guard(later, later_patterns, guard_predicates);
    if earlier == later {
        return GuardRelation::Equivalent;
    }
    let (Some(earlier), Some(later)) = (guard_constraints(&earlier), guard_constraints(&later))
    else {
        return GuardRelation::Unknown;
    };
    if constraints_imply(&later, &earlier) {
        GuardRelation::Implied
    } else {
        GuardRelation::Unknown
    }
}

/// Rejects oversized Boolean proof trees without recursively entering them.
fn exceeds_logical_branch_budget(expression: &SyntaxExprOutput) -> bool {
    let mut logical_operators = 0_usize;
    let mut pending = vec![expression];
    while let Some(current) = pending.pop() {
        if current.kind == SyntaxExprKind::BinaryOp
            && matches!(current.operator.as_deref(), Some("and" | "or"))
        {
            logical_operators = logical_operators.saturating_add(1);
            if logical_operators >= MAX_INTERVAL_BRANCHES {
                return true;
            }
        }
        pending.extend(&current.children);
        pending.extend(current.fields.iter().map(|field| field.value.as_ref()));
    }
    false
}

fn is_simple_guard(expression: &SyntaxExprOutput) -> bool {
    expression.patterns.is_empty()
        && expression.clauses.is_empty()
        && expression.catch_clauses.is_empty()
        && expression.try_after.is_none()
        && expression.html_nodes.is_empty()
        && expression.children.iter().all(is_simple_guard)
        && expression
            .fields
            .iter()
            .all(|field| is_simple_guard(&field.value))
}

fn canonical_guard(
    expression: &SyntaxExprOutput,
    patterns: &[SyntaxPatternOutput],
    guard_predicates: &GuardPredicateDefinitions,
) -> SyntaxExprOutput {
    let mut bindings = BTreeMap::new();
    for (index, pattern) in patterns.iter().enumerate() {
        collect_pattern_bindings(pattern, &format!("arg{index}"), &mut bindings);
    }
    let mut canonical = expression.clone();
    let mut remaining_expansions = MAX_PREDICATE_EXPANSIONS;
    inline_guard_predicates(&mut canonical, guard_predicates, &mut remaining_expansions);
    normalize_guard(&mut canonical, &bindings);
    canonical
}

fn inline_guard_predicates(
    expression: &mut SyntaxExprOutput,
    definitions: &GuardPredicateDefinitions,
    remaining_expansions: &mut usize,
) {
    for child in &mut expression.children {
        inline_guard_predicates(child, definitions, remaining_expansions);
    }
    for field in &mut expression.fields {
        inline_guard_predicates(&mut field.value, definitions, remaining_expansions);
    }
    if *remaining_expansions == 0
        || expression.kind != SyntaxExprKind::Call
        || expression.remote.is_some()
        || !expression.type_args.is_empty()
        || expression.arg_names.iter().any(Option::is_some)
    {
        return;
    }
    let Some(callee) = expression.children.first() else {
        return;
    };
    if callee.kind != SyntaxExprKind::Var {
        return;
    }
    let Some(name) = callee.text.as_deref() else {
        return;
    };
    let arguments = &expression.children[1..];
    let Some(definition) = definitions.get(&(name.to_string(), arguments.len())) else {
        return;
    };
    let substitutions = definition
        .params
        .iter()
        .cloned()
        .zip(arguments.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    let mut body = definition.body.clone();
    substitute_predicate_parameters(&mut body, &substitutions);
    *remaining_expansions -= 1;
    *expression = body;
}

fn substitute_predicate_parameters(
    expression: &mut SyntaxExprOutput,
    substitutions: &BTreeMap<String, SyntaxExprOutput>,
) {
    if expression.kind == SyntaxExprKind::Var {
        if let Some(replacement) = expression
            .text
            .as_ref()
            .and_then(|name| substitutions.get(name))
        {
            *expression = replacement.clone();
            return;
        }
    }
    for child in &mut expression.children {
        substitute_predicate_parameters(child, substitutions);
    }
    for field in &mut expression.fields {
        substitute_predicate_parameters(&mut field.value, substitutions);
    }
}

fn collect_pattern_bindings(
    pattern: &SyntaxPatternOutput,
    path: &str,
    bindings: &mut BTreeMap<String, String>,
) {
    match pattern.kind {
        SyntaxPatternKind::Var => {
            if let Some(name) = &pattern.text {
                bindings.insert(name.clone(), path.to_string());
            }
        }
        SyntaxPatternKind::Alias => {
            if let Some(name) = &pattern.text {
                bindings.insert(name.clone(), format!("{path}.alias"));
            }
        }
        SyntaxPatternKind::StringCapture => {
            if let Some(name) = pattern
                .text
                .as_deref()
                .and_then(|text| text.split_once(':').map(|(name, _)| name.trim()))
            {
                bindings.insert(name.to_string(), path.to_string());
            }
        }
        _ => {}
    }
    for (index, child) in pattern.children.iter().enumerate() {
        collect_pattern_bindings(child, &format!("{path}.child{index}"), bindings);
    }
    let mut fields = pattern.fields.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.key.cmp(&right.key));
    for field in fields {
        collect_pattern_bindings(
            &field.value,
            &format!("{path}.field.{}", field.key),
            bindings,
        );
    }
}

fn normalize_guard(expression: &mut SyntaxExprOutput, bindings: &BTreeMap<String, String>) {
    expression.span = Default::default();
    expression.raw = None;
    if expression.kind == SyntaxExprKind::Var {
        if let Some(path) = expression.text.as_ref().and_then(|name| bindings.get(name)) {
            expression.text = Some(path.clone());
        }
    }
    for child in &mut expression.children {
        normalize_guard(child, bindings);
    }
    for field in &mut expression.fields {
        normalize_guard(&mut field.value, bindings);
    }
}

fn guard_constraints(expression: &SyntaxExprOutput) -> Option<Vec<Constraints>> {
    collect_constraint_branches(expression, true)
}

fn collect_constraint_branches(
    expression: &SyntaxExprOutput,
    positive: bool,
) -> Option<Vec<Constraints>> {
    if expression.kind == SyntaxExprKind::UnaryOp
        && expression.operator.as_deref() == Some("not")
        && expression.children.len() == 1
    {
        return collect_constraint_branches(&expression.children[0], !positive);
    }
    if expression.kind != SyntaxExprKind::BinaryOp {
        let mut constraints = Constraints::default();
        constraints
            .predicates
            .push(PredicateFact::new_with_polarity(expression, positive)?);
        return Some(vec![constraints]);
    }
    let operator = expression.operator.as_deref()?;
    if matches!(operator, "and" | "or") {
        if expression.children.len() != 2 {
            return None;
        }
        let left = collect_constraint_branches(&expression.children[0], positive)?;
        let right = collect_constraint_branches(&expression.children[1], positive)?;
        let disjunction = (operator == "or") == positive;
        if disjunction {
            let branch_count = left.len().checked_add(right.len())?;
            if branch_count > MAX_INTERVAL_BRANCHES {
                return None;
            }
            return Some(left.into_iter().chain(right).collect());
        }
        let branch_count = left.len().checked_mul(right.len())?;
        if branch_count > MAX_INTERVAL_BRANCHES {
            return None;
        }
        return Some(
            left.into_iter()
                .flat_map(|left| {
                    right
                        .iter()
                        .cloned()
                        .map(move |right| merge_constraints(left.clone(), right))
                })
                .collect(),
        );
    }
    let operator = if positive {
        operator
    } else {
        negate_comparison_operator(operator)?
    };
    let mut constraints = Constraints::default();
    if let Some((variable, operator, value)) = comparison(expression, operator) {
        if operator == "!=" {
            return integer_inequality_constraints(variable, value);
        }
        apply_comparison(&mut constraints, variable, operator, value)?;
    } else {
        constraints
            .relations
            .push(relation_fact(expression, operator)?);
    }
    Some(vec![constraints])
}

fn integer_inequality_constraints(variable: String, value: i64) -> Option<Vec<Constraints>> {
    ["<", ">"]
        .into_iter()
        .map(|operator| {
            let mut constraints = Constraints::default();
            apply_comparison(&mut constraints, variable.clone(), operator, value)?;
            Some(constraints)
        })
        .collect()
}

fn merge_constraints(mut left: Constraints, right: Constraints) -> Constraints {
    left.impossible |= right.impossible;
    left.impossible |= merge_predicate_constraints(&mut left.predicates, right.predicates);
    for relation in right.relations {
        if !left.relations.contains(&relation) {
            left.relations.push(relation);
        }
    }
    for (variable, range) in right.ranges {
        let target = left.ranges.entry(variable).or_default();
        if let Some(lower) = range.lower {
            strengthen_lower(target, lower);
        }
        if let Some(upper) = range.upper {
            strengthen_upper(target, upper);
        }
        left.impossible |= range_is_empty(*target);
    }
    left
}

fn apply_comparison(
    constraints: &mut Constraints,
    variable: String,
    operator: &str,
    value: i64,
) -> Option<()> {
    let range = constraints.ranges.entry(variable).or_default();
    match operator {
        ">" => strengthen_lower(
            range,
            Bound {
                value,
                inclusive: false,
            },
        ),
        ">=" => strengthen_lower(
            range,
            Bound {
                value,
                inclusive: true,
            },
        ),
        "<" => strengthen_upper(
            range,
            Bound {
                value,
                inclusive: false,
            },
        ),
        "<=" => strengthen_upper(
            range,
            Bound {
                value,
                inclusive: true,
            },
        ),
        "==" => {
            let bound = Bound {
                value,
                inclusive: true,
            };
            strengthen_lower(range, bound);
            strengthen_upper(range, bound);
        }
        _ => return None,
    }
    constraints.impossible |= range_is_empty(*range);
    Some(())
}

fn comparison<'a>(
    expression: &'a SyntaxExprOutput,
    operator: &'a str,
) -> Option<(String, &'a str, i64)> {
    if expression.children.len() != 2 {
        return None;
    }
    let left = &expression.children[0];
    let right = &expression.children[1];
    if let (Some(variable), Some(value)) = (bound_variable(left), integer_value(right)) {
        return Some((variable, operator, value));
    }
    let (Some(value), Some(variable)) = (integer_value(left), bound_variable(right)) else {
        return None;
    };
    Some((variable, reverse_operator(operator)?, value))
}

fn relation_fact(expression: &SyntaxExprOutput, operator: &str) -> Option<RelationFact> {
    if expression.children.len() != 2 {
        return None;
    }
    let left = bound_variable(&expression.children[0])?;
    let right = bound_variable(&expression.children[1])?;
    RelationFact::new(left, operator, right)
}

fn bound_variable(expression: &SyntaxExprOutput) -> Option<String> {
    (expression.kind == SyntaxExprKind::Var)
        .then_some(expression.text.as_deref())
        .flatten()
        .filter(|name| name.starts_with("arg"))
        .map(str::to_string)
}

fn integer_value(expression: &SyntaxExprOutput) -> Option<i64> {
    if expression.kind == SyntaxExprKind::Int {
        return expression.text.as_deref()?.parse().ok();
    }
    if expression.kind == SyntaxExprKind::UnaryOp
        && expression.operator.as_deref() == Some("-")
        && expression.children.len() == 1
    {
        return integer_value(&expression.children[0])?.checked_neg();
    }
    None
}

fn reverse_operator(operator: &str) -> Option<&str> {
    match operator {
        ">" => Some("<"),
        ">=" => Some("<="),
        "<" => Some(">"),
        "<=" => Some(">="),
        "==" => Some("=="),
        "!=" => Some("!="),
        _ => None,
    }
}

fn negate_comparison_operator(operator: &str) -> Option<&str> {
    match operator {
        ">" => Some("<="),
        ">=" => Some("<"),
        "<" => Some(">="),
        "<=" => Some(">"),
        "==" => Some("!="),
        "!=" => Some("=="),
        _ => None,
    }
}

fn strengthen_lower(range: &mut IntRange, candidate: Bound) {
    range.lower = Some(match range.lower {
        None => candidate,
        Some(current) if candidate.value > current.value => candidate,
        Some(current) if candidate.value < current.value => current,
        Some(current) => Bound {
            value: current.value,
            inclusive: current.inclusive && candidate.inclusive,
        },
    });
}

fn strengthen_upper(range: &mut IntRange, candidate: Bound) {
    range.upper = Some(match range.upper {
        None => candidate,
        Some(current) if candidate.value < current.value => candidate,
        Some(current) if candidate.value > current.value => current,
        Some(current) => Bound {
            value: current.value,
            inclusive: current.inclusive && candidate.inclusive,
        },
    });
}

fn range_is_empty(range: IntRange) -> bool {
    let (Some(lower), Some(upper)) = (range.lower, range.upper) else {
        return false;
    };
    lower.value > upper.value
        || (lower.value == upper.value && (!lower.inclusive || !upper.inclusive))
}

fn constraints_imply(later: &[Constraints], earlier: &[Constraints]) -> bool {
    later.iter().all(|later_branch| {
        earlier
            .iter()
            .any(|earlier_branch| constraint_implies(later_branch, earlier_branch))
    })
}

fn constraint_implies(later: &Constraints, earlier: &Constraints) -> bool {
    if later.impossible {
        return true;
    }
    if earlier.impossible {
        return false;
    }
    relation_constraints_imply(&later.relations, &earlier.relations)
        && predicate_constraints_imply(&later.predicates, &earlier.predicates)
        && earlier.ranges.iter().all(|(variable, earlier_range)| {
            later
                .ranges
                .get(variable)
                .is_some_and(|later_range| range_is_subset(*later_range, *earlier_range))
        })
}

fn range_is_subset(later: IntRange, earlier: IntRange) -> bool {
    lower_is_subset(later.lower, earlier.lower) && upper_is_subset(later.upper, earlier.upper)
}

fn lower_is_subset(later: Option<Bound>, earlier: Option<Bound>) -> bool {
    let Some(earlier) = earlier else {
        return true;
    };
    let Some(later) = later else {
        return false;
    };
    later.value > earlier.value
        || (later.value == earlier.value && (!later.inclusive || earlier.inclusive))
}

fn upper_is_subset(later: Option<Bound>, earlier: Option<Bound>) -> bool {
    let Some(earlier) = earlier else {
        return true;
    };
    let Some(later) = later else {
        return false;
    };
    later.value < earlier.value
        || (later.value == earlier.value && (!later.inclusive || earlier.inclusive))
}
