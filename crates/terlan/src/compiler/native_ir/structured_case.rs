//! Direct NativeIR lowering for checked structured case patterns.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_scalar_field_operation,
    encode_list_first_operation, encode_list_is_empty_operation, encode_list_rest_operation,
    encode_managed_type_is_operation, encode_managed_variant_is_operation,
    encode_map_contains_operation, encode_map_get_operation, encode_string_equal_operation,
};
use crate::terlan_typeck::{CoreExpr, CorePattern, CoreTupleTypeElem, CoreType};

use super::constructors::{managed_field_projection, NativeConstructorLayout};
use super::{NativeBinaryOperator, NativeConstructorLayouts, NativeExpr, NativeType};

#[path = "structured_case/binary.rs"]
mod binary;
#[path = "structured_case/lowering.rs"]
mod lowering;
#[path = "structured_case/type_support.rs"]
mod type_support;
use binary::binary_plan;
pub(super) use lowering::{contains_case, lower_structured_case};
use type_support::{
    list_element_type, map_key, map_types, native_core_type, option_element_type,
    struct_field_type, tuple_element_type,
};

const MAX_STRUCTURED_PATTERN_DEPTH: usize = 64;
const MAX_STRUCTURED_PATTERN_BINDINGS: usize = 128;

pub(super) fn core_expr_type(
    expr: &CoreExpr,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
) -> Option<CoreType> {
    type_support::core_expr_type(expr, types, functions)
}

#[derive(Clone)]
pub(super) struct PatternBinding {
    pub(super) name: String,
    pub(super) value: NativeExpr,
    pub(super) ty: NativeType,
    pub(super) core_ty: Option<CoreType>,
}

pub(super) struct PatternPlan {
    pub(super) predicate: NativeExpr,
    pub(super) bindings: Vec<PatternBinding>,
}

