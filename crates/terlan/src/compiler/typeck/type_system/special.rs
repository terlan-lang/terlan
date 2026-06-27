use crate::terlan_typeck::Type;

/// Checks whether a type denotes Terlan's canonical Unit type.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for local `Unit` and fully-qualified `std.core.Unit.Unit`.
/// - `false` for all other named types and literal atoms.
///
/// Transformation:
/// - Recognizes only zero-argument Unit names so `Unit[T]` or unrelated
///   aliases do not become singleton-unit equivalents.
pub(crate) fn is_unit_named_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            module: None,
            name,
            args,
        } if name == "Unit" && args.is_empty()
    ) || matches!(
        ty,
        Type::Named {
            module: Some(module),
            name,
            args,
        } if module == "std.core.Unit" && name == "Unit" && args.is_empty()
    )
}

/// Checks whether a type denotes the canonical Unit singleton representation.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for the explicit `Atom["unit"]` literal type.
/// - `false` for all other atoms and named types.
///
/// Transformation:
/// - Keeps the equivalence at the type level; expression parsing still rejects
///   bare lowercase `unit` as a source-level Unit synonym.
pub(crate) fn is_unit_literal_type(ty: &Type) -> bool {
    matches!(ty, Type::LiteralAtom(atom) if atom == "unit")
}

/// Checks whether two types are equivalent Unit spellings.
///
/// Inputs:
/// - `left`: first resolved type.
/// - `right`: second resolved type.
///
/// Output:
/// - `true` when one side is named Unit and the other is `Atom["unit"]`.
/// - `false` for non-Unit atom aliases and unrelated named types.
///
/// Transformation:
/// - Bridges the public `std.core.Unit.Unit = Atom["unit"]` alias to the
///   compiler's singleton representation during type comparison only.
pub(crate) fn are_unit_equivalent_types(left: &Type, right: &Type) -> bool {
    (is_unit_named_type(left) && is_unit_literal_type(right))
        || (is_unit_literal_type(left) && is_unit_named_type(right))
}

/// Checks whether a type denotes the public template HTML facade.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for `Template.Html` and fully qualified
///   `std.template.Template.Html`.
/// - `false` for unrelated named types and parameterized template names.
///
/// Transformation:
/// - Recognizes only the zero-argument public std template type. This keeps
///   the facade narrow while allowing syntax-level HTML blocks to typecheck
///   against the source-visible std contract.
pub(crate) fn is_template_html_named_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            module: Some(module),
            name,
            args,
        } if module == "Template" && name == "Html" && args.is_empty()
    ) || matches!(
        ty,
        Type::Named {
            module: Some(module),
            name,
            args,
        } if module == "std.template.Template" && name == "Html" && args.is_empty()
    )
}

/// Checks whether a type denotes the internal syntax-level HTML value shape.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for local `Html[_]`.
/// - `false` for the public facade and all non-HTML types.
///
/// Transformation:
/// - Keeps syntax-produced HTML blocks distinct from the public facade while
///   letting comparison code bridge the two forms explicitly.
pub(crate) fn is_internal_html_value_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            module: None,
            name,
            args,
        } if name == "Html" && args.len() == 1
    )
}

/// Checks whether two HTML type spellings are equivalent for assignment.
///
/// Inputs:
/// - `left`: first resolved type.
/// - `right`: second resolved type.
///
/// Output:
/// - `true` when both sides are public template HTML spellings, or when one
///   side is public `Template.Html` and the other is internal `Html[_]`.
/// - `false` for all other combinations.
///
/// Transformation:
/// - Bridges shorthand and fully qualified public std template facade names,
///   then bridges that facade to the parser's HTML block value type during
///   type comparison only.
pub(crate) fn are_template_html_equivalent_types(left: &Type, right: &Type) -> bool {
    (is_template_html_named_type(left) && is_template_html_named_type(right))
        || (is_template_html_named_type(left) && is_internal_html_value_type(right))
        || (is_internal_html_value_type(left) && is_template_html_named_type(right))
}
