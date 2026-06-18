use super::*;

/// Parses Rust-backed std operations from the release manifest.
///
/// Inputs:
/// - Checked-in `std/RUST_BACKED_MANIFEST.tsv` embedded at compile time.
///
/// Output:
/// - Operation ids and arities from manifest rows.
///
/// Transformation:
/// - Skips comments/header lines, splits TSV rows, and keeps only rows
///   with a valid operation and integer arity.
fn rust_backed_manifest_operations() -> Vec<(&'static str, usize)> {
    include_str!("../../../std/RUST_BACKED_MANIFEST.tsv")
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("module\t") {
                return None;
            }
            let parts = line.split('\t').collect::<Vec<_>>();
            let operation = parts.get(3)?;
            let arity_text = parts.get(5)?;
            let arity = arity_text.parse::<usize>().ok()?;
            Some((*operation, arity))
        })
        .collect()
}

/// Dispatches an operation and returns a neutral value for tests.
///
/// Inputs:
/// - `operation`: compiler-native operation id expected to succeed.
/// - `args`: neutral operation arguments.
///
/// Output:
/// - `Some(value)` when dispatch succeeds.
/// - `None` after asserting failure is unexpected.
///
/// Transformation:
/// - Converts a dispatch result into an optional test value without
///   unwrap/expect.
fn dispatch_ok(operation: &str, args: &[SafeNativeValue]) -> Option<SafeNativeValue> {
    let result = dispatch(operation, args);
    assert!(result.is_ok());
    result.ok()
}

/// Dispatches a bridge operation and returns a bridge value for tests.
///
/// Inputs:
/// - `store`: resource store used by the bridge dispatcher.
/// - `operation`: compiler-native operation id expected to succeed.
/// - `args`: bridge-facing operation arguments.
///
/// Output:
/// - `Some(value)` when dispatch succeeds.
/// - `None` after asserting failure is unexpected.
///
/// Transformation:
/// - Converts a bridge dispatch result into an optional test value without
///   unwrap/expect.
fn bridge_dispatch_ok(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Option<SafeNativeBridgeValue> {
    let result = dispatch_with_resources(store, operation, args);
    assert!(result.is_ok());
    result.ok()
}

/// Validates dispatcher arities against the Rust-backed std manifest.
///
/// Inputs:
/// - Checked-in manifest rows for Rust-backed std operations.
///
/// Output:
/// - Test passes when each manifest operation is known to dispatch.
///
/// Transformation:
/// - Compares the release manifest operation inventory to
///   `operation_arity` so dispatch cannot silently drift from std.
#[test]
fn operation_arities_cover_rust_backed_std_manifest() {
    let operations = rust_backed_manifest_operations();
    assert_eq!(operations.len(), 42);

    for (operation, arity) in operations {
        assert_eq!(operation_arity(operation), Some(arity), "{operation}");
    }
}

/// Validates JSON constructor dispatch.
///
/// Inputs:
/// - JSON builder operation ids and primitive dispatch values.
///
/// Output:
/// - Test passes when constructor dispatch returns JSON values that render to
///   expected compact JSON text.
///
/// Transformation:
/// - Exercises the pure dispatch bridge for non-mutating JSON builder
///   operations.
#[test]
fn dispatch_json_builder_constructors_return_json_values() {
    let Some(SafeNativeValue::Json(value)) = dispatch_ok("std.data.json.null", &[]) else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("null")));

    let Some(SafeNativeValue::Json(value)) =
        dispatch_ok("std.data.json.bool", &[SafeNativeValue::Bool(true)])
    else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("true")));

    let Some(SafeNativeValue::Json(value)) =
        dispatch_ok("std.data.json.int", &[SafeNativeValue::Int(3)])
    else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("3")));

    let Some(SafeNativeValue::Json(value)) = dispatch_ok(
        "std.data.json.string",
        &[SafeNativeValue::Text(String::from("Ada"))],
    ) else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from(r#""Ada""#)));
}

