//! Package declarations remain authoritative without granting storage access.

use crate::terlan_native_boundary::dispatch::{
    dispatch, dispatch_with_resources, operation_arity, validate_operation_arity,
    NativeBoundaryBridgeValue, NativeBoundaryValue,
};
use crate::terlan_native_boundary::resource::ResourceStore;
use terlan_http_native::session_bindings::{bindings, SessionStorage};

#[test]
fn session_package_catalog_matches_source_and_runtime_arity() {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(include_str!(
        "../../../../../../std/http/Session.terl"
    ))
    .unwrap();
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut declared = std::collections::BTreeMap::new();
    for function in core.functions {
        if let Some(operation) = function.native_operation {
            assert!(declared.insert(operation, function.arity).is_none());
        }
    }
    let catalog = bindings::<dyn SessionStorage>();
    assert_eq!(declared.len(), catalog.len());
    for binding in catalog {
        assert_eq!(declared.get(binding.operation), Some(&binding.arity()));
        assert_eq!(operation_arity(binding.operation), Some(binding.arity()));
        for count in 0..=4 {
            let result = validate_operation_arity(binding.operation, count, |_| {
                panic!("package declaration must be known")
            });
            if count == binding.arity() {
                assert!(result.is_ok());
            } else {
                assert_eq!(result.unwrap_err().code(), "dispatch.arity");
            }
        }
        for unknown in [
            format!("{}.extra", binding.operation),
            binding.operation.to_uppercase(),
        ] {
            assert_eq!(operation_arity(&unknown), None);
        }
    }
}

#[test]
fn session_catalog_does_not_create_a_context_in_legacy_dispatch() {
    let mut store = ResourceStore::new();
    let before = store.clone();
    for binding in bindings::<dyn SessionStorage>() {
        let args = vec![NativeBoundaryValue::Text("identity".into()); binding.arity()];
        assert_eq!(
            dispatch(binding.operation, &args).unwrap_err().code(),
            "dispatch.unknown_operation"
        );
        let args = vec![NativeBoundaryBridgeValue::Text("identity".into()); binding.arity()];
        assert_eq!(
            dispatch_with_resources(&mut store, binding.operation, &args)
                .unwrap_err()
                .code(),
            "dispatch.unknown_operation"
        );
        assert_eq!(store, before);
    }
}
