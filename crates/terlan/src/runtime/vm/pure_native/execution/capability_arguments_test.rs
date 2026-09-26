//! Local VM calls do not inherit the external worker's narrower wire vocabulary.

use super::*;
use crate::runtime::vm::bitstring::VmBitString;

fn package(arguments: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: "test.external.operation".into(),
        arguments: vec![],
        package_arguments: Some(arguments),
        result_type: TvmBoundaryType::Unit,
    }
}

#[test]
fn built_in_arguments_are_borrowed_without_cloning() {
    let request = PureNativeCapabilityRequest {
        capability: "stdio".into(),
        operation: "std.io.console.println".into(),
        arguments: vec![NativeBoundaryTerm::Text("line".into())],
        package_arguments: None,
        result_type: TvmBoundaryType::Unit,
    };
    assert!(matches!(
        request.boundary_arguments().unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(
        request.boundary_arguments().unwrap().as_ref(),
        request.arguments
    );
}

#[test]
fn wire_conversion_uses_typed_package_values_not_stale_wire_fields() {
    let mut request = package(vec![ReplValue::Tuple(vec![
        ReplValue::Int(7),
        ReplValue::String("value".into()),
    ])]);
    request.arguments = vec![NativeBoundaryTerm::Int(0)];
    assert_eq!(
        request.boundary_arguments().unwrap().as_ref(),
        &[NativeBoundaryTerm::Tuple(vec![
            NativeBoundaryTerm::Int(7),
            NativeBoundaryTerm::Text("value".into()),
        ])]
    );
    assert!(request.package_arguments.is_some());
}

#[test]
fn unsupported_external_values_remain_vm_owned_and_do_not_leak_in_errors() {
    for value in [
        ReplValue::Map(vec![(
            ReplValue::Int(1),
            ReplValue::String("private-payload".into()),
        )]),
        ReplValue::Set(vec![ReplValue::String("private-payload".into())]),
        ReplValue::BitString(VmBitString::from_bytes(&[0xa0], 3).unwrap()),
    ] {
        let request = package(vec![ReplValue::Tuple(vec![value])]);
        let original = request.package_arguments.clone();
        let error = request.boundary_arguments().unwrap_err();
        assert!(error.starts_with("error[pure_native_capability_argument]"));
        assert!(!error.contains("private-payload"));
        assert_eq!(request.package_arguments, original);
    }
}
