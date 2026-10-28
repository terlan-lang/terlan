use super::*;

fn option(payload: Type) -> Type {
    Type::Union(vec![
        Type::Tuple(vec![Type::LiteralAtom("some".into()), payload]),
        Type::LiteralAtom("none".into()),
    ])
}

#[test]
fn substitutions_flatten_union_members_without_reordering_layouts() {
    let input = Type::Union(vec![Type::Var(1), Type::Binary]);
    let subst = HashMap::from([
        (1, Type::Union(vec![Type::Int, Type::Var(2)])),
        (2, Type::Union(vec![Type::Float, Type::Atom])),
    ]);
    let expected = Type::Union(vec![Type::Int, Type::Float, Type::Atom, Type::Binary]);
    let applied = apply_subst(&input, &subst);
    assert_eq!(applied, expected);
    assert_eq!(apply_subst(&applied, &subst), expected);
}

#[test]
fn fresh_union_payloads_unify_with_enclosing_context_and_reject_extra_members() {
    let actual = Type::Union(vec![Type::Var(1), option(Type::Var(0))]);
    let mut subst = HashMap::from([(1, option(Type::Var(22)))]);
    unify(&option(Type::Var(0)), &actual, &mut subst).expect("close fresh payload variables");
    assert_eq!(
        apply_subst(&Type::Var(0), &subst),
        apply_subst(&Type::Var(22), &subst)
    );
    let invalid = Type::Union(vec![Type::Var(1), Type::LiteralAtom("unrelated".into())]);
    assert!(unify(
        &option(Type::Int),
        &invalid,
        &mut HashMap::from([(1, option(Type::Int))])
    )
    .is_err());
}
