//! Type recovery and validation shared by structured-pattern lowering.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CoreMapTypeField, CorePattern, CorePrimitiveIntrinsic,
    CoreTupleTypeElem, CoreType,
};

use super::super::{native_type, NativeConstructorLayouts, NativeExpr, NativeType};

/// Shared grouped-let fallbacks span different closed unions and scalar guards.
/// Disjoint arms must not project fields or lower unreachable bindings. Unknown
/// and nominal types stay conservative: a missing layout is not disjointness.
pub(super) fn type_excludes_pattern(pattern: &CorePattern, core_type: Option<&CoreType>) -> bool {
    if let CorePattern::Alias { pattern, .. } = pattern {
        return type_excludes_pattern(pattern, core_type);
    }
    match (pattern, core_type) {
        (_, Some(CoreType::Union(variants))) => variants
            .iter()
            .all(|ty| type_excludes_pattern(pattern, Some(ty))),
        (CorePattern::Atom(expected), Some(CoreType::AtomLiteral(actual))) => expected != actual,
        (CorePattern::Atom(_), Some(CoreType::Tuple(_))) => true,
        (CorePattern::Tuple(patterns), Some(CoreType::Tuple(elements))) => {
            patterns.len() != elements.len()
                || patterns.iter().zip(elements).any(|(pattern, element)| {
                    type_excludes_pattern(pattern, Some(tuple_element_type(element)))
                })
        }
        (
            CorePattern::Tuple(_),
            Some(
                CoreType::Bool
                | CoreType::Int
                | CoreType::Float
                | CoreType::Number
                | CoreType::String
                | CoreType::Binary
                | CoreType::Atom
                | CoreType::AtomLiteral(_),
            ),
        ) => true,
        _ => false,
    }
}

pub(super) fn native_core_type(
    ty: &CoreType,
) -> Result<NativeType, crate::compiler::native_ir::NativeIrError> {
    Ok(native_type(Some(ty), &ty.contract_text()).ok_or_else(|| {
        format!(
            "error[native_ir.structured_pattern_type]: unsupported `{}`",
            ty.contract_text()
        )
    })?)
}

pub(super) fn core_expr_type(
    expr: &CoreExpr,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
) -> Option<CoreType> {
    core_expr_type_impl(expr, types, functions, None)
}

