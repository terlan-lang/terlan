#[cfg(test)]
mod tests {
    use crate::terlan_syntax::parse_module;
    use crate::terlan_syntax::parse_tree::Decl;

    /// Verifies bodyless source type aliases promote to singleton atom types.
    ///
    /// Inputs:
    /// - Non-opaque source aliases without explicit `=` bodies.
    ///
    /// Output:
    /// - Test passes when the parser stores canonical `Atom["..."]` bodies.
    ///
    /// Transformation:
    /// - Keeps source shorthand such as `pub type Hit.` semantic rather than
    ///   treating it as an interface-only declaration header.
    #[test]
    fn source_bodyless_type_aliases_promote_to_atom_literals() {
        let module = parse_module(
            r#"
module type_shorthand.

pub type Hit.
pub type InvalidMove.
pub type HTTPError.
pub type Clockwise90.
pub type Rotate180.
"#,
        )
        .expect("parse atom shorthand types");

        let variants = module
            .declarations
            .iter()
            .map(|declaration| match declaration {
                Decl::Type(type_decl) => {
                    assert_eq!(type_decl.variants.len(), 1);
                    type_decl.variants[0].text.as_str()
                }
                _ => panic!("expected type declaration"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            variants,
            vec![
                r#"Atom["hit"]"#,
                r#"Atom["invalid_move"]"#,
                r#"Atom["http_error"]"#,
                r#"Atom["clockwise_90"]"#,
                r#"Atom["rotate_180"]"#,
            ]
        );
    }

    /// Verifies generic aliases cannot use singleton atom shorthand.
    ///
    /// Inputs:
    /// - A bodyless generic source alias.
    ///
    /// Output:
    /// - Stable parser rejection requiring an explicit alias body.
    ///
    /// Transformation:
    /// - Prevents `type Box[T].` from discarding its parameter by silently
    ///   becoming the unrelated singleton `Atom["box"]` type.
    #[test]
    fn source_bodyless_generic_type_aliases_require_explicit_bodies() {
        let error = parse_module(
            r#"
module invalid_generic_type_shorthand.

pub type Box[T].
"#,
        )
        .expect_err("generic atom shorthand must be rejected");

        assert_eq!(error.message, "expected `=` in type declaration");
    }

    /// Verifies compact record fields are not mistaken for removed raw atoms.
    #[test]
    fn record_type_field_colons_remain_valid() {
        parse_module(
            r#"
module record_field_colons.

pub type Component = {
    template: std.js.String.JsString,
    nested: Map[String, {value: Int}]
}.
"#,
        )
        .expect("record field colons should parse as type separators");
    }

    /// Verifies legacy atom types canonicalize without confusing field colons.
    #[test]
    fn raw_atom_in_record_type_canonicalizes() {
        let module = parse_module(
            r#"
module raw_atom_record.

pub type Effect = {:effect, value: Int}.
"#,
        )
        .expect("raw atom type compatibility alias should parse");
        let Decl::Type(effect) = &module.declarations[0] else {
            panic!("expected type declaration");
        };
        assert_eq!(effect.variants[0].text, "{Atom[\"effect\"], value: Int}");
    }
}
