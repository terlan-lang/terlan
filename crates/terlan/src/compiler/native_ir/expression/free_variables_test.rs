//! Tests for exhaustive CoreIR free-variable analysis.

use std::collections::HashSet;

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreListComprehensionGenerator, CoreMapExprField, CorePattern,
    CoreRecordExprField, CoreTryAfter, CoreType,
};

use super::free_variables;

fn names(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn free_variables_cover_structured_control_and_pattern_scopes() {
    let expression = CoreExpr::Try {
        body: Box::new(CoreExpr::Tuple(vec![CoreExpr::Var("protected".into())])),
        of_clauses: vec![CoreCaseClause {
            pattern: CorePattern::Tuple(vec![CorePattern::Var("bound".into())]),
            guard: Some(CoreExpr::BinaryOp {
                operator: ">".into(),
                left: Box::new(CoreExpr::Var("bound".into())),
                right: Box::new(CoreExpr::Var("guard_free".into())),
            }),
            body: CoreExpr::Map(vec![CoreMapExprField {
                key: "answer".into(),
                required: true,
                value: CoreExpr::BinaryOp {
                    operator: "+".into(),
                    left: Box::new(CoreExpr::Var("bound".into())),
                    right: Box::new(CoreExpr::Var("body_free".into())),
                },
            }]),
        }],
        catch_clauses: Vec::new(),
        after_clause: Some(CoreTryAfter {
            trigger: Box::new(CoreExpr::Var("after_trigger".into())),
            body: Box::new(CoreExpr::Var("after_body".into())),
        }),
    };

    assert_eq!(
        free_variables(&expression),
        names(&[
            "protected",
            "guard_free",
            "body_free",
            "after_trigger",
            "after_body",
        ])
    );
}

#[test]
fn free_variables_cover_comprehension_lambda_and_managed_shapes() {
    let comprehension = CoreExpr::ListComprehension {
        expr: Box::new(CoreExpr::List(vec![
            CoreExpr::Var("item".into()),
            CoreExpr::Var("yield_free".into()),
        ])),
        generators: vec![CoreListComprehensionGenerator {
            pattern: CorePattern::Var("item".into()),
            source: CoreExpr::Var("source_free".into()),
        }],
        guards: vec![CoreExpr::BinaryOp {
            operator: ">".into(),
            left: Box::new(CoreExpr::Var("item".into())),
            right: Box::new(CoreExpr::Var("guard_free".into())),
        }],
        lift: None,
    };
    let lambda = CoreExpr::Lam {
        params: vec![CorePattern::Tuple(vec![CorePattern::Var(
            "parameter".into(),
        )])],
        body: Box::new(CoreExpr::RecordUpdate {
            base: Box::new(CoreExpr::Var("record_free".into())),
            name: "Record".into(),
            fields: vec![CoreRecordExprField {
                key: "value".into(),
                required: true,
                value: CoreExpr::Cast {
                    expr: Box::new(CoreExpr::BinaryOp {
                        operator: "+".into(),
                        left: Box::new(CoreExpr::Var("parameter".into())),
                        right: Box::new(CoreExpr::Var("capture_free".into())),
                    }),
                    target_type: CoreType::Int,
                },
            }],
        }),
    };

    assert_eq!(
        free_variables(&comprehension),
        names(&["source_free", "guard_free", "yield_free"])
    );
    assert_eq!(
        free_variables(&lambda),
        names(&["record_free", "capture_free"])
    );
}