fn core_expr_type_impl(
    expr: &CoreExpr,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    constructors: Option<&NativeConstructorLayouts>,
) -> Option<CoreType> {
    let inferred = match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) => Some(CoreType::AtomLiteral(value.clone())),
        CoreExpr::Var(name) if matches!(name.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Var(name) if name == "Unit" => Some(CoreType::Named("Unit".into())),
        CoreExpr::Var(name) => types.get(name).cloned(),
        CoreExpr::Call { function, args, .. } => {
            functions.get(&(function.clone(), args.len())).cloned()
        }
        CoreExpr::FunctionCall { callee, args } => {
            let CoreType::Arrow {
                params,
                return_type,
            } = core_expr_type_impl(callee, types, functions, constructors)?
            else {
                return None;
            };
            (params.len() == args.len()).then(|| return_type.as_ref().clone())
        }
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } if matches!(constructor.rsplit('.').next(), Some("Some" | "Ok" | "Err")) => {
            let tag = tagged_constructor_atom(constructor)?;
            let mut elements = Vec::with_capacity(args.len().saturating_add(1));
            elements.push(CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag)));
            elements.extend(
                args.iter()
                    .map(|argument| {
                        core_expr_type_impl(argument, types, functions, constructors)
                            .map(CoreTupleTypeElem::Type)
                    })
                    .collect::<Option<Vec<_>>>()?,
            );
            Some(CoreType::Tuple(elements))
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if index == 0 {
                    if let CoreExpr::Atom(tag) = item {
                        return Some(CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag.clone())));
                    }
                }
                core_expr_type_impl(item, types, functions, constructors)
                    .map(CoreTupleTypeElem::Type)
            })
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::Map(fields) if fields.iter().all(|field| field.required) => fields
            .iter()
            .map(|field| {
                Some(CoreMapTypeField {
                    key: field.key.clone(),
                    operator: ":".into(),
                    value: core_expr_type_impl(&field.value, types, functions, constructors)?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Map),
        CoreExpr::List(items) => super::super::expression::homogeneous_list_type(items, |item| {
            core_expr_type_impl(item, types, functions, constructors)
        }),
        CoreExpr::RecordConstruct { name, .. } => constructors
            .and_then(|layouts| {
                super::super::constructors::record_construct_result_core_type(expr, layouts)
            })
            .or_else(|| Some(CoreType::Named(name.clone()))),
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base = core_expr_type_impl(base, types, functions, constructors)?;
            let CoreType::Struct { fields, .. } = base else {
                return None;
            };
            fields
                .into_iter()
                .find(|candidate| candidate.name == *field)
                .map(|candidate| candidate.ty)
        }
        CoreExpr::Index { base, index } => {
            let CoreExpr::Int(index) = index.as_ref() else {
                return None;
            };
            let index = usize::try_from(*index).ok()?;
            match core_expr_type_impl(base, types, functions, constructors)? {
                CoreType::Tuple(elements) => elements.get(index).map(tuple_element_type).cloned(),
                CoreType::List(element) => Some(*element),
                CoreType::Apply { constructor, args }
                    if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
                {
                    args.into_iter().next()
                }
                _ => None,
            }
        }
        CoreExpr::Cast { expr, target_type }
            if matches!(target_type, CoreType::Dynamic)
                || matches!(target_type, CoreType::Named(name) if name == "Dynamic") =>
        {
            core_expr_type_impl(expr, types, functions, constructors)
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(
                    CorePrimitiveIntrinsic::ListConcat
                        | CorePrimitiveIntrinsic::ListSubtract
                        | CorePrimitiveIntrinsic::ListIterator
                        | CorePrimitiveIntrinsic::ListPush
                        | CorePrimitiveIntrinsic::ListClear
                )
            ) =>
        {
            call.args
                .first()
                .and_then(|operand| core_expr_type_impl(operand, types, functions, constructors))
        }
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::UnaryOp { operator, operand } => match operator.as_str() {
            "not" | "!" => Some(CoreType::Bool),
            "-" => core_expr_type_impl(operand, types, functions, constructors),
            _ => None,
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => match operator.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or" => Some(CoreType::Bool),
            "div" | "rem" => Some(CoreType::Int),
            "+" | "-" | "*" | "/" => {
                let left = core_expr_type_impl(left, types, functions, constructors)?;
                let right = core_expr_type_impl(right, types, functions, constructors)?;
                if left == CoreType::Float || right == CoreType::Float {
                    Some(CoreType::Float)
                } else {
                    Some(left)
                }
            }
            _ => None,
        },
        CoreExpr::Let { bindings, body } => {
            let mut lexical = types.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let inferred =
                    core_expr_type_impl(&binding.value, &lexical, functions, constructors);
                lexical.remove(name);
                if let Some(ty) = inferred {
                    lexical.insert(name.clone(), ty);
                }
            }
            core_expr_type_impl(body, &lexical, functions, constructors)
        }
        CoreExpr::If { clauses } => control_result_type(
            clauses.iter().map(|clause| &clause.body),
            types,
            functions,
            constructors,
        ),
        CoreExpr::Case { scrutinee, clauses } => {
            case_result_type(scrutinee, clauses, types, functions, constructors)
        }
        _ => None,
    };
    inferred.map(normalize_inferred_type)
}