pub(super) fn pattern_plan(
    pattern: &CorePattern,
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    if depth > MAX_STRUCTURED_PATTERN_DEPTH {
        return Err(format!(
            "error[native_ir.structured_pattern_depth]: pattern exceeds {MAX_STRUCTURED_PATTERN_DEPTH} layers"
        ));
    }
    match pattern {
        CorePattern::Wildcard => Ok(always()),
        CorePattern::Var(name) if !matches!(name.as_str(), "true" | "false" | "Unit") => {
            Ok(PatternPlan {
                predicate: NativeExpr::Bool(true),
                bindings: vec![PatternBinding {
                    name: name.clone(),
                    value,
                    ty: value_type,
                    core_ty: core_type.cloned(),
                }],
            })
        }
        CorePattern::Var(name) | CorePattern::Atom(name)
            if matches!(name.as_str(), "true" | "false" | "Unit") =>
        {
            let literal = match name.as_str() {
                "true" => NativeExpr::Bool(true),
                "false" => NativeExpr::Bool(false),
                _ => NativeExpr::Unit,
            };
            Ok(equality(value, literal, value_type))
        }
        CorePattern::Atom(name) => Ok(equality(
            value,
            NativeExpr::AtomLiteral(Arc::from(name.as_str())),
            value_type,
        )),
        CorePattern::Var(name) => Ok(PatternPlan {
            predicate: NativeExpr::Bool(true),
            bindings: vec![PatternBinding {
                name: name.clone(),
                value,
                ty: value_type,
                core_ty: core_type.cloned(),
            }],
        }),
        CorePattern::Int(expected) => Ok(equality(
            value,
            NativeExpr::Int(*expected),
            NativeType::Int,
        )),
        CorePattern::Float(expected) => {
            let expected = expected.parse::<f64>().map_err(|error| {
                format!("error[native_ir.structured_float]: invalid float pattern: {error}")
            })?;
            Ok(equality(
                value,
                NativeExpr::Float(expected.to_bits()),
                NativeType::Float,
            ))
        }
        CorePattern::String(expected) => {
            let encoded = crate::runtime::native_image::managed::encode_string_literal(expected)
                .map_err(|error| format!("error[native_ir.structured_string]: {error}"))?;
            Ok(PatternPlan {
                predicate: NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_string_equal_operation()),
                    args: vec![value, NativeExpr::StringLiteral { encoded: encoded.into() }],
                },
                bindings: vec![],
            })
        }
        CorePattern::Alias { alias, pattern } => {
            let mut plan = pattern_plan(
                pattern,
                value.clone(),
                value_type,
                core_type,
                constructors,
                depth + 1,
            )?;
            plan.bindings.insert(
                0,
                PatternBinding {
                    name: alias.clone(),
                    value,
                    ty: value_type,
                    core_ty: core_type.cloned(),
                },
            );
            Ok(plan)
        }
        CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } => constructor_plan(
            name,
            constructor_identity.as_deref(),
            args,
            value,
            value_type,
            core_type,
            constructors,
            depth,
        ),
        CorePattern::Record { name, fields } => {
            let NativeType::ManagedRef(semantic) = value_type else {
                return Err("error[native_ir.record_pattern_type]: record is not managed".into());
            };
            let mut plans = vec![PatternPlan {
                predicate: NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_managed_type_is_operation(semantic)),
                    args: vec![value.clone()],
                },
                bindings: vec![],
            }];
            for field in fields {
                let (encoded, field_type) =
                    managed_field_projection(value_type, Some(name), &field.key, constructors)?;
                plans.push(pattern_plan(
                    &field.value,
                    NativeExpr::ManagedOperation {
                        encoded,
                        args: vec![value.clone()],
                    },
                    field_type,
                    struct_field_type(core_type, &field.key),
                    constructors,
                    depth + 1,
                )?);
            }
            merge(plans)
        }
        CorePattern::Tuple(patterns) => tuple_plan(
            patterns,
            value,
            value_type,
            core_type,
            constructors,
            depth,
        ),
        CorePattern::List(patterns) => list_plan(
            patterns,
            value,
            value_type,
            core_type,
            constructors,
            depth,
        ),
        CorePattern::ListCons { head, tail } => list_cons_plan(
            head,
            tail,
            value,
            value_type,
            core_type,
            constructors,
            depth,
        ),
        CorePattern::Map(fields) if matches!(core_type, Some(CoreType::Map(_))) => {
            structural_map_plan(
                fields,
                value,
                value_type,
                core_type,
                constructors,
                depth,
            )
        }
        CorePattern::Map(fields) => {
            map_plan(fields, value, value_type, core_type, constructors, depth)
        }
        CorePattern::BinaryLayout { endian, fields } => {
            binary_plan(*endian, fields, value, value_type)
        }
        CorePattern::StringPattern(_) => {
            Err("error[native_ir.structured_pattern_family]: pattern family needs a dedicated bounded matcher".to_string())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn constructor_plan(
    name: &str,
    identity: Option<&str>,
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    if name == "Unit" && patterns.is_empty() {
        return Ok(equality(value, NativeExpr::Unit, NativeType::Unit));
    }
    if let Some(element) = option_element_type(core_type) {
        return option_constructor_plan(
            name,
            patterns,
            value,
            value_type,
            element,
            constructors,
            depth,
        );
    }
    if let Some((ok, error)) = result_element_types(core_type) {
        let (discriminant, field) = match name.rsplit('.').next().unwrap_or(name) {
            "Ok" => (0, ok),
            "Err" => (1, error),
            _ => {
                return Err(format!(
                    "error[native_ir.result_pattern_variant]: `{name}` is not a Result variant"
                ))
            }
        };
        return tagged_union_constructor_plan(
            name,
            patterns,
            value,
            value_type,
            discriminant,
            2,
            std::slice::from_ref(field),
            constructors,
            depth,
        );
    }
    if let Some((discriminant, variant_count, fields)) = tagged_union_constructor(name, core_type) {
        return tagged_union_constructor_plan(
            name,
            patterns,
            value,
            value_type,
            discriminant,
            variant_count,
            &fields,
            constructors,
            depth,
        );
    }
    let layout = constructor_layout(name, identity, patterns.len(), constructors)?;
    if layout.result != value_type {
        return Err(format!(
            "error[native_ir.constructor_pattern_type]: `{name}` does not match the scrutinee"
        ));
    }
    let semantic = managed_semantic(value_type)?;
    let discriminant = layout.descriptor.discriminant().ok_or_else(|| {
        "error[native_ir.constructor_pattern_layout]: missing discriminant".to_string()
    })?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
            args: vec![value.clone()],
        },
        bindings: vec![],
    }];
    for (index, (pattern, field_type)) in patterns.iter().zip(&layout.parameters).enumerate() {
        plans.push(pattern_plan(
            pattern,
            project(value.clone(), semantic, index, *field_type)?,
            *field_type,
            None,
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

fn result_element_types(core_type: Option<&CoreType>) -> Option<(&CoreType, &CoreType)> {
    match core_type? {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn tagged_union_constructor_plan(
    name: &str,
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    discriminant: u32,
    _variant_count: u32,
    fields: &[CoreType],
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    if patterns.len() != fields.len() {
        return Err(format!(
            "error[native_ir.union_pattern_arity]: `{name}` expects {} fields",
            fields.len()
        ));
    }
    let semantic = managed_semantic(value_type)?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
            args: vec![value.clone()],
        },
        bindings: Vec::new(),
    }];
    for (index, (pattern, field)) in patterns.iter().zip(fields).enumerate() {
        let field_type = native_core_type(field)?;
        plans.push(pattern_plan(
            pattern,
            project(value.clone(), semantic, index, field_type)?,
            field_type,
            Some(field),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

fn tagged_union_constructor(
    name: &str,
    core_type: Option<&CoreType>,
) -> Option<(u32, u32, Vec<CoreType>)> {
    let CoreType::Union(variants) = core_type? else {
        return None;
    };
    let expected = match name.rsplit('.').next()? {
        "Err" => "error",
        other => return tagged_union_by_constructor_name(other, variants),
    };
    tagged_union_by_atom(expected, variants)
}

fn tagged_union_by_constructor_name(
    name: &str,
    variants: &[CoreType],
) -> Option<(u32, u32, Vec<CoreType>)> {
    let mut chars = name.chars();
    let expected = chars
        .next()?
        .to_lowercase()
        .chain(chars)
        .collect::<String>();
    tagged_union_by_atom(&expected, variants)
}

fn tagged_union_by_atom(
    expected: &str,
    variants: &[CoreType],
) -> Option<(u32, u32, Vec<CoreType>)> {
    variants.iter().enumerate().find_map(|(index, variant)| {
        let CoreType::Tuple(elements) = variant else {
            return None;
        };
        let (first, fields) = elements.split_first()?;
        let atom = match first {
            CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
            | CoreTupleTypeElem::Field {
                ty: CoreType::AtomLiteral(atom),
                ..
            } => atom,
            _ => return None,
        };
        if atom != expected {
            return None;
        }
        Some((
            u32::try_from(index).ok()?,
            u32::try_from(variants.len()).ok()?,
            fields.iter().map(tuple_element_type).cloned().collect(),
        ))
    })
}

fn option_constructor_plan(
    name: &str,
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    element: &CoreType,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let (discriminant, expected_arity) = match name {
        "None" => (0, 0),
        "Some" => (1, 1),
        _ => {
            return Err(format!(
                "error[native_ir.option_pattern_variant]: `{name}` is not an Option variant"
            ))
        }
    };
    if patterns.len() != expected_arity {
        return Err(format!(
            "error[native_ir.option_pattern_arity]: `{name}` expects {expected_arity} fields"
        ));
    }
    if name == "None" {
        let semantic = managed_semantic(value_type)?;
        let immediate = equality(
            value.clone(),
            NativeExpr::AtomLiteral(Arc::from("none")),
            NativeType::Atom,
        );
        return Ok(PatternPlan {
            predicate: bool_or(
                immediate.predicate,
                NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
                    args: vec![value],
                },
            ),
            bindings: Vec::new(),
        });
    }
    let semantic = managed_semantic(value_type)?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
            args: vec![value.clone()],
        },
        bindings: Vec::new(),
    }];
    if let [pattern] = patterns {
        let element_native = native_core_type(element)?;
        plans.push(pattern_plan(
            pattern,
            project(value, semantic, 0, element_native)?,
            element_native,
            Some(element),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

fn tuple_plan(
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    if let (Some(CoreType::Union(variants)), Some(CorePattern::Atom(tag))) =
        (core_type, patterns.first())
    {
        if let Some((discriminant, variant_count, fields)) = tagged_union_by_atom(tag, variants) {
            return tagged_union_constructor_plan(
                tag,
                &patterns[1..],
                value,
                value_type,
                discriminant,
                variant_count,
                &fields,
                constructors,
                depth,
            );
        }
    }
    let Some(CoreType::Tuple(elements)) = core_type else {
        return Err(format!(
            "error[native_ir.tuple_pattern_type]: tuple type is unavailable for {patterns:?}; found {core_type:?}"
        ));
    };
    if elements.len() != patterns.len() {
        return Err("error[native_ir.tuple_pattern_arity]: tuple arity differs".into());
    }
    let semantic = managed_semantic(value_type)?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_type_is_operation(semantic)),
            args: vec![value.clone()],
        },
        bindings: vec![],
    }];
    for (index, (pattern, element)) in patterns.iter().zip(elements).enumerate() {
        let element = tuple_element_type(element);
        let native = native_core_type(element)?;
        plans.push(pattern_plan(
            pattern,
            project(value.clone(), semantic, index, native)?,
            native,
            Some(element),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

fn list_plan(
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let element = list_element_type(core_type)?;
    let element_native = native_core_type(element)?;
    let semantic = managed_semantic(value_type)?;
    let mut current = value;
    let mut plans = Vec::with_capacity(patterns.len() * 2 + 1);
    for pattern in patterns {
        plans.push(nonempty(current.clone(), semantic));
        let first = NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_list_first_operation(
                semantic,
                element_native.is_managed_reference(),
            )),
            args: vec![current.clone()],
        };
        plans.push(pattern_plan(
            pattern,
            first,
            element_native,
            Some(element),
            constructors,
            depth + 1,
        )?);
        current = NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_list_rest_operation(semantic)),
            args: vec![current],
        };
    }
    plans.push(empty(current, semantic));
    merge(plans)
}

#[allow(clippy::too_many_arguments)]
fn list_cons_plan(
    head: &CorePattern,
    tail: &CorePattern,
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let element = list_element_type(core_type)?;
    let element_native = native_core_type(element)?;
    let semantic = managed_semantic(value_type)?;
    let first = NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_list_first_operation(
            semantic,
            element_native.is_managed_reference(),
        )),
        args: vec![value.clone()],
    };
    let rest = NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_list_rest_operation(semantic)),
        args: vec![value.clone()],
    };
    merge(vec![
        nonempty(value, semantic),
        pattern_plan(
            head,
            first,
            element_native,
            Some(element),
            constructors,
            depth + 1,
        )?,
        pattern_plan(tail, rest, value_type, core_type, constructors, depth + 1)?,
    ])
}

