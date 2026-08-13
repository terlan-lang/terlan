use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

use super::FunctionTypes;

pub(super) fn is_std_list_constructor(constructor: &str, identity: Option<&str>) -> bool {
    constructor == "std.collections.List.List"
        || identity.is_some_and(|identity| {
            identity == "std.collections.List.List"
                || identity.starts_with("std.collections.List.List/")
        })
}

pub(super) fn is_std_set_constructor(constructor: &str, identity: Option<&str>) -> bool {
    constructor == "std.collections.Set.Set"
        || identity.is_some_and(|identity| {
            identity == "std.collections.Set.Set"
                || identity.starts_with("std.collections.Set.Set/")
        })
}

pub(super) fn is_std_map_constructor(constructor: &str, identity: Option<&str>) -> bool {
    matches!(
        constructor,
        "std.collections.Map" | "std.collections.Map.Map"
    ) || identity.is_some_and(|identity| {
        identity == "std.collections.Map"
            || identity.starts_with("std.collections.Map/")
            || identity == "std.collections.Map.Map"
            || identity.starts_with("std.collections.Map.Map/")
    })
}

pub(super) fn option_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        CoreType::Union(variants)
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

pub(super) fn tuple_elements(ty: &CoreType) -> Option<(&CoreType, &CoreType)> {
    let CoreType::Tuple(elements) = ty else {
        return None;
    };
    let [left, right] = elements.as_slice() else {
        return None;
    };
    Some((tuple_element_type(left), tuple_element_type(right)))
}

pub(super) fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

pub(super) fn named_field_type<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
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

pub(super) fn nominal_type_key(name: &str) -> String {
    format!("$type:{name}")
}

pub(super) fn nominal_type<'a>(
    functions: &'a FunctionTypes,
    module: &str,
    ty: &CoreType,
) -> Option<&'a CoreType> {
    let CoreType::Named(name) = ty else {
        return None;
    };
    let (owner, local) = name.rsplit_once('.').unwrap_or((module, name));
    functions
        .get(&(owner.to_string(), nominal_type_key(local), 0))
        .map(|signature| &signature.result)
}

pub(super) fn named_field_type_with_nominals<'a>(
    ty: &'a CoreType,
    name: &str,
    functions: &'a FunctionTypes,
    module: &str,
) -> Option<&'a CoreType> {
    named_field_type(ty, name).or_else(|| {
        nominal_type(functions, module, ty).and_then(|resolved| named_field_type(resolved, name))
    })
}

pub(super) fn map_elements(ty: &CoreType) -> Option<(&CoreType, &CoreType)> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

pub(super) fn set_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Set") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

pub(super) fn is_dynamic_type(ty: &CoreType) -> bool {
    matches!(ty, CoreType::Dynamic) || matches!(ty, CoreType::Named(name) if name == "Dynamic")
}

pub(super) fn option(element: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: "Option".to_string(),
        args: vec![element],
    }
}