/// Recovers fields projected from recursive nominal struct anchors. Alias
/// expansion deliberately retains those anchors to keep recursive Core types
/// finite; constructor metadata owns the corresponding one-layer declaration.
pub(super) fn core_expr_type_with_constructors(
    expr: &CoreExpr,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    if let Some(ty) = core_expr_type_impl(expr, types, functions, Some(constructors)) {
        return Some(ty);
    }
    let (CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. }) = expr
    else {
        return None;
    };
    let base = core_expr_type_with_constructors(base, types, functions, constructors)?;
    let canonical = super::super::expression::managed_semantic_contract(&base);
    constructors
        .values()
        .filter_map(|layout| layout.result_core_type.as_ref())
        .find_map(|candidate| {
            let CoreType::Struct { fields, .. } = candidate else {
                return None;
            };
            (super::super::expression::managed_semantic_contract(candidate) == canonical)
                .then(|| fields.iter().find(|candidate| candidate.name == *field))
                .flatten()
                .map(|candidate| candidate.ty.clone())
        })
}

/// Recovers a case result while adding the variables introduced by each
/// checked clause pattern to that clause's lexical type environment.
fn case_result_type(
    scrutinee: &CoreExpr,
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    constructors: Option<&NativeConstructorLayouts>,
) -> Option<CoreType> {
    let scrutinee_type = core_expr_type_impl(scrutinee, types, functions, constructors);
    if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
        eprintln!(
            "[native-aot case-type] scrutinee={} type={} variables={:?}",
            scrutinee.contract_text(),
            scrutinee_type
                .as_ref()
                .map_or_else(|| "unknown".to_string(), CoreType::contract_text),
            types.keys().collect::<Vec<_>>(),
        );
    }
    let mut result = None;
    let mut inherited_none = false;
    for clause in clauses {
        if is_nullary_none(&clause.body) {
            inherited_none = true;
            continue;
        }
        let mut lexical = types.clone();
        if let Some(scrutinee_type) = scrutinee_type.as_ref() {
            bind_pattern_types(&clause.pattern, scrutinee_type, &mut lexical);
        }
        let clause_type = core_expr_type_impl(&clause.body, &lexical, functions, constructors);
        if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
            eprintln!(
                "[native-aot case-type] pattern={} body={} type={clause_type:?} variables={:?}",
                clause.pattern.contract_text(),
                clause.body.contract_text(),
                lexical.keys().collect::<Vec<_>>(),
            );
        }
        match clause_type {
            Some(found) => match result.take() {
                Some(expected) => result = Some(merge_control_types(expected, found)?),
                None => result = Some(found),
            },
            None => return None,
        }
    }
    complete_control_result(result?, inherited_none)
}

/// Adds every variable introduced by one structural pattern to a lexical type
/// environment using the already checked scrutinee type.
fn bind_pattern_types(pattern: &CorePattern, ty: &CoreType, types: &mut HashMap<String, CoreType>) {
    match pattern {
        CorePattern::Var(name) => {
            types.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            types.insert(alias.clone(), ty.clone());
            bind_pattern_types(pattern, ty, types);
        }
        CorePattern::Tuple(patterns) => {
            let structural =
                super::super::generic_specialization::structural_tuple_variant(patterns, ty);
            let structural_ty = structural.as_ref().unwrap_or(ty);
            let elements = match structural_ty {
                CoreType::Tuple(elements) => elements.as_slice(),
                CoreType::Union(variants) => {
                    let Some(CorePattern::Atom(tag)) = patterns.first() else {
                        return;
                    };
                    let Some(elements) = variants.iter().find_map(|variant| {
                        let CoreType::Tuple(elements) = variant else {
                            return None;
                        };
                        matches!(
                            elements.first().map(tuple_element_type),
                            Some(CoreType::AtomLiteral(candidate)) if candidate == tag
                        )
                        .then_some(elements.as_slice())
                    }) else {
                        return;
                    };
                    elements
                }
                _ => return,
            };
            for (pattern, element) in patterns.iter().zip(elements) {
                bind_pattern_types(pattern, tuple_element_type(element), types);
            }
        }
        CorePattern::Constructor { name, args, .. } if name.rsplit('.').next() == Some("Some") => {
            let Some(element) = option_element_type(Some(ty)) else {
                return;
            };
            if let [argument] = args.as_slice() {
                bind_pattern_types(argument, element, types);
            }
        }
        CorePattern::List(patterns) => {
            let Ok(element) = list_element_type(Some(ty)) else {
                return;
            };
            for pattern in patterns {
                bind_pattern_types(pattern, element, types);
            }
        }
        CorePattern::ListCons { head, tail } => {
            let Ok(list) = list_variant_type(Some(ty)) else {
                return;
            };
            let Ok(element) = list_element_type(Some(list)) else {
                return;
            };
            bind_pattern_types(head, element, types);
            bind_pattern_types(tail, list, types);
        }
        _ => {}
    }
}

