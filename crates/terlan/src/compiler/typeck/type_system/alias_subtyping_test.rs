use super::*;

fn named(name: &str, args: Vec<Type>) -> Type {
    Type::Named {
        module: None,
        name: name.into(),
        args,
    }
}

fn alias(body: Type) -> TypeAlias {
    TypeAlias {
        params: vec![],
        param_variance: vec![],
        bounds: vec![],
        body,
        constructor_param_names: vec![],
        is_opaque: false,
    }
}

#[test]
fn recursive_keys_preserve_equivalence_and_reject_incompatible_members() {
    let key = named("Key", vec![]);
    let array_key = named("Array", vec![key.clone()]);
    let body = Type::Union(vec![Type::Int, Type::Binary, array_key.clone()]);
    let mut array = alias(named("Array", vec![Type::Var(0)]));
    array.params = vec![0];
    array.param_variance = vec![Variance::Invariant];
    array.is_opaque = true;
    let aliases = HashMap::from([("Key".into(), alias(body.clone())), ("Array".into(), array)]);

    assert!(is_subtype_with_aliases(&key, &body, &aliases));
    assert!(is_subtype_with_aliases(&body, &key, &aliases));
    assert!(!is_subtype_with_aliases(
        &named("Array", vec![Type::Float]),
        &key,
        &aliases
    ));
    assert!(!is_subtype_with_aliases(&key, &array_key, &aliases));
}

#[test]
fn memoized_failures_respect_the_remaining_alias_expansion_depth() {
    let ty = named("Alias", vec![]);
    let aliases = HashMap::from([("Alias".into(), alias(Type::Int))]);
    let mut memo = HashMap::new();
    assert!(!is_subtype_with_aliases_inner(
        &ty,
        &Type::Int,
        &aliases,
        8,
        &mut memo
    ));
    assert!(is_subtype_with_aliases_inner(
        &ty,
        &Type::Int,
        &aliases,
        0,
        &mut memo
    ));
}

#[test]
fn comparison_cache_does_not_leak_between_alias_environments() {
    let ty = named("Alias", vec![]);
    let numbers = HashMap::from([("Alias".into(), alias(Type::Int))]);
    let strings = HashMap::from([("Alias".into(), alias(Type::Binary))]);
    assert!(is_subtype_with_aliases(&ty, &Type::Int, &numbers));
    assert!(!is_subtype_with_aliases(&ty, &Type::Int, &strings));
}
