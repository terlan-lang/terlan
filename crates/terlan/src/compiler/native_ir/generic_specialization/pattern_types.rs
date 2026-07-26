//! Lexical type binding for patterns inside monomorphized generic bodies.

use std::collections::HashMap;

use crate::terlan_typeck::{CorePattern, CoreTupleTypeElem, CoreType};

pub(super) fn bind_pattern_types(
    pattern: &CorePattern,
    ty: &CoreType,
    variables: &mut HashMap<String, CoreType>,
) {
    match pattern {
        CorePattern::Var(name) => {
            variables.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            variables.insert(alias.clone(), ty.clone());
            bind_pattern_types(pattern, ty, variables);
        }
        CorePattern::Constructor { name, args, .. } if name.rsplit('.').next() == Some("Some") => {
            if let (Some(element), [pattern]) = (applied_element(ty, "Option"), args.as_slice()) {
                bind_pattern_types(pattern, element, variables);
            }
        }
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => {
            let structural_variant = structural_tuple_variant(patterns, ty);
            let structural_ty = structural_variant.as_ref().unwrap_or(ty);
            if let CoreType::Tuple(elements) = structural_ty {
                for (pattern, element) in patterns.iter().zip(elements) {
                    let ty = match element {
                        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                    };
                    bind_pattern_types(pattern, ty, variables);
                }
            } else if let Some(element) = list_element_type(structural_ty) {
                for pattern in patterns {
                    bind_pattern_types(pattern, element, variables);
                }
            } else if let Some(fields) = iterator_step_fields(structural_ty) {
                for (pattern, field_type) in patterns.iter().zip(fields) {
                    bind_pattern_types(pattern, &field_type, variables);
                }
            }
        }
        CorePattern::ListCons { head, tail } => {
            if let Some(element) = list_element_type(ty) {
                bind_pattern_types(head, element, variables);
                bind_pattern_types(tail, ty, variables);
            }
        }
        CorePattern::Map(fields) => {
            for field in fields {
                if let Some(field_type) = named_field_type(ty, &field.key)
                    .cloned()
                    .or_else(|| iterator_step_field_type(ty, &field.key))
                {
                    bind_pattern_types(&field.value, &field_type, variables);
                }
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                if let Some(field_type) = named_field_type(ty, &field.key)
                    .cloned()
                    .or_else(|| iterator_step_field_type(ty, &field.key))
                {
                    bind_pattern_types(&field.value, &field_type, variables);
                }
            }
        }
        _ => {}
    }
}

fn structural_tuple_variant(patterns: &[CorePattern], ty: &CoreType) -> Option<CoreType> {
    let CorePattern::Atom(expected) = patterns.first()? else {
        return None;
    };
    if let CoreType::Apply { constructor, args } = ty {
        match (
            constructor.rsplit('.').next(),
            expected.as_str(),
            args.as_slice(),
        ) {
            (Some("Option"), "some", [value]) => {
                return Some(tagged_tuple("some", value.clone()));
            }
            (Some("Result"), "ok", [value, _]) => {
                return Some(tagged_tuple("ok", value.clone()));
            }
            (Some("Result"), "error", [_, reason]) => {
                return Some(tagged_tuple("error", reason.clone()));
            }
            _ => {}
        }
    }
    let CoreType::Union(variants) = ty else {
        return None;
    };
    variants
        .iter()
        .find(|variant| {
            let CoreType::Tuple(elements) = variant else {
                return false;
            };
            matches!(
                elements.first(),
                Some(
                    CoreTupleTypeElem::Type(CoreType::AtomLiteral(actual))
                        | CoreTupleTypeElem::Field {
                            ty: CoreType::AtomLiteral(actual),
                            ..
                        }
                ) if actual == expected
            )
        })
        .cloned()
}

fn tagged_tuple(atom: &str, value: CoreType) -> CoreType {
    CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom.to_string())),
        CoreTupleTypeElem::Type(value),
    ])
}

fn applied_element<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some(name) && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

fn list_element_type(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

fn named_field_type<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    match ty {
        CoreType::Tuple(elements) => elements.iter().find_map(|element| match element {
            CoreTupleTypeElem::Field { name: field, ty } if field == name => Some(ty),
            _ => None,
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.ty),
        CoreType::Map(fields) => fields
            .iter()
            .find(|field| field.key == name)
            .map(|field| &field.value),
        _ => None,
    }
}

fn iterator_step_fields(ty: &CoreType) -> Option<[CoreType; 2]> {
    Some([
        iterator_step_field_type(ty, "value")?,
        iterator_step_field_type(ty, "next")?,
    ])
}

fn iterator_step_field_type(ty: &CoreType, name: &str) -> Option<CoreType> {
    let CoreType::Apply { constructor, args } = ty else {
        return None;
    };
    if constructor.rsplit('.').next() != Some("Step") || args.len() != 1 {
        return None;
    }
    match name {
        "value" => Some(args[0].clone()),
        "next" => Some(CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![args[0].clone()],
        }),
        _ => None,
    }
}