/// Returns the runtime tag carried by a transparent standard constructor.
fn tagged_constructor_atom(constructor: &str) -> Option<String> {
    match constructor.rsplit('.').next()? {
        "Some" => Some("some".to_string()),
        "Ok" => Some("ok".to_string()),
        "Err" => Some("error".to_string()),
        _ => None,
    }
}

/// Recovers one checked control result while permitting the atom-form nullary
/// `None` arm to inherit the concrete managed `Option` representation from a
/// sibling. Type checking has already established branch compatibility; this
/// step restores the erased structural type needed by NativeIR allocation.
fn control_result_type<'a>(
    expressions: impl Iterator<Item = &'a CoreExpr>,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    constructors: Option<&NativeConstructorLayouts>,
) -> Option<CoreType> {
    let mut result = None;
    let mut inherited_none = false;
    for expression in expressions {
        if is_nullary_none(expression) {
            inherited_none = true;
            continue;
        }
        match core_expr_type_impl(expression, types, functions, constructors) {
            Some(found) => match result.take() {
                Some(expected) => result = Some(merge_control_types(expected, found)?),
                None => result = Some(found),
            },
            None => return None,
        }
    }
    complete_control_result(result?, inherited_none)
}

/// Reconstructs a checked tagged union whose transparent constructors were
/// lowered to distinct tuple or atom variants in CoreIR.
/// Joins compatible values while retaining closed constructor variants and labels.
pub(in crate::compiler::native_ir) fn merge_control_types(
    expected: CoreType,
    found: CoreType,
) -> Option<CoreType> {
    if expected == found {
        return Some(expected);
    }
    if expected == CoreType::Never {
        return Some(found);
    }
    if found == CoreType::Never {
        return Some(expected);
    }
    if let (CoreType::List(left), CoreType::List(right)) = (&expected, &found) {
        return merge_control_types((**left).clone(), (**right).clone())
            .map(|element| CoreType::List(Box::new(element)));
    }
    if let (CoreType::Tuple(left), CoreType::Tuple(right)) = (&expected, &found) {
        let left_tag = left
            .first()
            .and_then(|element| match tuple_element_type(element) {
                CoreType::AtomLiteral(tag) => Some(tag),
                _ => None,
            });
        let right_tag = right
            .first()
            .and_then(|element| match tuple_element_type(element) {
                CoreType::AtomLiteral(tag) => Some(tag),
                _ => None,
            });
        // Products join field-by-field. Constructor variants only do so when
        // their provider tag is identical; different tags remain alternatives
        // of one closed union rather than becoming a product of unions.
        if left.len() == right.len()
            && ((left_tag.is_none() && right_tag.is_none()) || left_tag == right_tag)
        {
            let elements = left
                .iter()
                .cloned()
                .zip(right.iter().cloned())
                .map(|(left, right)| merge_control_tuple_elements(left, right))
                .collect::<Option<Vec<_>>>()?;
            return Some(CoreType::Tuple(elements));
        }
    }
    // Constructor expressions recover positional payloads, while checked
    // signatures retain payload labels. Prefer the covering signature's
    // layout instead of manufacturing duplicate variants with different IDs.
    if union_covers(&found, &expected)
        && (!union_covers(&expected, &found) || payload_labels(&found) > payload_labels(&expected))
    {
        return Some(found);
    }
    if union_covers(&expected, &found) {
        return Some(expected);
    }
    let mut variants = match expected {
        CoreType::Union(variants) if variants.iter().all(is_tagged_variant) => variants,
        variant if is_tagged_variant(&variant) => vec![variant],
        _ => return None,
    };
    let additions = match found {
        CoreType::Union(variants) if variants.iter().all(is_tagged_variant) => variants,
        variant if is_tagged_variant(&variant) => vec![variant],
        _ => return None,
    };
    for variant in additions {
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }
    Some(CoreType::Union(canonical_atom_union(variants)))
}

