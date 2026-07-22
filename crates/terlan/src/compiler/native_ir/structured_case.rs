//! Direct NativeIR lowering for checked structured case patterns.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_scalar_field_operation,
    encode_binary_pattern_extract_operation, encode_binary_pattern_matches_operation,
    encode_list_first_operation, encode_list_is_empty_operation, encode_list_rest_operation,
    encode_managed_type_is_operation, encode_managed_variant_is_operation,
    encode_map_contains_operation, encode_map_get_operation, encode_string_equal_operation,
    ManagedBinaryPatternEndian, ManagedBinaryPatternField,
};
use crate::terlan_typeck::{
    CoreBinaryPatternDescriptor, CoreBinaryPatternEndian, CoreBinaryPatternField, CoreExpr,
    CoreFunction, CorePattern, CoreTupleTypeElem, CoreType,
};

use super::constructors::{managed_field_projection, NativeConstructorLayout};
use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeBinaryOperator, NativeConstructorLayouts, NativeExpr, NativeType,
};

const MAX_STRUCTURED_CASE_CLAUSES: usize = 256;
const MAX_STRUCTURED_PATTERN_DEPTH: usize = 64;
const MAX_STRUCTURED_PATTERN_BINDINGS: usize = 128;

#[derive(Clone)]
pub(super) struct PatternBinding {
    pub(super) name: String,
    pub(super) value: NativeExpr,
    pub(super) ty: NativeType,
}

