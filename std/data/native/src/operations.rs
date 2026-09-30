//! JSON source contracts, independent of compiler and VM dispatch tables.

use terlan_runtime_abi::NativeResourceOperation;

const fn direct(operation: &'static str, arity: usize) -> NativeResourceOperation {
    NativeResourceOperation {
        operation,
        arity,
        resource_type: "std.data.Json.Json",
        returns_resource: true,
        result_error: None,
        mutates_receiver: false,
    }
}

const fn result(operation: &'static str, arity: usize) -> NativeResourceOperation {
    NativeResourceOperation {
        operation,
        arity,
        resource_type: "std.data.Json.Json",
        returns_resource: true,
        result_error: Some("JsonError"),
        mutates_receiver: false,
    }
}

const fn mutating(operation: &'static str, arity: usize) -> NativeResourceOperation {
    NativeResourceOperation {
        mutates_receiver: true,
        ..direct(operation, arity)
    }
}

const fn value(mut contract: NativeResourceOperation) -> NativeResourceOperation {
    contract.returns_resource = false;
    contract
}

/// Exact JSON operations and their source return conventions.
///
/// Mutators return their receiver directly; a failed mutation is a boundary
/// failure, not an Err value. Registration grants no host or worker capability.
pub static RESOURCE_OPERATIONS: &[NativeResourceOperation] = &[
    direct("std.data.json.null", 0),
    direct("std.data.json.bool", 1),
    direct("std.data.json.int", 1),
    result("std.data.json.float", 1),
    direct("std.data.json.string", 1),
    direct("std.data.json.array", 0),
    direct("std.data.json.object", 0),
    mutating("std.data.json.array_push", 2),
    mutating("std.data.json.array_extend", 2),
    mutating("std.data.json.array_set", 3),
    mutating("std.data.json.object_put", 3),
    mutating("std.data.json.object_remove", 2),
    result("std.data.json.parse", 1),
    value(result("std.data.json.stringify", 1)),
    value(direct("std.data.json.to_string", 1)),
    value(result("std.data.json.stringify_pretty", 1)),
    result("std.data.json.get", 2),
    value(result("std.data.json.keys", 1)),
    value(result("std.data.json.object_length", 1)),
    value(result("std.data.json.required_fields", 4)),
    value(result("std.data.json.required_field_rows", 4)),
    value(result("std.data.json.required_field_rows_page", 6)),
    value(result("std.data.json.string_field_rows", 2)),
    value(result("std.data.json.nested_string_field_rows", 4)),
    value(result("std.data.json.nested_string_field_rows_page", 6)),
    result("std.data.json.string_object_rows", 2),
    value(result("std.data.json.length", 1)),
    result("std.data.json.at", 2),
    value(result("std.data.json.as_string", 1)),
    value(result("std.data.json.as_int", 1)),
    value(result("std.data.json.as_float", 1)),
    value(result("std.data.json.as_bool", 1)),
    value(direct("std.data.json.is_null", 1)),
];

/// Resolves an exact package operation, never a namespace prefix.
pub fn operation(name: &str) -> Option<&'static NativeResourceOperation> {
    RESOURCE_OPERATIONS
        .iter()
        .find(|contract| contract.operation == name)
}

#[cfg(test)]
#[path = "operations_test.rs"]
mod tests;