/// Restores the checked structural-union spelling retained by constructor layouts.
///
/// Control joins infer variants from executable tuples and atoms, which do not
/// retain declaration order or payload labels. Those details participate in a
/// managed semantic identity, so nested products must recover the canonical
/// checked union before NativeIR assigns their ABI type.
pub(in crate::compiler::native_ir) fn canonical_control_type(
    ty: CoreType,
    constructors: &NativeConstructorLayouts,
) -> CoreType {
    let ty = match ty {
        CoreType::List(element) => {
            CoreType::List(Box::new(canonical_control_type(*element, constructors)))
        }
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .into_iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => {
                        CoreTupleTypeElem::Type(canonical_control_type(ty, constructors))
                    }
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name,
                        ty: canonical_control_type(ty, constructors),
                    },
                })
                .collect(),
        ),
        CoreType::Union(variants) => CoreType::Union(
            variants
                .into_iter()
                .map(|variant| canonical_control_type(variant, constructors))
                .collect(),
        ),
        other => other,
    };
    if !matches!(ty, CoreType::Union(_)) {
        return ty;
    }
    constructors
        .values()
        .filter_map(|layout| layout.result_core_type.as_ref())
        .filter(|candidate| matches!(candidate, CoreType::Union(_)))
        .filter(|candidate| control_types_equivalent(&ty, candidate))
        .max_by(|left, right| {
            payload_labels(left)
                .cmp(&payload_labels(right))
                .then_with(|| right.contract_text().cmp(&left.contract_text()))
        })
        .cloned()
        .unwrap_or(ty)
}

pub(in crate::compiler::native_ir) fn control_types_equivalent(
    left: &CoreType,
    right: &CoreType,
) -> bool {
    match (left, right) {
        (CoreType::Tuple(left), CoreType::Tuple(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    control_types_equivalent(tuple_element_type(left), tuple_element_type(right))
                })
        }
        (CoreType::Union(left), CoreType::Union(right)) => {
            left.len() == right.len()
                && left.iter().all(|left| {
                    right
                        .iter()
                        .any(|right| control_types_equivalent(left, right))
                })
        }
        (CoreType::List(left), CoreType::List(right)) => control_types_equivalent(left, right),
        (CoreType::Named(left), CoreType::Struct { name: right, .. })
        | (CoreType::Struct { name: left, .. }, CoreType::Named(right)) => left == right,
        _ => left == right,
    }
}

fn merge_control_tuple_elements(
    expected: CoreTupleTypeElem,
    found: CoreTupleTypeElem,
) -> Option<CoreTupleTypeElem> {
    match (expected, found) {
        (CoreTupleTypeElem::Type(expected), CoreTupleTypeElem::Type(found)) => {
            merge_control_types(expected, found).map(CoreTupleTypeElem::Type)
        }
        (
            CoreTupleTypeElem::Field {
                name: expected_name,
                ty: expected,
            },
            CoreTupleTypeElem::Field {
                name: found_name,
                ty: found,
            },
        ) if expected_name == found_name => {
            merge_control_types(expected, found).map(|ty| CoreTupleTypeElem::Field {
                name: expected_name,
                ty,
            })
        }
        (CoreTupleTypeElem::Field { name, ty: expected }, CoreTupleTypeElem::Type(found))
        | (CoreTupleTypeElem::Type(found), CoreTupleTypeElem::Field { name, ty: expected }) => {
            merge_control_types(expected, found).map(|ty| CoreTupleTypeElem::Field { name, ty })
        }
        _ => None,
    }
}

