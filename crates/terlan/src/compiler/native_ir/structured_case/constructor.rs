//! Constructor patterns across structural unions and nominal records.

use super::*;
use crate::runtime::native_image::managed::ManagedAggregateKind;

use super::super::constructors::NativeConstructorLayout;

pub(super) fn constructor_plan(
    pattern: ConstructorPattern<'_>,
    subject: PatternSubject<'_>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let ConstructorPattern {
        name,
        identity,
        fields: patterns,
    } = pattern;
    let PatternSubject {
        value,
        native_type: value_type,
        core_type,
    } = subject;
    let resolved_name = identity.unwrap_or(name);
    let source_name = resolved_name.rsplit('.').next().unwrap_or(resolved_name);
    if source_name == "Unit" && patterns.is_empty() {
        return Ok(equality(value, NativeExpr::Unit, NativeType::Unit));
    }
    if let Some(element) = option_element_type(core_type) {
        return option_constructor_plan(
            source_name,
            patterns,
            value,
            value_type,
            element,
            constructors,
            depth,
        );
    }
    if let Some((ok, error)) = result_element_types(core_type) {
        let (discriminant, field) = match source_name {
            "Ok" => (0, ok),
            "Err" => (1, error),
            _ => {
                return Err(format!(
                    "error[native_ir.result_pattern_variant]: `{name}` is not a Result variant"
                ))
            }
        };
        return tagged_union_constructor_plan(
            TaggedUnionPattern {
                name,
                fields: patterns,
                discriminant,
                field_types: std::slice::from_ref(field),
            },
            value,
            value_type,
            constructors,
            depth,
        );
    }
    if let Some((discriminant, _variant_count, fields)) =
        tagged_union_constructor(resolved_name, core_type)
    {
        return tagged_union_constructor_plan(
            TaggedUnionPattern {
                name,
                fields: patterns,
                discriminant,
                field_types: &fields,
            },
            value,
            value_type,
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
    let predicate = match (layout.descriptor.kind(), layout.descriptor.discriminant()) {
        (ManagedAggregateKind::Constructor, Some(discriminant)) => {
            encode_managed_variant_is_operation(semantic, discriminant)
        }
        (ManagedAggregateKind::Record, None) => encode_managed_type_is_operation(semantic),
        _ => {
            return Err(
                "error[native_ir.constructor_pattern_layout]: incompatible aggregate layout"
                    .to_string(),
            )
        }
    };
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(predicate),
            args: vec![value.clone()],
        },
        bindings: vec![],
    }];
    for (index, ((pattern, field_type), field_core_type)) in patterns
        .iter()
        .zip(&layout.parameters)
        .zip(&layout.parameter_core_types)
        .enumerate()
    {
        plans.push(pattern_plan(
            pattern,
            project(value.clone(), semantic, index, *field_type)?,
            *field_type,
            field_core_type.as_ref(),
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

pub(super) fn option_constructor_plan(
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
