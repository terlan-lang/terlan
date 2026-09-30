//! Package value transport, admission, and request lifecycle regressions.

use super::*;
use crate::native_worker::protocol::{run_capability_worker, CapabilityWorkerConfig};
use crate::terlan_native_boundary::capability_sandbox::CapabilitySandboxProfile;
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, write_json_frame, CapabilityHandle, CapabilityOutcome, CapabilityRequest,
    CapabilityResponse, CAPABILITY_PROTOCOL_VERSION,
};

fn identity(arguments: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    Ok(arguments[0].clone())
}

const IDENTITY: NativeBinding = NativeBinding {
    operation: "application.identity",
    arity: 1,
    invoke: identity,
};

fn request(id: u64, arguments: Vec<CapabilityValue>) -> CapabilityCall {
    CapabilityCall {
        request_id: id,
        owner_id: 7,
        capability: "package-native".into(),
        operation: IDENTITY.operation.into(),
        arguments,
    }
}

#[test]
fn preserves_owned_values_and_constructor_field_order() {
    let values = vec![
        CapabilityValue::Map(vec![(
            CapabilityValue::Text("key".into()),
            CapabilityValue::Map(vec![(CapabilityValue::Int(1), CapabilityValue::Bool(true))]),
        )]),
        CapabilityValue::Unit,
        CapabilityValue::Bool(true),
        CapabilityValue::Int(-42),
        CapabilityValue::Float(1.5),
        CapabilityValue::Text("\u{03bb}".into()),
        CapabilityValue::Bytes(vec![0, 255]),
        CapabilityValue::Atom("ready".into()),
        CapabilityValue::Tuple(vec![CapabilityValue::Int(1)]),
        CapabilityValue::Record {
            name: "Pair".into(),
            fields: vec![
                ("second".into(), CapabilityValue::Text("x".into())),
                ("first".into(), CapabilityValue::List(vec![])),
            ],
        },
    ];
    let value = CapabilityValue::List(values);
    assert_eq!(
        execute(&IDENTITY, vec![value.clone()]).unwrap(),
        value.into_term()
    );
    for option in [None, Some(String::new()), Some("value".into())] {
        let expected: NativeValue = option.clone().into();
        assert_eq!(
            to_native(CapabilityValue::OptionalText(option)).unwrap(),
            expected
        );
    }
}

#[test]
fn rejects_handles_including_nested_values_before_callback() {
    let handle = CapabilityHandle {
        id: 1,
        generation: 1,
    };
    for value in [
        CapabilityValue::Handle(handle),
        CapabilityValue::OptionalHandle(None),
        CapabilityValue::OptionalHandle(Some(handle)),
    ] {
        for value in [
            CapabilityValue::Map(vec![(value.clone(), CapabilityValue::Unit)]),
            CapabilityValue::Map(vec![(CapabilityValue::Unit, value.clone())]),
            value.clone(),
            CapabilityValue::List(vec![value.clone()]),
            CapabilityValue::Record {
                name: "Box".into(),
                fields: vec![("value".into(), value)],
            },
        ] {
            let error = execute(&IDENTITY, vec![value]).unwrap_err();
            assert_eq!(error.code(), "native_package.value");
        }
    }
    let error = execute(&IDENTITY, vec![]).unwrap_err();
    assert_eq!(error.code(), "native_package.arguments");
    let error = execute(
        &IDENTITY,
        vec![CapabilityValue::Unit, CapabilityValue::Handle(handle)],
    )
    .unwrap_err();
    assert_eq!(error.code(), "native_package.arguments");
}

#[test]
fn rejects_nonfinite_floats_and_oversized_results() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            execute(&IDENTITY, vec![CapabilityValue::Float(value)])
                .unwrap_err()
                .code(),
            "native_package.value"
        );
        assert_eq!(
            validate_result(&NativeValue::List(vec![NativeValue::Float(value)]))
                .unwrap_err()
                .code(),
            "native_package.value"
        );
    }
    let at_limit = NativeValue::List(vec![NativeValue::Unit; MAX_CAPABILITY_TERM_COUNT - 1]);
    validate_result(&at_limit).unwrap();
    assert!(validate_result(&NativeValue::List(vec![
        NativeValue::Unit;
        MAX_CAPABILITY_TERM_COUNT
    ]))
    .is_err());
    let mut nested = NativeValue::Unit;
    for _ in 0..64 {
        nested = NativeValue::Tuple(vec![nested]);
    }
    validate_result(&nested).unwrap();
    assert!(validate_result(&NativeValue::Tuple(vec![nested])).is_err());
}

