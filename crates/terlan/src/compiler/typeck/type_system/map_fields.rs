use super::*;

/// Checks structural subtype compatibility for map fields.
///
/// Inputs:
/// - `lhs`: fields present on the candidate map type.
/// - `rhs`: fields required or allowed by the expected map type.
///
/// Output:
/// - `true` when all required expected fields are present with compatible types.
///
/// Transformation:
/// - Treats optional expected fields as skippable and rejects optional candidate
///   fields where the expected type requires the field.
pub(crate) fn map_fields_is_subtype(lhs: &[MapFieldType], rhs: &[MapFieldType]) -> bool {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return false;
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return false;
        }

        if !is_subtype(&lhs_field.value, &rhs_field.value) {
            return false;
        }
    }

    true
}

/// Checks map-field subtype compatibility with alias-aware value checks.
///
/// Inputs:
/// - `lhs`: candidate map fields.
/// - `rhs`: expected map fields.
/// - `aliases`: visible alias metadata.
/// - `depth`: current subtype recursion depth.
///
/// Output:
/// - `true` when all required expected fields are present and compatible.
///
/// Transformation:
/// - Mirrors `map_fields_is_subtype` while delegating field value comparison to
///   the alias-aware subtype relation.
pub(crate) fn map_fields_is_subtype_with_aliases(
    lhs: &[MapFieldType],
    rhs: &[MapFieldType],
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
) -> bool {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return false;
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return false;
        }

        if !is_subtype_with_aliases_inner(&lhs_field.value, &rhs_field.value, aliases, depth) {
            return false;
        }
    }

    true
}

/// Unifies two structural map field lists.
///
/// Inputs:
/// - `lhs`: candidate map fields.
/// - `rhs`: expected map fields.
/// - `subst`: mutable substitution table for field value types.
///
/// Output:
/// - `Ok(())` when required fields and value types can unify.
/// - `Err(message)` when a required field is missing or incompatible.
///
/// Transformation:
/// - Matches fields by key, enforces requiredness, and delegates value
///   compatibility to `unify`.
pub(crate) fn unify_map_fields(
    lhs: &[MapFieldType],
    rhs: &[MapFieldType],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return Err(format!("missing required map field: {}", rhs_field.key));
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return Err(format!(
                "required map field {} cannot match optional",
                rhs_field.key
            ));
        }

        unify(&lhs_field.value, &rhs_field.value, subst)?;
    }

    for lhs_field in lhs {
        if lhs_field.required {
            let present = rhs.iter().any(|rhs_field| rhs_field.key == lhs_field.key);
            if !present {
                return Err(format!("missing required map field: {}", lhs_field.key));
            }
        }
    }

    Ok(())
}
