//! Type compatibility preserves portable representations and return constraints.

use super::*;

/// Every inferred alternative must fit, irrespective of branch order.
#[test]
fn return_union_checks_every_variant_instead_of_any_overlap() {
    for variants in [vec![Type::Int, Type::Bool], vec![Type::Bool, Type::Int]] {
        let actual = Type::Union(variants);
        assert!(unify_return_type(&Type::Int, &actual, &mut HashMap::new()).is_err());
        let expected = Type::Union(vec![Type::Binary, Type::Bool, Type::Int]);
        assert!(unify_return_type(&expected, &actual, &mut HashMap::new()).is_ok());
        assert!(unify_return_type(&Type::Term, &actual, &mut HashMap::new()).is_ok());
    }
}

/// An inferred return variable captures the complete union, including through substitution.
#[test]
fn return_union_preserves_generic_inference_and_checks_bound_variables() {
    let actual = Type::Union(vec![Type::Int, Type::Bool]);
    let mut subst = HashMap::new();
    assert!(unify_return_type(&Type::Var(0), &actual, &mut subst).is_ok());
    assert_eq!(subst.get(&0), Some(&actual));
    assert!(unify_return_type(&Type::Int, &Type::Var(0), &mut subst).is_err());
    assert_eq!(subst.get(&0), Some(&actual));
}

/// A rejected alternative cannot leak earlier successful generic constraints.
#[test]
fn return_union_failure_does_not_commit_partial_substitutions() {
    let expected = Type::Tuple(vec![Type::Var(0), Type::Int]);
    let actual = Type::Union(vec![
        Type::Tuple(vec![Type::Bool, Type::Int]),
        Type::Tuple(vec![Type::Bool, Type::Bool]),
    ]);
    let mut subst = HashMap::new();
    assert!(unify_return_type(&expected, &actual, &mut subst).is_err());
    assert!(subst.is_empty());
}

fn portable(module: Option<&str>, args: Vec<Type>) -> Type {
    Type::Named {
        module: module.map(str::to_owned),
        name: "List".to_owned(),
        args,
    }
}

/// Standard-list elements still participate in ordinary generic inference.
#[test]
fn standard_list_boundary_unifies_elements_in_both_directions() {
    let literal = Type::List(Box::new(Type::Var(0)));
    let native = portable(Some("std.collections.List"), vec![Type::Binary]);
    for (left, right) in [(&literal, &native), (&native, &literal)] {
        let mut substitution = HashMap::new();
        assert!(unify(left, right, &mut substitution).is_ok());
        assert_eq!(substitution.get(&0), Some(&Type::Binary));
    }
}

/// Bridging representation does not weaken element-type or constructor identity checks.
#[test]
fn standard_list_boundary_rejects_wrong_elements_namespaces_and_arity() {
    let literal = Type::List(Box::new(Type::Int));
    for native in [
        portable(Some("std.collections.List"), vec![Type::Bool]),
        portable(Some("user.collections.List"), vec![Type::Int]),
        portable(None, vec![Type::Int]),
        portable(Some("std.collections.List"), vec![]),
        portable(Some("std.collections.List"), vec![Type::Int, Type::Int]),
    ] {
        assert!(unify(&literal, &native, &mut HashMap::new()).is_err());
        assert!(unify(&native, &literal, &mut HashMap::new()).is_err());
    }
}
