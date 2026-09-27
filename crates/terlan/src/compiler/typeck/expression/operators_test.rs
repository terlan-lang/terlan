use super::*;

/// A union member may appear on either side of equality, including nested values.
#[test]
fn equality_accepts_option_members_in_either_operand_order() {
    for expression in [
        "None == value",
        "value == None",
        "None != value",
        "value != None",
        "{None, 1} == {value, 1}",
        "{value, 1} == {None, 1}",
        "[None] == [value]",
        "[value] == [None]",
    ] {
        let source = format!("module option_equality. import std.core.Option. import std.core.Option.{{None}}. pub check(value: Option[String]): Bool -> {expression}.");
        let diagnostics =
            crate::terlan_typeck::test_support::check_syntax_output_with_std_interfaces(
                &source,
                "std/core/Option.terl",
            );
        assert!(diagnostics.is_empty(), "{expression}: {diagnostics:?}");
    }
}

/// Symmetric comparison does not make unrelated operand types compatible.
#[test]
fn equality_rejects_unrelated_operand_types_in_both_directions() {
    for (left, right) in [(Type::Bool, Type::Binary), (Type::Int, Type::Binary)] {
        for (left, right) in [(&left, &right), (&right, &left)] {
            assert!(
                unify_equality_types(left, right, &HashMap::new(), &mut HashMap::new()).is_err()
            );
        }
    }
}

/// Failed alternatives must not constrain later inference through partial bindings.
#[test]
fn equality_failure_preserves_existing_substitutions() {
    let left = Type::Tuple(vec![Type::Var(7), Type::Int]);
    let right = Type::Tuple(vec![Type::Binary, Type::Bool]);
    let mut subst = HashMap::from([(3, Type::Float)]);
    let before = subst.clone();
    assert!(unify_equality_types(&left, &right, &HashMap::new(), &mut subst).is_err());
    assert_eq!(subst, before);
}
