use crate::compiler::native_ir::{NativeExpr, NativeFunction, NativeType};

use super::*;

fn module(name: &str, value: i64) -> NativeModule {
    NativeModule {
        name: name.to_string(),
        functions: vec![NativeFunction {
            export_id: value as u64 + 100,
            name: "value".to_string(),
            public: true,
            arity: 0,
            source_module: name.to_string(),
            source_function: "value".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::Int(value),
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

#[test]
fn application_fingerprint_invalidates_units_when_a_sibling_body_changes() {
    let original = vec![module("app.First", 1), module("app.Second", 2)];
    let same = vec![module("app.First", 1), module("app.Second", 2)];
    let changed_sibling = vec![module("app.First", 1), module("app.Second", 3)];

    assert_eq!(
        application_implementation_fingerprint(&original),
        application_implementation_fingerprint(&same)
    );
    assert_ne!(
        application_implementation_fingerprint(&original),
        application_implementation_fingerprint(&changed_sibling)
    );
}
