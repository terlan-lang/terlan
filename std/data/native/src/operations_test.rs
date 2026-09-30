use super::RESOURCE_OPERATIONS;
use std::collections::BTreeMap;

#[test]
fn contracts_cover_the_declared_json_surface_exactly() {
    let summary: serde_json::Value = serde_json::from_str(include_str!(
        "../../../summaries/std.data.Json.native_boundary.json"
    ))
    .unwrap();
    let functions = summary["functions"].as_array().unwrap();
    let mut expected = BTreeMap::new();
    for function in functions {
        assert!(expected
            .insert(
                function["operation"].as_str().unwrap(),
                function["arity"].as_u64().unwrap() as usize
            )
            .is_none());
    }
    let mut actual = BTreeMap::new();
    for contract in RESOURCE_OPERATIONS {
        assert!(actual.insert(contract.operation, contract.arity).is_none());
    }
    assert_eq!(actual, expected);
}

#[test]
fn direct_results_are_not_accidentally_wrapped_in_result() {
    let direct = [
        "null",
        "bool",
        "int",
        "string",
        "array",
        "object",
        "array_push",
        "array_extend",
        "array_set",
        "object_put",
        "object_remove",
        "to_string",
        "is_null",
    ];
    for contract in RESOURCE_OPERATIONS {
        let method = contract.operation.strip_prefix("std.data.json.").unwrap();
        assert_eq!(
            contract.result_error,
            if direct.contains(&method) {
                None
            } else {
                Some("JsonError")
            },
            "{method}"
        );
    }
}
