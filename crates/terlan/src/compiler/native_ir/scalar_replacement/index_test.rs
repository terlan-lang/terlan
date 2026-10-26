use super::*;

#[test]
fn index_proof_rejects_other_shapes_and_unbounded_expressions() {
    let proof = FixedIndex::prove(&CoreExpr::Int(1), 2).unwrap();
    assert_eq!(proof.select(&[10, 20]), Some(&20));
    assert_eq!(proof.select(&[10, 20, 30]), None);
    assert!(FixedIndex::prove(&CoreExpr::Int(-1), 2).is_none());
    assert!(FixedIndex::prove(&CoreExpr::Int(2), 2).is_none());
    assert!(FixedIndex::prove(&CoreExpr::Var("index".into()), 2).is_none());
    let mut expression = CoreExpr::Int(1);
    for _ in 0..64 {
        expression = CoreExpr::UnaryOp {
            operator: "-".into(),
            operand: Box::new(expression),
        };
    }
    assert!(FixedIndex::prove(&expression, 2).is_none());
}
