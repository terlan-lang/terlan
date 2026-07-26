use crate::terlan_typeck::CoreExpr;

fn and(left: CoreExpr, right: CoreExpr) -> CoreExpr {
    CoreExpr::BinaryOp {
        operator: "and".to_string(),
        left: Box::new(left),
        right: Box::new(right),
    }
}

#[test]
fn homogeneous_left_chain_becomes_one_ordered_right_spine() {
    let mut expression = and(
        and(CoreExpr::Var("a".into()), CoreExpr::Var("b".into())),
        CoreExpr::Var("c".into()),
    );
    super::short_circuit_normalization::normalize(&mut expression);
    assert_eq!(
        expression,
        and(
            CoreExpr::Var("a".into()),
            and(CoreExpr::Var("b".into()), CoreExpr::Var("c".into()))
        )
    );
}
