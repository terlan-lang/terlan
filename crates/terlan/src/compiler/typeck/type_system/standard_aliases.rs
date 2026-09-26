//! Structural compatibility adapters for compiler-owned standard aliases.

use crate::terlan_typeck::Type;

/// Checks one structural value against the standard implicit `Option` type.
///
/// Inputs:
/// - `lhs`: candidate structural value type.
/// - `rhs`: candidate standard `Option[T]` supertype.
/// - `payload_is_subtype`: recursive comparison for a `Some` payload and `T`.
///
/// Output:
/// - `Some(true)` when the pair is a compatible `None` or `Some` relation.
/// - `Some(false)` when a `Some` payload is incompatible with `T`.
/// - `None` when the pair is not a standard Option representation relation.
///
/// Transformation:
/// - Recognizes `none` and `{some, payload}` without exposing that runtime
///   representation to ordinary nominal named-type comparison.
pub(super) fn option_representation_is_subtype(
    lhs: &Type,
    rhs: &Type,
    payload_is_subtype: impl FnOnce(&Type, &Type) -> bool,
) -> Option<bool> {
    let inner = option_inner_type(rhs)?;
    match lhs {
        Type::LiteralAtom(atom) if atom == "none" => Some(true),
        Type::Tuple(items)
            if items.len() == 2
                && matches!(items.first(), Some(Type::LiteralAtom(tag)) if tag == "some") =>
        {
            Some(payload_is_subtype(&items[1], inner))
        }
        _ => None,
    }
}

/// Returns the payload type of the standard implicit `Option` application.
///
/// Inputs:
/// - `ty`: candidate named type.
///
/// Output:
/// - The sole `Option[T]` argument for unqualified or standard-library Option
///   names, otherwise `None`.
///
/// Transformation:
/// - Recognizes the compiler prelude spelling and its fully qualified provider
///   identity without treating unrelated module-local names as standard.
fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Named { module, name, args } = ty else {
        return None;
    };
    let is_standard = module
        .as_deref()
        .is_none_or(|module| module == "std.core.Option");
    (name == "Option" && is_standard && args.len() == 1).then(|| &args[0])
}