/// Validates manifest-backed dispatch arity rejects unsupported operations.
///
/// Inputs:
/// - Operation id absent from the manifest.
///
/// Output:
/// - Test passes when the dispatch table returns `None`.
///
/// Transformation:
/// - Guards the negative branch of the manifest-backed arity table.
#[test]
fn operation_arity_rejects_non_manifest_operation() {
    assert_eq!(operation_arity("std.nope.missing"), None);
}

/// Validates bridge JSON operations use opaque handles.
///
/// Inputs:
/// - JSON source text, an object key, and a bridge resource store.
///
/// Output:
/// - Test passes when parse/get return handles and accessor returns text.
///
/// Transformation:
/// - Exercises resource-backed dispatch without exposing Rust `Json`
///   values across the bridge-facing API.
#[test]
fn bridge_dispatch_json_returns_and_accepts_handles() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[SafeNativeBridgeValue::Text(String::from(
            r#"{"name":"Ada"}"#,
        ))],
    ) else {
        return;
    };
    let Some(SafeNativeBridgeValue::Handle(name)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.get",
        &[
            SafeNativeBridgeValue::Handle(root),
            SafeNativeBridgeValue::Text(String::from("name")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.as_string",
            &[SafeNativeBridgeValue::Handle(name)]
        ),
        Ok(SafeNativeBridgeValue::Text(String::from("Ada")))
    );
}

/// Validates bridge JSON array operations use opaque handles.
///
/// Inputs:
/// - JSON array source text, an index, and a bridge resource store.
///
/// Output:
/// - Test passes when length returns an integer and indexed lookup returns a
///   handle accepted by typed accessors.
///
/// Transformation:
/// - Exercises resource-backed dispatch for JSON array reads without exposing
///   backend JSON values over the bridge-facing API.
#[test]
fn bridge_dispatch_json_array_length_and_at_use_handles() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[SafeNativeBridgeValue::Text(String::from(r#"["Ada",3]"#))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.length",
            &[SafeNativeBridgeValue::Handle(root)]
        ),
        Ok(SafeNativeBridgeValue::Int(2))
    );

    let Some(SafeNativeBridgeValue::Handle(name)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.at",
        &[
            SafeNativeBridgeValue::Handle(root),
            SafeNativeBridgeValue::Int(0),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.as_string",
            &[SafeNativeBridgeValue::Handle(name)]
        ),
        Ok(SafeNativeBridgeValue::Text(String::from("Ada")))
    );
}

/// Validates direct HTTP dispatch over request and response operations.
///
/// Inputs:
/// - Rust-native request and JSON values wrapped as neutral dispatch values.
///
/// Output:
/// - Test passes when body JSON parsing returns a JSON value and response
///   builders return HTTP response values.
///
/// Transformation:
/// - Exercises the SafeNative HTTP dispatch branches without crossing the
///   resource-handle bridge.
#[test]
fn dispatch_http_request_and_response_operations_return_native_values() {
    let request = http::Request::new(r#"{"name":"Ada"}"#);
    let Some(SafeNativeValue::Json(parsed)) = dispatch_ok(
        "std.http.request.body_json",
        &[SafeNativeValue::HttpRequest(request)],
    ) else {
        return;
    };
    let name = json::get(&parsed, "name")
        .and_then(|value| json::as_string(&value))
        .unwrap_or_else(|_| String::new());

    assert_eq!(name, "Ada");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.json",
        &[SafeNativeValue::Json(json::r#bool(true))],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "true");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.text",
        &[SafeNativeValue::Text(String::from("ok"))],
    ) else {
        return;
    };
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "ok");
}

/// Validates bridge HTTP dispatch stores request and response handles.
///
/// Inputs:
/// - Resource store containing an HTTP request value.
///
/// Output:
/// - Test passes when request parsing returns a JSON handle and response
///   construction returns an HTTP response handle.
///
/// Transformation:
/// - Exercises the resource-backed HTTP bridge path that server adapters can
///   use without exposing Rust HTTP values directly to BEAM terms.
#[test]
fn bridge_dispatch_http_request_and_response_operations_use_handles() {
    let mut store = ResourceStore::new();
    let request = store
        .insert(ResourceValue::HttpRequest(http::Request::new(
            r#"{"name":"Ada"}"#,
        )))
        .ok();
    let Some(request) = request else {
        return;
    };

    let Some(SafeNativeBridgeValue::Handle(parsed)) = bridge_dispatch_ok(
        &mut store,
        "std.http.request.body_json",
        &[SafeNativeBridgeValue::Handle(request)],
    ) else {
        return;
    };
    let Some(SafeNativeBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.json",
        &[SafeNativeBridgeValue::Handle(parsed)],
    ) else {
        return;
    };

    let response = store.http_response(response).ok();
    let Some(response) = response else {
        return;
    };
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), r#"{"name":"Ada"}"#);
}

