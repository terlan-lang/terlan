//! Lexical type binding for patterns inside monomorphized generic bodies.

use std::collections::HashMap;

use crate::terlan_typeck::{CorePattern, CoreTupleTypeElem, CoreType};

#[cfg(test)]
#[path = "pattern_types_test.rs"]
mod tests;

pub(super) fn bind_pattern_types(
    pattern: &CorePattern,
    ty: &CoreType,
    variables: &mut HashMap<String, CoreType>,
) {
    let ty = match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 =>
        {
            &args[0]
        }
        _ => ty,
    };
    match pattern {
        CorePattern::Var(name) => {
            variables.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            variables.insert(alias.clone(), ty.clone());
            bind_pattern_types(pattern, ty, variables);
        }
        CorePattern::Constructor { name, args, .. } => {
            if let Some(argument_types) = constructor_argument_types(ty, name) {
                for (pattern, argument_type) in args.iter().zip(argument_types) {
                    bind_pattern_types(pattern, argument_type, variables);
                }
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

/// Returns one payload from a two-parameter `Result` application.
fn applied_result_element(ty: &CoreType, index: usize) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            args.get(index)
        }
        _ => None,
    }
}

/// Resolves a unary constructor payload from generic or expanded alias types.
fn constructor_payload_type<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    let constructor = name.rsplit('.').next()?;
    let applied = match constructor {
        "Some" => applied_element(ty, "Option"),
        "Ok" => applied_result_element(ty, 0),
        "Err" => applied_result_element(ty, 1),
        _ => None,
    };
    applied.or_else(|| {
        let expected_tag = match constructor {
            "Some" => "some",
            "Ok" => "ok",
            "Err" => "error",
            _ => return None,
        };
        let CoreType::Union(variants) = ty else {
            return None;
        };
        variants.iter().find_map(|variant| {
            let CoreType::Tuple(elements) = variant else {
                return None;
            };
            let tag = tuple_element_type(elements.first()?)?;
            let payload = tuple_element_type(elements.get(1)?)?;
            matches!(tag, CoreType::AtomLiteral(actual) if actual == expected_tag)
                .then_some(payload)
        })
    })
}

/// Resolves all payload fields carried by one tagged constructor pattern.
fn constructor_argument_types<'a>(ty: &'a CoreType, name: &str) -> Option<Vec<&'a CoreType>> {
    if let Some(payload) = constructor_payload_type(ty, name) {
        return Some(vec![payload]);
    }
    let expected_tag = constructor_tag(name.rsplit('.').next()?);
    let variants = match ty {
        CoreType::Union(variants) => variants.as_slice(),
        other => std::slice::from_ref(other),
    };
    variants.iter().find_map(|variant| {
        let CoreType::Tuple(elements) = variant else {
            return None;
        };
        let tag = tuple_element_type(elements.first()?)?;
        if !matches!(tag, CoreType::AtomLiteral(actual) if actual == &expected_tag) {
            return None;
        }
        elements
            .iter()
            .skip(1)
            .map(tuple_element_type)
            .collect::<Option<Vec<_>>>()
    })
}

fn constructor_tag(name: &str) -> String {
    let mut tag = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                tag.push('_');
            }
            tag.extend(character.to_lowercase());
        } else {
            tag.push(character);
        }
    }
    tag
}

/// Returns the type carried by one positional or named tuple element.
fn tuple_element_type(element: &CoreTupleTypeElem) -> Option<&CoreType> {
    Some(match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    })
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