#[allow(clippy::too_many_arguments)]
fn structural_map_plan(
    patterns: &[crate::terlan_typeck::CoreMapPatternField],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let Some(CoreType::Map(fields)) = core_type else {
        return Err("error[native_ir.map_record_pattern_type]: shape is unavailable".into());
    };
    let semantic = managed_semantic(value_type)?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_type_is_operation(semantic)),
            args: vec![value.clone()],
        },
        bindings: Vec::new(),
    }];
    for pattern in patterns {
        let (index, field) = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.key == pattern.key)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.map_record_pattern_field]: `{}` is unavailable",
                    pattern.key
                )
            })?;
        let native = native_core_type(&field.value)?;
        plans.push(pattern_plan(
            &pattern.value,
            project(value.clone(), semantic, index, native)?,
            native,
            Some(&field.value),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

#[allow(clippy::too_many_arguments)]
fn map_plan(
    fields: &[crate::terlan_typeck::CoreMapPatternField],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let (key_type, field_type) = map_types(core_type)?;
    let key_native = native_core_type(key_type)?;
    let field_native = native_core_type(field_type)?;
    let semantic = managed_semantic(value_type)?;
    let mut plans = Vec::with_capacity(fields.len() * 2);
    for field in fields {
        let key = map_key(&field.key, key_type)?;
        plans.push(PatternPlan {
            predicate: NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_map_contains_operation(semantic)),
                args: vec![value.clone(), key.clone()],
            },
            bindings: vec![],
        });
        plans.push(pattern_plan(
            &field.value,
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_map_get_operation(
                    semantic,
                    field_native.is_managed_reference(),
                )),
                args: vec![value.clone(), key],
            },
            field_native,
            Some(field_type),
            constructors,
            depth + 1,
        )?);
    }
    if key_native == NativeType::Unit {
        return Err("error[native_ir.map_pattern_key]: Unit map keys are unavailable".into());
    }
    merge(plans)
}