/// Validates bridge path operations use opaque handles.
///
/// Inputs:
/// - Path source text and child segment.
///
/// Output:
/// - Test passes when path outputs are handles and component access returns
///   optional text.
///
/// Transformation:
/// - Exercises resource-backed path parse/join/file-name dispatch.
#[test]
fn bridge_dispatch_path_returns_and_accepts_handles() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(base)) = bridge_dispatch_ok(
        &mut store,
        "std.io.path.from_string",
        &[SafeNativeBridgeValue::Text(String::from("src"))],
    ) else {
        return;
    };
    let Some(SafeNativeBridgeValue::Handle(joined)) = bridge_dispatch_ok(
        &mut store,
        "std.io.path.join",
        &[
            SafeNativeBridgeValue::Handle(base),
            SafeNativeBridgeValue::Text(String::from("main.terl")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.io.path.file_name",
            &[SafeNativeBridgeValue::Handle(joined)]
        ),
        Ok(SafeNativeBridgeValue::OptionalText(Some(String::from(
            "main.terl"
        ))))
    );
}

/// Validates bridge URI operations use opaque handles.
///
/// Inputs:
/// - URI source text.
///
/// Output:
/// - Test passes when parse returns a handle and component access accepts
///   that handle.
///
/// Transformation:
/// - Exercises resource-backed URI parse and component dispatch.
#[test]
fn bridge_dispatch_uri_returns_and_accepts_handles() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(uri)) = bridge_dispatch_ok(
        &mut store,
        "std.net.uri.parse",
        &[SafeNativeBridgeValue::Text(String::from(
            "https://example.com/docs",
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.net.uri.host",
            &[SafeNativeBridgeValue::Handle(uri)]
        ),
        Ok(SafeNativeBridgeValue::OptionalText(Some(String::from(
            "example.com"
        ))))
    );
}

/// Validates bridge dispatch rejects stale resource handles.
///
/// Inputs:
/// - JSON parse output handle that is disposed before use.
///
/// Output:
/// - Test passes when later accessor dispatch returns `resource.stale_handle`.
///
/// Transformation:
/// - Exercises resource liveness before adapter invocation.
#[test]
fn bridge_dispatch_rejects_stale_handle_with_stable_error_code() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[SafeNativeBridgeValue::Text(String::from("null"))],
    ) else {
        return;
    };
    assert_eq!(store.dispose(root), Ok(()));

    let error = dispatch_with_resources(
        &mut store,
        "std.data.json.is_null",
        &[SafeNativeBridgeValue::Handle(root)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "resource.stale_handle");
}