pub(super) struct PatternPlan {
    pub(super) predicate: NativeExpr,
    pub(super) bindings: Vec<PatternBinding>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_structured_case(
    body: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::Case { scrutinee, clauses } = body else {
        return Ok(None);
    };
    if clauses.is_empty() || clauses.len() > MAX_STRUCTURED_CASE_CLAUSES {
        return Err(format!(
            "error[native_ir.structured_case_clauses]: structured case has {} clauses; limit is {MAX_STRUCTURED_CASE_CLAUSES}",
            clauses.len()
        ));
    }
    let scrutinee_type =
        infer_native_type_with_constructors(scrutinee, param_types, function_types, constructors)
            .ok_or_else(|| {
            "error[native_ir.structured_case_type]: unknown scrutinee type".to_string()
        })?;
    let core_types = function
        .params
        .iter()
        .filter_map(|parameter| {
            parameter
                .core_ty
                .as_ref()
                .map(|ty| (parameter.name.clone(), ty.clone()))
        })
        .collect::<HashMap<_, _>>();
    let scrutinee_core = core_expr_type(scrutinee, &core_types);
    let scrutinee = lower_expr_with_constructors(
        scrutinee,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    let scrutinee_slot = params
        .values()
        .copied()
        .max()
        .map_or(0, |slot| slot.saturating_add(1));
    let mut outer_slots = params.clone();
    let mut outer_types = param_types.clone();
    let scrutinee_name = "$native_structured_case_scrutinee".to_string();
    outer_slots.insert(scrutinee_name, scrutinee_slot);
    outer_types.insert(
        "$native_structured_case_scrutinee".to_string(),
        scrutinee_type,
    );
    let scrutinee_value = NativeExpr::Param(scrutinee_slot);

    let mut native_clauses = Vec::with_capacity(clauses.len());
    for clause in clauses {
        let plan = pattern_plan(
            &clause.pattern,
            scrutinee_value.clone(),
            scrutinee_type,
            scrutinee_core.as_ref(),
            constructors,
            0,
        )?;
        validate_bindings(&plan.bindings)?;
        let (binding_slots, binding_types) = extend_bindings(
            &outer_slots,
            &outer_types,
            scrutinee_slot.saturating_add(1),
            &plan.bindings,
        );
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                lower_expr_with_constructors(
                    guard,
                    &binding_slots,
                    &binding_types,
                    functions,
                    function_types,
                    constructors,
                )
            })
            .transpose()?
            .unwrap_or(NativeExpr::Bool(true));
        let guard = bind_values(&plan.bindings, guard);
        let condition = bool_and(plan.predicate, guard);
        let selected = lower_expr_with_constructors(
            &clause.body,
            &binding_slots,
            &binding_types,
            functions,
            function_types,
            constructors,
        )?;
        native_clauses.push((condition, bind_values(&plan.bindings, selected)));
    }
    Ok(Some(NativeExpr::Let {
        bindings: vec![scrutinee],
        body: Box::new(NativeExpr::If {
            clauses: native_clauses,
        }),
    }))
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
        CorePattern::Var(name) => Ok(PatternPlan {
            predicate: NativeExpr::Bool(true),
            bindings: vec![PatternBinding {
                name: name.clone(),
                value,
                ty: value_type,
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
        CorePattern::Map(fields) => map_plan(
            fields,
            value,
            value_type,
            core_type,
            constructors,
            depth,
        ),
        CorePattern::BinaryLayout { endian, fields } => {
            binary_plan(*endian, fields, value, value_type)
        }
        CorePattern::StringPattern(_) | CorePattern::Atom(_) => {
            Err("error[native_ir.structured_pattern_family]: pattern family needs a dedicated bounded matcher".to_string())
        }
    }
}

fn binary_plan(
    endian: CoreBinaryPatternEndian,
    fields: &[CoreBinaryPatternField],
    value: NativeExpr,
    value_type: NativeType,
) -> Result<PatternPlan, String> {
    if value_type != NativeType::BinaryRef {
        return Err("error[native_ir.binary_pattern_type]: pattern requires Binary".to_string());
    }
    let endian = match endian {
        CoreBinaryPatternEndian::Big => ManagedBinaryPatternEndian::Big,
        CoreBinaryPatternEndian::Little => ManagedBinaryPatternEndian::Little,
    };
    let descriptors = fields
        .iter()
        .map(|field| managed_binary_field(field.descriptor))
        .collect::<Vec<_>>();
    let predicate = encode_binary_pattern_matches_operation(endian, &descriptors)
        .map_err(|error| format!("error[native_ir.binary_pattern_layout]: {error}"))?;
    let bindings = fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.name != "_")
        .map(|(index, field)| {
            let encoded = encode_binary_pattern_extract_operation(endian, &descriptors, index)
                .map_err(|error| format!("error[native_ir.binary_pattern_layout]: {error}"))?;
            Ok(PatternBinding {
                name: field.name.clone(),
                value: NativeExpr::ManagedOperation {
                    encoded: Arc::from(encoded),
                    args: vec![value.clone()],
                },
                ty: binary_field_type(field.descriptor),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(predicate),
            args: vec![value],
        },
        bindings,
    })
}

fn managed_binary_field(field: CoreBinaryPatternDescriptor) -> ManagedBinaryPatternField {
    match field {
        CoreBinaryPatternDescriptor::UInt(width) => ManagedBinaryPatternField::UInt(width),
        CoreBinaryPatternDescriptor::IntBits(width) => ManagedBinaryPatternField::Int(width),
        CoreBinaryPatternDescriptor::Bytes(width) => ManagedBinaryPatternField::Bytes(width),
        CoreBinaryPatternDescriptor::Bits(width) => ManagedBinaryPatternField::Bits(width),
        CoreBinaryPatternDescriptor::Utf8 => ManagedBinaryPatternField::Utf8,
        CoreBinaryPatternDescriptor::Utf16 => ManagedBinaryPatternField::Utf16,
        CoreBinaryPatternDescriptor::Utf32 => ManagedBinaryPatternField::Utf32,
        CoreBinaryPatternDescriptor::Rest => ManagedBinaryPatternField::Rest,
    }
}

fn binary_field_type(field: CoreBinaryPatternDescriptor) -> NativeType {
    match field {
        CoreBinaryPatternDescriptor::Bytes(_) | CoreBinaryPatternDescriptor::Rest => {
            NativeType::BytesRef
        }
        CoreBinaryPatternDescriptor::Bits(_) => NativeType::BinaryRef,
        CoreBinaryPatternDescriptor::UInt(_)
        | CoreBinaryPatternDescriptor::IntBits(_)
        | CoreBinaryPatternDescriptor::Utf8
        | CoreBinaryPatternDescriptor::Utf16
        | CoreBinaryPatternDescriptor::Utf32 => NativeType::Int,
    }
}

#[allow(clippy::too_many_arguments)]
fn constructor_plan(
    name: &str,
    identity: Option<&str>,
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    if name == "Unit" && patterns.is_empty() {
        return Ok(equality(value, NativeExpr::Unit, NativeType::Unit));
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

fn tuple_plan(
    patterns: &[CorePattern],
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let Some(CoreType::Tuple(elements)) = core_type else {
        return Err("error[native_ir.tuple_pattern_type]: tuple type is unavailable".into());
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

fn native_core_type(ty: &CoreType) -> Result<NativeType, String> {
    native_type(Some(ty), &ty.contract_text()).ok_or_else(|| {
        format!(
            "error[native_ir.structured_pattern_type]: unsupported `{}`",
            ty.contract_text()
        )
    })
}

fn core_expr_type(expr: &CoreExpr, types: &HashMap<String, CoreType>) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) => types.get(name).cloned(),
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        _ => None,
    }
}

fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

fn list_element_type(core_type: Option<&CoreType>) -> Result<&CoreType, String> {
    match core_type {
        Some(CoreType::List(element)) => Ok(element),
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err("error[native_ir.list_pattern_type]: concrete List type is unavailable".into()),
    }
}

fn map_types(core_type: Option<&CoreType>) -> Result<(&CoreType, &CoreType), String> {
    match core_type {
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Ok((&args[0], &args[1]))
        }
        _ => Err("error[native_ir.map_pattern_type]: concrete Map type is unavailable".into()),
    }
}

fn struct_field_type<'a>(core_type: Option<&'a CoreType>, name: &str) -> Option<&'a CoreType> {
    let Some(CoreType::Struct { fields, .. }) = core_type else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.ty)
}

fn map_key(key: &str, ty: &CoreType) -> Result<NativeExpr, String> {
    match ty {
        CoreType::String => {
            let encoded = crate::runtime::native_image::managed::encode_string_literal(key)
                .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}"))?;
            Ok(NativeExpr::StringLiteral {
                encoded: encoded.into(),
            })
        }
        CoreType::Int => key
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}")),
        _ => Err(format!(
            "error[native_ir.map_pattern_key]: unsupported key type `{}`",
            ty.contract_text()
        )),
    }
}
