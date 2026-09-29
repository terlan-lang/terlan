use super::*;

#[test]
fn none_in_an_atom_union_keeps_its_scalar_representation() {
    let value = CoreExpr::Atom("none".into());
    let same_site = CoreType::Union(
        ["lax", "strict", "none"]
            .into_iter()
            .map(|name| CoreType::AtomLiteral(name.into()))
            .collect(),
    );
    assert!(!is_none_option_value(&value, &same_site));
    assert!(is_none_option_value(
        &value,
        &CoreType::Apply {
            constructor: "Option".into(),
            args: vec![same_site],
        }
    ));
}
