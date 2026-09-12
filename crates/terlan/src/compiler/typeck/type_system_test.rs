//! Portable standard-library lists share the intrinsic list contract, not user nominals.

use super::*;

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
