//! Range operators lower to the public standard-library contract.

use super::*;

fn lower(body: &str) -> CoreExpr {
    let source = format!("module app.RangeOperators.\npub example(): Dynamic -> {body}.");
    let syntax = parse_module_as_syntax_output(&source).expect("parse range operator fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.functions
        .iter()
        .find(|function| function.name == "example")
        .expect("example function")
        .clauses[0]
        .body
        .core_expr
        .clone()
        .expect("example CoreIR")
}

#[test]
fn range_construction_lowers_to_the_public_inclusive_constructor() {
    assert_eq!(
        lower("1 .. 3"),
        CoreExpr::Call {
            type_args: Vec::new(),
            function: "std.range.Range.inclusive".to_string(),
            args: vec![CoreExpr::Int(1), CoreExpr::Int(3)],
        }
    );
}

#[test]
fn range_membership_lowers_to_the_public_receiver_operation() {
    assert_eq!(
        lower("2 in 1 .. 3"),
        CoreExpr::Call {
            type_args: Vec::new(),
            function: "std.range.Range.contains".to_string(),
            args: vec![
                CoreExpr::Call {
                    type_args: Vec::new(),
                    function: "std.range.Range.inclusive".to_string(),
                    args: vec![CoreExpr::Int(1), CoreExpr::Int(3)],
                },
                CoreExpr::Int(2),
            ],
        }
    );
}
