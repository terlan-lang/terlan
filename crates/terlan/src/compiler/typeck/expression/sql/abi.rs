use crate::terlan_typeck::Type;

pub(super) const SQL_SCALAR_ABI_TYPE_SUMMARY: &str = "Int, Float, Number, Binary, or Bool";
pub(super) const SQL_ROW_DECODE_ABI_TYPE_SUMMARY: &str =
    "Int, Binary, Bool, std.data.Json.Json, or Option of one of these";

/// Returns whether a type can cross the current SQL scalar parameter ABI.
pub(super) fn sql_scalar_abi_type_is_supported(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int | Type::LiteralInt(_) | Type::Float | Type::Number | Type::Binary | Type::Bool
    )
}

/// Returns whether a SQL row field has a scalar or nullable-scalar decoder.
pub(super) fn sql_row_decode_type_is_supported(ty: &Type) -> bool {
    sql_row_scalar_type_is_supported(ty)
        || structural_option_inner(ty).is_some_and(sql_row_scalar_type_is_supported)
}

fn sql_row_scalar_type_is_supported(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int | Type::LiteralInt(_) | Type::Binary | Type::Bool
    )
}

/// Extracts the payload from the normalized structural form of `Option[T]`.
pub(super) fn structural_option_inner(ty: &Type) -> Option<&Type> {
    let Type::Union(variants) = ty else {
        return None;
    };
    if variants.len() != 2
        || !variants
            .iter()
            .any(|variant| matches!(variant, Type::LiteralAtom(name) if name == "none"))
    {
        return None;
    }

    variants.iter().find_map(|variant| match variant {
        Type::Tuple(items)
            if items.len() == 2
                && matches!(&items[0], Type::LiteralAtom(name) if name == "some") =>
        {
            items.get(1)
        }
        _ => None,
    })
}
