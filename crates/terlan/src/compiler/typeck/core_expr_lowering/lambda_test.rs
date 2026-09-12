use super::*;

#[test]
fn typed_lambda_core_contracts_distinguish_parameter_types() {
    let lower_type = |ty: &str| {
        let syntax =
            crate::terlan_syntax::parse_expr_as_syntax_output(&format!("(value: {ty}) -> value"))
                .unwrap();
        lower(&syntax).unwrap()
    };
    let integer = lower_type("Int");
    let string = lower_type("String");
    assert_ne!(integer.contract_text(), string.contract_text());
    let CoreExpr::Lam {
        parameter_types, ..
    } = &integer
    else {
        panic!("lambda")
    };
    assert_eq!(parameter_types, &[Some(CoreType::Int)]);
    let encoded = serde_json::to_vec(&integer).unwrap();
    assert_eq!(
        serde_json::from_slice::<CoreExpr>(&encoded).unwrap(),
        integer
    );
}