#[test]
fn releases_credit_after_success_argument_failure_and_callback_failure() {
    let fail = NativeBinding {
        operation: "application.failure",
        arity: 1,
        invoke: |_| Err(invalid_value("callback rejected value")),
    };
    let mut worker = NativeBoundaryWorker::new(1);
    let success = call(
        &mut worker,
        &IDENTITY,
        request(1, vec![CapabilityValue::Int(7)]),
    );
    assert_eq!(
        success.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Int(7))
    );
    let bad_args = call(&mut worker, &IDENTITY, request(2, vec![]));
    assert!(
        matches!(bad_args.result, NativeBoundaryReplyTerm::Error { ref code, .. } if code == "native_package.arguments")
    );
    let failed = call(&mut worker, &fail, request(3, vec![CapabilityValue::Unit]));
    assert!(
        matches!(failed.result, NativeBoundaryReplyTerm::Error { ref code, .. } if code == "native_package.value")
    );
    for reply in [success, bad_args, failed] {
        assert_eq!(reply.reserved_credits, 0);
        assert_eq!(reply.available_credits, 1);
    }
    let duplicate = call(
        &mut worker,
        &IDENTITY,
        request(3, vec![CapabilityValue::Unit]),
    );
    assert!(matches!(
        duplicate.result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
    assert_eq!(worker.available_credits(), 1);
}

fn roundtrip(
    policy: &[&str],
    capability: &str,
    operation: &str,
    arguments: Vec<CapabilityValue>,
) -> CapabilityOutcome {
    let profile = CapabilitySandboxProfile::current().unwrap();
    let args: Vec<_> = [
        "--execution-profile",
        "crash-isolated",
        "--sandbox-profile",
        profile.name(),
    ]
    .into_iter()
    .chain(policy.iter().copied())
    .map(std::ffi::OsString::from)
    .collect();
    let config = CapabilityWorkerConfig::parse(&args).unwrap();
    let mut input = Vec::new();
    write_json_frame(
        &mut input,
        &CapabilityRequest::Call {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 7,
            capability: capability.into(),
            operation: operation.into(),
            arguments,
        },
        16_384,
    )
    .unwrap();
    write_json_frame(
        &mut input,
        &CapabilityRequest::Shutdown {
            version: CAPABILITY_PROTOCOL_VERSION,
        },
        16_384,
    )
    .unwrap();
    let mut output = Vec::new();
    run_capability_worker(config, std::io::Cursor::new(input), &mut output).unwrap();
    let mut output = std::io::Cursor::new(output);
    let Some(CapabilityResponse::Reply {
        request_id,
        reserved_credits,
        available_credits,
        outcome,
        ..
    }) = read_json_frame(&mut output, 16_384).unwrap()
    else {
        panic!("expected call reply");
    };
    assert_eq!(request_id, 1);
    assert_eq!(reserved_credits, 0);
    assert_eq!(available_credits, 64);
    assert!(matches!(
        read_json_frame(&mut output, 16_384).unwrap(),
        Some(CapabilityResponse::ShutdownAck { .. })
    ));
    outcome
}

const POLICY: &[&str] = &["--allow", "package-native", "--worker-class", "fast"];

#[test]
fn registered_resource_calls_require_exact_operation_capability_and_scheduler_admission() {
    let parsed = roundtrip(
        POLICY,
        "package-native",
        "std.data.json.parse",
        vec![CapabilityValue::Text("[1,2]".into())],
    );
    assert_eq!(
        parsed,
        CapabilityOutcome::Ok {
            value: CapabilityValue::Handle(CapabilityHandle {
                id: 1,
                generation: 1
            })
        }
    );
    for (policy, capability, operation, expected) in [
        (
            &[][..],
            "package-native",
            "std.data.json.parse",
            "native_boundary.capability_denied",
        ),
        (
            &["--allow", "package-native"][..],
            "package-native",
            "std.data.json.parse",
            "native_boundary.scheduler_denied",
        ),
        (
            POLICY,
            "filesystem",
            "std.data.json.parse",
            "capability_worker.capability_mismatch",
        ),
        (
            POLICY,
            "package-native",
            "std.data.json.parse.extra",
            "capability_worker.operation_denied",
        ),
        (
            POLICY,
            "package-native",
            "std.data.json.parse",
            "dispatch.arity",
        ),
    ] {
        let outcome = roundtrip(policy, capability, operation, vec![]);
        assert!(
            matches!(&outcome, CapabilityOutcome::Error { code, .. } if code == expected),
            "{outcome:?}"
        );
    }
    let stale = roundtrip(
        POLICY,
        "package-native",
        "std.data.json.length",
        vec![CapabilityValue::Handle(CapabilityHandle {
            id: 1,
            generation: 1,
        })],
    );
    assert!(
        matches!(&stale, CapabilityOutcome::Error { code, .. } if code == "resource.stale_handle"),
        "{stale:?}"
    );
}

#[test]
fn production_worker_invokes_registered_package_and_preserves_typed_error_result() {
    for text in ["https://example.com/a?b=#", "not a URL"] {
        let arguments = vec![CapabilityValue::Text(text.into())];
        let expected = execute(&terlan_net_native::PARSE, arguments.clone()).unwrap();
        assert_eq!(
            roundtrip(
                POLICY,
                "package-native",
                terlan_net_native::PARSE.operation,
                arguments
            ),
            CapabilityOutcome::Ok {
                value: CapabilityValue::from_term(expected)
            }
        );
    }
}

#[test]
fn worker_admission_is_exact_and_does_not_grant_namespace_or_handle_access() {
    for (policy, capability, operation, expected) in [
        (
            &[][..],
            "package-native",
            terlan_net_native::PARSE.operation,
            "native_boundary.capability_denied",
        ),
        (
            &["--allow", "package-native"][..],
            "package-native",
            terlan_net_native::PARSE.operation,
            "native_boundary.scheduler_denied",
        ),
        (
            POLICY,
            "filesystem",
            terlan_net_native::PARSE.operation,
            "capability_worker.capability_mismatch",
        ),
        (
            POLICY,
            "package-native",
            "std.net.uri.parse_parts_extra",
            "capability_worker.operation_denied",
        ),
        (
            POLICY,
            "package-native",
            "std.http.request.path",
            "capability_worker.operation_denied",
        ),
    ] {
        let result = roundtrip(policy, capability, operation, vec![]);
        assert!(
            matches!(result, CapabilityOutcome::Error { ref code, .. } if code == expected),
            "{result:?}"
        );
    }
    let result = roundtrip(
        POLICY,
        "package-native",
        terlan_net_native::PARSE.operation,
        vec![CapabilityValue::Handle(CapabilityHandle {
            id: 1,
            generation: 1,
        })],
    );
    assert!(
        matches!(result, CapabilityOutcome::Error { ref code, .. } if code == "native_package.value")
    );
}
#[test]
fn map_results_cannot_hide_nonfinite_values_or_bypass_term_and_depth_limits() {
    for entry in [
        (NativeValue::Float(f64::NAN), NativeValue::Unit),
        (NativeValue::Unit, NativeValue::Float(f64::INFINITY)),
    ] {
        assert!(validate_result(&NativeValue::Map(vec![entry])).is_err());
    }
    let entries = vec![(NativeValue::Unit, NativeValue::Unit); MAX_CAPABILITY_TERM_COUNT / 2];
    assert!(validate_result(&NativeValue::Map(entries)).is_err());
    let mut value = NativeValue::Unit;
    for _ in 0..65 {
        value = NativeValue::Map(vec![(value, NativeValue::Unit)]);
    }
    assert!(validate_result(&value).is_err());
}
