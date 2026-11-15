//! Structural and nominal receiver-type compatibility.

use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

/// Compares receiver types after nominal qualification and opaque-type
/// expansion have exposed equivalent short and structural names.
pub(in crate::compiler::native_ir::application) fn receiver_types_match(
    expected: &CoreType,
    actual: &CoreType,
) -> bool {
    if expected == actual {
        return true;
    }
    if let (
        CoreType::Apply {
            constructor: expected_constructor,
            args: expected_args,
        },
        CoreType::Apply {
            constructor: actual_constructor,
            args: actual_args,
        },
    ) = (expected, actual)
    {
        return expected_args.len() == actual_args.len()
            && receiver_nominal_names_match(expected_constructor, actual_constructor)
            && expected_args
                .iter()
                .zip(actual_args)
                .all(|(expected, actual)| receiver_types_match(expected, actual));
    }
    if let CoreType::Union(variants) = expected {
        return variants
            .iter()
            .any(|variant| receiver_types_match(variant, actual));
    }
    if let (CoreType::Tuple(expected), CoreType::Tuple(actual)) = (expected, actual) {
        return expected.len() == actual.len()
            && expected.iter().zip(actual).all(|(expected, actual)| {
                receiver_types_match(tuple_element_type(expected), tuple_element_type(actual))
            });
    }
    let (Some(expected), Some(actual)) = (nominal_name(expected), nominal_name(actual)) else {
        return false;
    };
    receiver_nominal_names_match(expected, actual)
}

fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

fn receiver_nominal_names_match(expected: &str, actual: &str) -> bool {
    let expected = canonical_receiver_name(expected);
    let actual = canonical_receiver_name(actual);
    if expected == actual {
        return true;
    }
    // `Map` and `Set` are the canonical ABI spellings for Terlan's built-in
    // collections. They are identities, not unresolved short names. Treating
    // them as leaf-name aliases makes an imported type such as
    // `std.js.Map.Map[K, V]` collide with `std.collections.Map[K, V]` during
    // receiver dispatch.
    if matches!(expected, "Map" | "Set") || matches!(actual, "Map" | "Set") {
        return false;
    }
    (!expected.contains('.') || !actual.contains('.'))
        && expected.rsplit('.').next() == actual.rsplit('.').next()
}

fn canonical_receiver_name(name: &str) -> &str {
    let Some((owner, leaf)) = name.rsplit_once('.') else {
        return name;
    };
    if owner.rsplit('.').next() == Some(leaf) {
        owner
    } else {
        name
    }
}

/// Returns the declared name carried by a nominal or expanded structural type.
fn nominal_name(ty: &CoreType) -> Option<&str> {
    match ty {
        CoreType::Named(name) | CoreType::Struct { name, .. } => Some(name.as_str()),
        _ => None,
    }
}
