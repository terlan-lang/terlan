//! Tests for callable capture metadata in native image descriptors.

use crate::compiler::native_ir::{NativeExpr, NativeFunction, NativeModule, NativeType};
use crate::runtime::native_image::TvmBoundaryType;

use super::native_descriptor::native_application_image_descriptor;

#[test]
fn lifted_callable_descriptor_separates_captures_from_call_arguments() {
    let module = NativeModule {
        name: "DescriptorClosure".to_owned(),
        functions: vec![
            NativeFunction {
                export_id: 301,
                name: "lifted".to_owned(),
                public: false,
                arity: 2,
                source_module: "DescriptorClosure".to_owned(),
                source_function: "main".to_owned(),
                source_arity: 0,
                callable_captures: vec![NativeType::Int],
                params: vec![NativeType::Int, NativeType::Bool],
                return_type: NativeType::Int,
                body: NativeExpr::Param(0),
            },
            NativeFunction {
                export_id: 302,
                name: "main".to_owned(),
                public: true,
                arity: 0,
                source_module: "DescriptorClosure".to_owned(),
                source_function: "main".to_owned(),
                source_arity: 0,
                callable_captures: Vec::new(),
                params: Vec::new(),
                return_type: NativeType::Unit,
                body: NativeExpr::Unit,
            },
        ],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    };

    let descriptor = native_application_image_descriptor(
        "descriptor-closure",
        "descriptor-closure",
        &[module],
        "11",
    )
    .expect("native descriptor");

    assert_eq!(descriptor.exports.len(), 1);
    assert_eq!(descriptor.exports[0].id, 302);
    assert_eq!(descriptor.callables.len(), 2);
    assert_eq!(descriptor.callables[0].id, 301);
    assert_eq!(descriptor.callables[0].captures, [TvmBoundaryType::Int]);
    assert_eq!(descriptor.callables[0].parameters, [TvmBoundaryType::Bool]);
    assert_eq!(descriptor.callables[0].results, [TvmBoundaryType::Int]);
}

#[test]
fn materialized_continuation_functions_are_not_admitted_as_closures() {
    let module = NativeModule {
        name: "$terlan.continuations".to_owned(),
        functions: vec![NativeFunction {
            export_id: 401,
            name: "$continuation_7".to_owned(),
            public: false,
            arity: 0,
            source_module: "app.Main".to_owned(),
            source_function: "main".to_owned(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::Int(42),
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    };

    let descriptor = native_application_image_descriptor(
        "descriptor-continuation-body",
        "descriptor-continuation-body",
        &[module],
        "22",
    )
    .expect("native descriptor");

    assert!(descriptor.exports.is_empty());
    assert!(descriptor.callables.is_empty());
}