/// Gives equivalent closed atom domains one order, without reordering payload layouts.
pub(in crate::compiler::native_ir) fn canonical_atom_union(
    mut variants: Vec<CoreType>,
) -> Vec<CoreType> {
    if variants
        .iter()
        .all(|ty| matches!(ty, CoreType::AtomLiteral(_)))
    {
        variants.sort_by_cached_key(CoreType::contract_text);
        variants.dedup();
    }
    variants
}

fn union_covers(cover: &CoreType, candidate: &CoreType) -> bool {
    let CoreType::Union(variants) = cover else {
        return false;
    };
    if variants.is_empty() || !variants.iter().all(is_tagged_variant) {
        return false;
    }
    let covered = |candidate: &CoreType| {
        variants.iter().any(|variant| match (variant, candidate) {
            (CoreType::Tuple(left), CoreType::Tuple(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| tuple_element_type(left) == tuple_element_type(right))
            }
            _ => variant == candidate,
        })
    };
    match candidate {
        CoreType::Union(candidates) => candidates.iter().all(covered),
        candidate => covered(candidate),
    }
}

fn payload_labels(ty: &CoreType) -> usize {
    match ty {
        CoreType::Union(variants) => variants.iter().map(payload_labels).sum(),
        CoreType::Tuple(elements) => elements
            .iter()
            .filter(|element| matches!(element, CoreTupleTypeElem::Field { .. }))
            .count(),
        _ => 0,
    }
}

/// Reports whether a type is one transparent constructor variant.
fn is_tagged_variant(ty: &CoreType) -> bool {
    match ty {
        CoreType::AtomLiteral(_) => true,
        CoreType::Tuple(elements) => matches!(
            elements.first().map(tuple_element_type),
            Some(CoreType::AtomLiteral(_))
        ),
        _ => false,
    }
}

/// Restores the transparent `None | Some[T]` union after branch inference.
fn complete_control_result(result: CoreType, inherited_none: bool) -> Option<CoreType> {
    if !inherited_none || option_like_type(&result) {
        return Some(result);
    }
    tagged_some_tuple(&result)
        .then(|| CoreType::Union(vec![CoreType::AtomLiteral("none".to_string()), result]))
}

fn is_nullary_none(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Atom(name) | CoreExpr::Var(name) => name.eq_ignore_ascii_case("none"),
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } => args.is_empty() && constructor.rsplit('.').next() == Some("None"),
        CoreExpr::Cast { expr, .. } => is_nullary_none(expr),
        CoreExpr::Let { body, .. } => is_nullary_none(body),
        CoreExpr::If { clauses } => {
            !clauses.is_empty() && clauses.iter().all(|clause| is_nullary_none(&clause.body))
        }
        CoreExpr::Case { clauses, .. } => {
            !clauses.is_empty() && clauses.iter().all(|clause| is_nullary_none(&clause.body))
        }
        _ => false,
    }
}

fn option_like_type(ty: &CoreType) -> bool {
    match ty {
        CoreType::Apply { constructor, args } => {
            constructor.rsplit('.').next() == Some("Option") && args.len() == 1
        }
        CoreType::Union(variants) => variants
            .iter()
            .any(|variant| matches!(variant, CoreType::AtomLiteral(name) if name == "none")),
        _ => false,
    }
}

/// Reports whether a structural tuple is the payload-carrying `Some` variant.
fn tagged_some_tuple(ty: &CoreType) -> bool {
    let CoreType::Tuple(elements) = ty else {
        return false;
    };
    matches!(
        elements.first().map(tuple_element_type),
        Some(CoreType::AtomLiteral(tag)) if tag == "some"
    )
}