fn project(
    value: NativeExpr,
    semantic: crate::runtime::native_image::managed::SemanticTypeId,
    index: usize,
    ty: NativeType,
) -> Result<NativeExpr, String> {
    let encoded = if ty.is_managed_reference() {
        encode_aggregate_field_operation(semantic, index)
    } else {
        encode_aggregate_scalar_field_operation(semantic, index)
    }
    .map_err(|error| format!("error[native_ir.pattern_projection]: {error}"))?;
    Ok(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![value],
    })
}

fn nonempty(
    value: NativeExpr,
    semantic: crate::runtime::native_image::managed::SemanticTypeId,
) -> PatternPlan {
    PatternPlan {
        predicate: NativeExpr::Not(Box::new(NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_list_is_empty_operation(semantic)),
            args: vec![value],
        })),
        bindings: vec![],
    }
}

fn empty(
    value: NativeExpr,
    semantic: crate::runtime::native_image::managed::SemanticTypeId,
) -> PatternPlan {
    PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_list_is_empty_operation(semantic)),
            args: vec![value],
        },
        bindings: vec![],
    }
}

fn merge(plans: Vec<PatternPlan>) -> Result<PatternPlan, String> {
    let mut predicate = NativeExpr::Bool(true);
    let mut bindings = Vec::new();
    for plan in plans {
        predicate = bool_and(predicate, plan.predicate);
        bindings.extend(plan.bindings);
    }
    if bindings.len() > MAX_STRUCTURED_PATTERN_BINDINGS {
        return Err(format!(
            "error[native_ir.structured_pattern_bindings]: pattern binds more than {MAX_STRUCTURED_PATTERN_BINDINGS} values"
        ));
    }
    Ok(PatternPlan {
        predicate,
        bindings,
    })
}

