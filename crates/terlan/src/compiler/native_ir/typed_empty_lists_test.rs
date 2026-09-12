use std::collections::HashMap;

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreRuntimeCapability, CoreType,
};

use super::typed_empty_lists::annotate;

#[test]
fn nested_empty_lists_recover_the_checked_nonempty_witness() {
    let empty = CoreExpr::List(Vec::new());
    let value = CoreExpr::List(vec![CoreExpr::Int(7)]);
    for items in [
        vec![empty.clone(), value.clone()],
        vec![value.clone(), empty.clone()],
        vec![empty.clone(), value, empty],
    ] {
        assert_eq!(
            super::structured_case::core_expr_type(
                &CoreExpr::List(items),
                &HashMap::new(),
                &HashMap::new(),
            ),
            Some(CoreType::List(Box::new(CoreType::List(Box::new(
                CoreType::Int
            )))))
        );
    }
}

#[test]
fn nested_list_witness_does_not_hide_unknown_or_incompatible_elements() {
    let integers = CoreExpr::List(vec![CoreExpr::Int(7)]);
    let empty = CoreExpr::List(Vec::new());
    for items in [
        vec![empty.clone(), CoreExpr::Int(7)],
        vec![empty.clone(), empty.clone()],
        vec![integers.clone(), CoreExpr::Var("unknown".to_string())],
        vec![
            integers.clone(),
            CoreExpr::List(vec![CoreExpr::Atom("true".to_string())]),
        ],
        vec![
            integers,
            CoreExpr::Cast {
                expr: Box::new(empty),
                target_type: CoreType::List(Box::new(CoreType::Bool)),
            },
        ],
    ] {
        assert_eq!(
            super::structured_case::core_expr_type(
                &CoreExpr::List(items),
                &HashMap::new(),
                &HashMap::new(),
            ),
            None
        );
    }
}

#[test]
fn runtime_tree_matching_empty_filters_inherit_string_list_types() {
    let mut expression = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextTreeMatching),
        args: vec![
            CoreExpr::Binary("\"crates\"".to_string()),
            CoreExpr::List(Vec::new()),
            CoreExpr::List(Vec::new()),
            CoreExpr::List(Vec::new()),
            CoreExpr::Int(0),
            CoreExpr::Int(64),
        ],
        return_type: CoreType::Dynamic,
        effects: CoreEffectSet {
            effects: vec!["filesystem".to_string()],
        },
        span: Span { start: 0, end: 0 },
    });

    annotate(&mut expression, &HashMap::new());

    let CoreExpr::Intrinsic(call) = expression else {
        panic!("runtime intrinsic changed shape");
    };
    for index in [1, 2, 3] {
        assert!(matches!(
            &call.args[index],
            CoreExpr::Cast {
                expr,
                target_type: CoreType::List(element),
            } if matches!(expr.as_ref(), CoreExpr::List(items) if items.is_empty())
                && element.as_ref() == &CoreType::String
        ));
    }
}

#[test]
fn runtime_tree_matching_preserves_nonempty_filters() {
    let suffix = CoreExpr::List(vec![CoreExpr::Binary("\".rs\"".to_string())]);
    let mut expression = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadTextTreeMatching),
        args: vec![
            CoreExpr::Binary("\"crates\"".to_string()),
            CoreExpr::List(Vec::new()),
            suffix.clone(),
            CoreExpr::List(Vec::new()),
            CoreExpr::Int(0),
            CoreExpr::Int(64),
        ],
        return_type: CoreType::Dynamic,
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: Span { start: 0, end: 0 },
    });

    annotate(&mut expression, &HashMap::new());

    let CoreExpr::Intrinsic(call) = expression else {
        panic!("runtime intrinsic changed shape");
    };
    assert_eq!(call.args[2], suffix);
}
