use super::*;

fn binary(operator: &str, left: i64, right: i64) -> CoreExpr {
    CoreExpr::BinaryOp {
        operator: operator.into(),
        left: Box::new(CoreExpr::Int(left)),
        right: Box::new(CoreExpr::Int(right)),
    }
}

fn indexed(index: CoreExpr) -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".into()),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::Index {
            base: Box::new(CoreExpr::Var("pair".into())),
            index: Box::new(index),
        }),
    }
}

#[test]
fn checked_computed_indexes_eliminate_fixed_aggregate_allocations() {
    for index in [
        binary("+", 0, 1),
        binary("-", 2, 1),
        binary("*", 1, 1),
        binary("div", 3, 2),
        binary("rem", 3, 2),
    ] {
        let rewritten = scalar_replace_fixed_aggregates(&indexed(index), &layouts());
        let CoreExpr::Let { bindings, body } = rewritten else {
            panic!("scalar bindings")
        };
        assert!(bindings
            .iter()
            .all(|binding| matches!(binding.value, CoreExpr::Int(_))));
        assert_eq!(*body, CoreExpr::Var("$native_sroa_0_1".into()));
    }
}

#[test]
fn failing_or_out_of_bounds_index_expressions_keep_checked_execution() {
    for index in [
        binary("div", 1, 0),
        binary("+", i64::MAX, 1),
        binary("div", i64::MIN, -1),
        binary("-", 0, 1),
        binary("+", 1, 1),
        CoreExpr::Var("index".into()),
    ] {
        let original = indexed(index);
        assert_eq!(
            scalar_replace_fixed_aggregates(&original, &layouts()),
            original
        );
    }
}

#[cfg(all(unix, feature = "native-codegen"))]
#[test]
fn computed_index_source_survives_the_full_native_compilation_cycle() {
    let modules = crate::compiler::native_ir::source_constructor_test::check_sources(&[r#"
module computed_index.
pub check(): Bool ->
    let pair = {20, 22};
    pair[3 div 2] == 22 and {20, 22}[2 - 2] == 20.
"#]);
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "check")
        .unwrap();
    let mut encodings = Vec::new();
    function.body.collect_managed_encodings(&mut encodings);
    assert!(
        encodings.is_empty(),
        "computed fixed indexes still reach the managed allocator"
    );
}