fn always() -> PatternPlan {
    PatternPlan {
        predicate: NativeExpr::Bool(true),
        bindings: vec![],
    }
}

fn equality(value: NativeExpr, expected: NativeExpr, ty: NativeType) -> PatternPlan {
    PatternPlan {
        predicate: NativeExpr::Binary {
            operator: NativeBinaryOperator::Equal,
            operand_type: ty,
            left: Box::new(value),
            right: Box::new(expected),
        },
        bindings: vec![],
    }
}

pub(super) fn bool_and(left: NativeExpr, right: NativeExpr) -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (left, right),
            (NativeExpr::Bool(true), NativeExpr::Bool(false)),
        ],
    }
}

fn bool_or(left: NativeExpr, right: NativeExpr) -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (left, NativeExpr::Bool(true)),
            (NativeExpr::Bool(true), right),
        ],
    }
}

pub(super) fn bind_values(bindings: &[PatternBinding], body: NativeExpr) -> NativeExpr {
    if bindings.is_empty() {
        body
    } else {
        NativeExpr::Let {
            bindings: bindings
                .iter()
                .map(|binding| binding.value.clone())
                .collect(),
            body: Box::new(body),
        }
    }
}

pub(super) fn extend_bindings(
    slots: &HashMap<String, usize>,
    types: &HashMap<String, NativeType>,
    start: usize,
    bindings: &[PatternBinding],
) -> (HashMap<String, usize>, HashMap<String, NativeType>) {
    let mut slots = slots.clone();
    let mut types = types.clone();
    for (index, binding) in bindings.iter().enumerate() {
        slots.insert(binding.name.clone(), start + index);
        types.insert(binding.name.clone(), binding.ty);
    }
    (slots, types)
}

pub(super) fn validate_bindings(bindings: &[PatternBinding]) -> Result<(), String> {
    let mut names = HashSet::new();
    if bindings.iter().all(|binding| names.insert(&binding.name)) {
        Ok(())
    } else {
        Err("error[native_ir.structured_pattern_binding]: duplicate binding name".to_string())
    }
}

fn constructor_layout<'a>(
    name: &str,
    identity: Option<&str>,
    arity: usize,
    layouts: &'a NativeConstructorLayouts,
) -> Result<&'a NativeConstructorLayout, String> {
    if let Some(identity) = identity {
        if let Some(layout) = layouts.get(&(identity.to_string(), arity)) {
            return Ok(layout);
        }
    }
    if let Some(layout) = layouts.get(&(name.to_string(), arity)) {
        return Ok(layout);
    }
    let mut matches = layouts.iter().filter(|((identity, candidate_arity), _)| {
        *candidate_arity == arity && identity.rsplit('.').next() == Some(name)
    });
    let (_, layout) = matches.next().ok_or_else(|| {
        format!("error[native_ir.constructor_pattern_identity]: `{name}/{arity}` is absent")
    })?;
    if matches.next().is_some() {
        return Err(format!(
            "error[native_ir.constructor_pattern_identity]: `{name}/{arity}` is ambiguous"
        ));
    }
    Ok(layout)
}

fn managed_semantic(
    ty: NativeType,
) -> Result<crate::runtime::native_image::managed::SemanticTypeId, String> {
    match ty {
        NativeType::ManagedRef(semantic) => Ok(semantic),
        _ => Err("error[native_ir.structured_pattern_type]: value is not managed".to_string()),
    }
}
