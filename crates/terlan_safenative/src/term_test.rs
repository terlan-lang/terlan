use super::*;
use crate::dispatch::DispatchError;
use crate::postgres;

/// Builds a stable handle fixture for term codec tests.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - A deterministic handle with id `42` and generation `7`.
///
/// Transformation:
/// - Constructs fixed resource identity data for round-trip assertions.
fn handle_fixture() -> SafeNativeHandle {
    SafeNativeHandle {
        id: 42,
        generation: 7,
    }
}

/// Verifies primitive bridge values round-trip through terms.
///
/// Inputs:
/// - Representative unit, text, integer, float, and bool values.
///
/// Output:
/// - Test passes when decoded values match the original inputs.
///
/// Transformation:
/// - Exercises lossless primitive conversion without resource access.
#[test]
fn primitive_values_round_trip_through_terms() {
    let values = [
        SafeNativeBridgeValue::Unit,
        SafeNativeBridgeValue::Text(String::from("hello")),
        SafeNativeBridgeValue::Int(7),
        SafeNativeBridgeValue::Float(1.5),
        SafeNativeBridgeValue::Bool(true),
    ];

    for value in values {
        let term = encode_bridge_value(value.clone());
        assert_eq!(decode_bridge_value(&term), value);
    }
}

/// Verifies opaque handles round-trip through explicit id/generation terms.
///
/// Inputs:
/// - A deterministic resource handle.
///
/// Output:
/// - Test passes when the handle survives encode/decode unchanged.
///
/// Transformation:
/// - Confirms the bridge term contract never carries adapter-owned Rust
///   values directly.
#[test]
fn handle_value_round_trips_through_term_fields() {
    let handle = handle_fixture();
    let value = SafeNativeBridgeValue::Handle(handle);

    assert_eq!(
        encode_bridge_value(value.clone()),
        SafeNativeTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        }
    );
    assert_eq!(
        decode_bridge_value(&encode_bridge_value(value.clone())),
        value
    );
}

/// Verifies optional text and handle values round-trip through terms.
///
/// Inputs:
/// - `Some` and `None` optional bridge values.
///
/// Output:
/// - Test passes when optional payload shape is preserved.
///
/// Transformation:
/// - Exercises the optional result shapes used by path and URI accessors.
#[test]
fn optional_values_round_trip_through_terms() {
    let values = [
        SafeNativeBridgeValue::OptionalText(Some(String::from("name"))),
        SafeNativeBridgeValue::OptionalText(None),
        SafeNativeBridgeValue::OptionalHandle(Some(handle_fixture())),
        SafeNativeBridgeValue::OptionalHandle(None),
    ];

    for value in values {
        let term = encode_bridge_value(value.clone());
        assert_eq!(decode_bridge_value(&term), value);
    }
}

/// Verifies Postgres config values round-trip through explicit terms.
///
/// Inputs:
/// - A Postgres connection config used by the runtime connect operation.
///
/// Output:
/// - Test passes when the config survives encode/decode unchanged.
///
/// Transformation:
/// - Exercises the input-only bridge term shape needed before a handler can
///   call `std.db.postgres.connect` through `SafeNativeRuntime`.
#[test]
fn postgres_config_round_trips_through_terms() {
    let config = postgres::Config::new("postgres://localhost/terlan")
        .with_pool_limits(1, 2)
        .with_timeouts(100, 200);
    let value = SafeNativeBridgeValue::PostgresConfig(config);

    let term = encode_bridge_value(value.clone());

    assert_eq!(decode_bridge_value(&term), value);
}

/// Verifies argument lists preserve order through term encoding.
///
/// Inputs:
/// - A mixed bridge argument list.
///
/// Output:
/// - Test passes when decoded arguments equal the original ordered list.
///
/// Transformation:
/// - Exercises bulk argument conversion used before resource dispatch.
#[test]
fn argument_lists_round_trip_in_order() {
    let args = vec![
        SafeNativeBridgeValue::Handle(handle_fixture()),
        SafeNativeBridgeValue::Text(String::from("key")),
        SafeNativeBridgeValue::Bool(false),
    ];

    let terms = encode_bridge_args(&args);

    assert_eq!(decode_bridge_args(&terms), args);
}

/// Verifies command terms expose their request ids consistently.
///
/// Inputs:
/// - A call command and a dispose command with different request ids.
///
/// Output:
/// - Test passes when each command reports its own request id.
///
/// Transformation:
/// - Exercises the common request-id accessor used by transport-neutral
///   worker dispatch.
#[test]
fn command_terms_expose_request_ids() {
    let call = SafeNativeCommandTerm::Call {
        request_id: 11,
        operation: String::from("std.encoding.base64.encode"),
        args: vec![SafeNativeTerm::Text(String::from("hello"))],
    };
    let dispose = SafeNativeCommandTerm::Dispose {
        request_id: 12,
        handle: handle_fixture(),
    };

    assert_eq!(call.request_id(), 11);
    assert_eq!(dispose.request_id(), 12);
}

/// Verifies successful dispatch replies encode as `Ok` terms.
///
/// Inputs:
/// - A successful bridge dispatch result.
///
/// Output:
/// - Test passes when the reply decodes back to the original value.
///
/// Transformation:
/// - Converts a result into the reply contract and back through the success
///   decoder.
#[test]
fn successful_dispatch_reply_decodes_to_value() {
    let value = SafeNativeBridgeValue::Text(String::from("done"));
    let reply = encode_dispatch_reply(Ok(value.clone()));

    assert_eq!(decode_success_reply(&reply), Ok(value));
}

/// Verifies failed dispatch replies preserve stable error fields.
///
/// Inputs:
/// - A dispatch error with code, message, and offset.
///
/// Output:
/// - Test passes when decoding returns the same stable error payload.
///
/// Transformation:
/// - Converts dispatch errors into bridge reply terms without backend
///   exception leakage.
#[test]
fn failed_dispatch_reply_decodes_to_stable_term_error() {
    let reply = encode_dispatch_reply(Err(DispatchError::new("dispatch.type", "wrong value", 12)));
    let error = decode_success_reply(&reply)
        .err()
        .unwrap_or_else(|| TermError::new("missing", "missing", 0));

    assert_eq!(error.code(), "dispatch.type");
    assert_eq!(error.message(), "wrong value");
    assert_eq!(error.offset(), 12);
}
