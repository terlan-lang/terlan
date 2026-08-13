use std::collections::HashMap;

use super::*;

/// Verifies constructor patterns receive the selected `Result` payload.
#[test]
fn result_constructor_binds_only_its_payload_type() {
    let ty = CoreType::Apply {
        constructor: "std.core.Result.Result".to_string(),
        args: vec![CoreType::Int, CoreType::String],
    };
    let pattern = CorePattern::Constructor {
        name: "Ok".to_string(),
        constructor_identity: None,
        args: vec![CorePattern::Var("value".to_string())],
    };
    let mut variables = HashMap::new();

    bind_pattern_types(&pattern, &ty, &mut variables);

    assert_eq!(variables.get("value"), Some(&CoreType::Int));
}

/// Verifies transparent `Result` aliases retain constructor payload types.
#[test]
fn expanded_result_union_binds_only_the_selected_payload_type() {
    let ty = CoreType::Union(vec![
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: CoreType::Int,
            },
        ]),
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("error".to_string())),
            CoreTupleTypeElem::Field {
                name: "reason".to_string(),
                ty: CoreType::String,
            },
        ]),
    ]);
    let pattern = CorePattern::Constructor {
        name: "Ok".to_string(),
        constructor_identity: None,
        args: vec![CorePattern::Var("value".to_string())],
    };
    let mut variables = HashMap::new();

    bind_pattern_types(&pattern, &ty, &mut variables);

    assert_eq!(variables.get("value"), Some(&CoreType::Int));
}