fn normalize_inferred_type(ty: CoreType) -> CoreType {
    let ty = match ty {
        CoreType::Apply {
            constructor,
            mut args,
        } if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 => args.remove(0),
        other => other,
    };
    // Source Unit expressions retain their native sentinel spelling, while
    // transparent aliases and intrinsic signatures carry the lowercase atom.
    // Both denote one zero-width result, never two variants of an atom union.
    match ty {
        CoreType::AtomLiteral(name) if matches!(name.as_str(), "Unit" | "unit") => {
            CoreType::Named("Unit".into())
        }
        other => other,
    }
}

pub(super) fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

pub(super) fn list_element_type(
    core_type: Option<&CoreType>,
) -> Result<&CoreType, crate::compiler::native_ir::NativeIrError> {
    match list_variant_type(core_type)? {
        CoreType::List(element) => Ok(element),
        CoreType::Apply { args, .. } => Ok(&args[0]),
        _ => unreachable!("list variant validation returned a non-list type"),
    }
}

/// Selects the unique concrete list member of a structural union.
pub(super) fn list_variant_type(
    core_type: Option<&CoreType>,
) -> Result<&CoreType, crate::compiler::native_ir::NativeIrError> {
    match core_type {
        Some(CoreType::List(_)) => Ok(core_type.expect("matched concrete list type")),
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Ok(core_type.expect("matched concrete list type"))
        }
        Some(CoreType::Union(variants)) => {
            let mut lists = variants.iter().filter(|variant| {
                matches!(variant, CoreType::List(_))
                    || matches!(
                        variant,
                        CoreType::Apply { constructor, args }
                            if constructor.rsplit('.').next() == Some("List") && args.len() == 1
                    )
            });
            match (lists.next(), lists.next()) {
                (Some(list), None) => Ok(list),
                _ => Err("error[native_ir.list_pattern_type]: structural union must contain exactly one List variant".into()),
            }
        }
        _ => Err(format!(
            "error[native_ir.list_pattern_type]: concrete List type is unavailable from {core_type:?}"
        )
        .into()),
    }
}

pub(super) fn option_element_type(core_type: Option<&CoreType>) -> Option<&CoreType> {
    match core_type {
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        Some(CoreType::Union(variants))
            if variants.iter().any(
                |variant| matches!(variant, CoreType::AtomLiteral(atom) if atom == "none"),
            ) =>
        {
            variants.iter().find_map(|variant| {
                let CoreType::Tuple(elements) = variant else {
                    return None;
                };
                let [tag, value] = elements.as_slice() else {
                    return None;
                };
                matches!(tuple_element_type(tag), CoreType::AtomLiteral(atom) if atom == "some")
                    .then(|| tuple_element_type(value))
            })
        }
        _ => None,
    }
}

pub(super) fn map_types(
    core_type: Option<&CoreType>,
) -> Result<(&CoreType, &CoreType), crate::compiler::native_ir::NativeIrError> {
    match core_type {
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Ok((&args[0], &args[1]))
        }
        _ => Err(format!(
            "error[native_ir.map_pattern_type]: concrete Map type is unavailable from {core_type:?}"
        )
        .into()),
    }
}

pub(super) fn struct_field_type<'a>(
    core_type: Option<&'a CoreType>,
    name: &str,
) -> Option<&'a CoreType> {
    let Some(CoreType::Struct { fields, .. }) = core_type else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.ty)
}

pub(super) fn map_key(
    key: &str,
    ty: &CoreType,
) -> Result<NativeExpr, crate::compiler::native_ir::NativeIrError> {
    match ty {
        CoreType::String => {
            let encoded = crate::runtime::native_image::managed::encode_string_literal(key)
                .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}"))?;
            Ok(NativeExpr::ManagedLiteral {
                encoded: encoded.into(),
            })
        }
        CoreType::Int => Ok(key
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}"))?),
        _ => Err((format!(
            "error[native_ir.map_pattern_key]: unsupported key type `{}`",
            ty.contract_text()
        ))
        .into()),
    }
}
