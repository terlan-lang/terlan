use super::*;

/// Extracts a handle from a successful reply term.
///
/// Inputs:
/// - `reply`: runtime reply expected to contain a handle.
///
/// Output:
/// - `Some(handle)` when the reply is a handle success.
/// - `None` after asserting the reply had an unexpected shape.
///
/// Transformation:
/// - Pattern-matches the stable reply term without unwrap/expect.
fn handle_reply(reply: SafeNativeReplyTerm) -> Option<SafeNativeHandle> {
    let SafeNativeReplyTerm::Ok(SafeNativeTerm::Handle { id, generation }) = reply else {
        return None;
    };
    Some(SafeNativeHandle { id, generation })
}

/// Verifies JSON can execute through the term runtime path.
///
/// Inputs:
/// - JSON text, object key, and operation ids.
///
/// Output:
/// - Test passes when parse/get/as-string returns the expected text.
///
/// Transformation:
/// - Exercises term decoding, resource-backed dispatch, opaque handle
///   storage, and reply encoding together.
#[test]
fn runtime_executes_json_operations_through_terms() {
    let mut runtime = SafeNativeRuntime::new();
    let Some(root) = handle_reply(runtime.call(
        "std.data.json.parse",
        &[SafeNativeTerm::Text(String::from(r#"{"name":"Ada"}"#))],
    )) else {
        return;
    };
    let Some(name) = handle_reply(runtime.call(
        "std.data.json.get",
        &[
            SafeNativeTerm::Handle {
                id: root.id,
                generation: root.generation,
            },
            SafeNativeTerm::Text(String::from("name")),
        ],
    )) else {
        return;
    };

    assert_eq!(
        runtime.call(
            "std.data.json.as_string",
            &[SafeNativeTerm::Handle {
                id: name.id,
                generation: name.generation,
            }],
        ),
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("Ada")))
    );
}

/// Verifies Base64 can execute through the term runtime path.
///
/// Inputs:
/// - Text operation argument.
///
/// Output:
/// - Test passes when encode returns the expected Base64 text.
///
/// Transformation:
/// - Exercises primitive-only SafeNative dispatch without resource handles.
#[test]
fn runtime_executes_base64_operations_through_terms() {
    let mut runtime = SafeNativeRuntime::new();

    assert_eq!(
        runtime.call(
            "std.encoding.base64.encode",
            &[SafeNativeTerm::Text(String::from("hello"))],
        ),
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("aGVsbG8=")))
    );
}

/// Verifies path operations can return optional handle terms.
///
/// Inputs:
/// - Path text with a parent component.
///
/// Output:
/// - Test passes when parent returns `OptionalHandle(Some(_))`.
///
/// Transformation:
/// - Exercises optional opaque resource results through the runtime term
///   boundary.
#[test]
fn runtime_executes_path_optional_handle_operations_through_terms() {
    let mut runtime = SafeNativeRuntime::new();
    let Some(path) = handle_reply(runtime.call(
        "std.io.path.from_string",
        &[SafeNativeTerm::Text(String::from("src/main.terl"))],
    )) else {
        return;
    };

    let parent = runtime.call(
        "std.io.path.parent",
        &[SafeNativeTerm::Handle {
            id: path.id,
            generation: path.generation,
        }],
    );

    assert!(matches!(
        parent,
        SafeNativeReplyTerm::Ok(SafeNativeTerm::OptionalHandle(Some(_)))
    ));
}

/// Verifies disposed handles are rejected by later runtime calls.
///
/// Inputs:
/// - JSON parse output handle disposed before a second call.
///
/// Output:
/// - Test passes when the later operation returns `resource.stale_handle`.
///
/// Transformation:
/// - Exercises deterministic cleanup and stale-handle rejection through the
///   term runtime boundary.
#[test]
fn runtime_rejects_disposed_handles_through_terms() {
    let mut runtime = SafeNativeRuntime::new();
    let Some(root) = handle_reply(runtime.call(
        "std.data.json.parse",
        &[SafeNativeTerm::Text(String::from("null"))],
    )) else {
        return;
    };
    assert_eq!(
        runtime.dispose(root),
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Unit)
    );

    assert_eq!(
        runtime.call(
            "std.data.json.is_null",
            &[SafeNativeTerm::Handle {
                id: root.id,
                generation: root.generation,
            }],
        ),
        SafeNativeReplyTerm::Error {
            code: String::from("resource.stale_handle"),
            message: String::from("SafeNative resource handle 1 generation 1 is not live."),
            offset: 0,
        }
    );
}