/// Validates JSON parse, object lookup, and string accessor dispatch.
///
/// Inputs:
/// - JSON source text and object key.
///
/// Output:
/// - Test passes when dispatcher chains through JSON adapter functions.
///
/// Transformation:
/// - Exercises JSON operations through operation ids rather than direct
///   adapter calls.
#[test]
fn dispatches_json_parse_get_and_as_string() {
    let Some(SafeNativeValue::Json(root)) = dispatch_ok(
        "std.data.json.parse",
        &[SafeNativeValue::Text(String::from(r#"{"name":"Ada"}"#))],
    ) else {
        return;
    };
    let Some(SafeNativeValue::Json(name)) = dispatch_ok(
        "std.data.json.get",
        &[
            SafeNativeValue::Json(root),
            SafeNativeValue::Text(String::from("name")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch("std.data.json.as_string", &[SafeNativeValue::Json(name)]),
        Ok(SafeNativeValue::Text(String::from("Ada")))
    );
}

/// Validates Base64 dispatch over standard encode/decode operations.
///
/// Inputs:
/// - Plain UTF-8 text.
///
/// Output:
/// - Test passes when encode and decode preserve the text.
///
/// Transformation:
/// - Routes Base64 operations through the shared dispatcher.
#[test]
fn dispatches_base64_round_trip() {
    let Some(SafeNativeValue::Text(encoded)) = dispatch_ok(
        "std.encoding.base64.encode",
        &[SafeNativeValue::Text(String::from("hello Terlan"))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.encoding.base64.decode",
            &[SafeNativeValue::Text(encoded)]
        ),
        Ok(SafeNativeValue::Text(String::from("hello Terlan")))
    );
}

/// Validates lexical path dispatch over parse, join, and component access.
///
/// Inputs:
/// - Base path and child path text.
///
/// Output:
/// - Test passes when joined path exposes the expected final component.
///
/// Transformation:
/// - Routes path operations through the shared dispatcher.
#[test]
fn dispatches_path_join_and_file_name() {
    let Some(SafeNativeValue::Path(base)) = dispatch_ok(
        "std.io.path.from_string",
        &[SafeNativeValue::Text(String::from("src"))],
    ) else {
        return;
    };
    let Some(SafeNativeValue::Path(joined)) = dispatch_ok(
        "std.io.path.join",
        &[
            SafeNativeValue::Path(base),
            SafeNativeValue::Text(String::from("main.terl")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch("std.io.path.file_name", &[SafeNativeValue::Path(joined)]),
        Ok(SafeNativeValue::OptionalText(Some(String::from(
            "main.terl"
        ))))
    );
}

/// Validates URI dispatch over parse and component accessors.
///
/// Inputs:
/// - HTTPS URI source text.
///
/// Output:
/// - Test passes when component accessors return stable values.
///
/// Transformation:
/// - Routes URI operations through the shared dispatcher.
#[test]
fn dispatches_uri_components() {
    let Some(SafeNativeValue::Uri(uri)) = dispatch_ok(
        "std.net.uri.parse",
        &[SafeNativeValue::Text(String::from(
            "https://example.com/docs?q=terlan",
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch("std.net.uri.scheme", &[SafeNativeValue::Uri(uri.clone())]),
        Ok(SafeNativeValue::Text(String::from("https")))
    );
    assert_eq!(
        dispatch("std.net.uri.host", &[SafeNativeValue::Uri(uri)]),
        Ok(SafeNativeValue::OptionalText(Some(String::from(
            "example.com"
        ))))
    );
}

/// Validates stable wrong-arity errors.
///
/// Inputs:
/// - Operation id with no supplied arguments.
///
/// Output:
/// - Test passes when the error uses `dispatch.arity`.
///
/// Transformation:
/// - Exercises the dispatcher argument-count guard before adapter calls.
#[test]
fn rejects_wrong_arity_with_stable_error_code() {
    let error = dispatch("std.data.json.parse", &[])
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "dispatch.arity");
}

/// Validates stable wrong-type errors.
///
/// Inputs:
/// - JSON accessor with a text value instead of a JSON value.
///
/// Output:
/// - Test passes when the error uses `dispatch.type`.
///
/// Transformation:
/// - Exercises runtime argument shape validation before adapter calls.
#[test]
fn rejects_wrong_type_with_stable_error_code() {
    let error = dispatch(
        "std.data.json.as_string",
        &[SafeNativeValue::Text(String::from("not json"))],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "dispatch.type");
}

/// Validates stable unknown-operation errors.
///
/// Inputs:
/// - Unsupported operation id.
///
/// Output:
/// - Test passes when the error uses `dispatch.unknown_operation`.
///
/// Transformation:
/// - Exercises dispatch-table miss handling.
#[test]
fn rejects_unknown_operation_with_stable_error_code() {
    let error = dispatch("std.unknown.nope", &[])
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "dispatch.unknown_operation");
}
