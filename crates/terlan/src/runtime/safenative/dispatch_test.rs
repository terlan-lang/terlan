use super::*;
use crate::terlan_safenative::resource::{ResourceStore, ResourceValue};

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
    include_str!("../../../../../std/RUST_BACKED_MANIFEST.tsv")
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
    assert_eq!(operations.len(), 79);

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

/// Validates native vector bridge operations allocate and mutate resources.
///
/// Inputs:
/// - A bridge list of text values and one resource store.
///
/// Output:
/// - Test passes when vector operations return stable handles and indexed
///   reads observe mutations.
///
/// Transformation:
/// - Exercises the SafeNative resource dispatch path used by
///   `std.native.collections.Vector` without exposing Rust vectors across the
///   bridge boundary.
#[test]
fn bridge_dispatch_native_vector_allocates_and_mutates_handle() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(vector)) = bridge_dispatch_ok(
        &mut store,
        "std.native.collections.vector.from_list",
        &[SafeNativeBridgeValue::List(vec![
            SafeNativeBridgeValue::Text(String::from("Ada")),
            SafeNativeBridgeValue::Text(String::from("Grace")),
        ])],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.length",
            &[SafeNativeBridgeValue::Handle(vector)]
        ),
        Ok(SafeNativeBridgeValue::Int(2))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.get_at",
            &[
                SafeNativeBridgeValue::Handle(vector),
                SafeNativeBridgeValue::Int(1)
            ]
        ),
        Ok(SafeNativeBridgeValue::Text(String::from("Grace")))
    );

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.set_at",
            &[
                SafeNativeBridgeValue::Handle(vector),
                SafeNativeBridgeValue::Int(1),
                SafeNativeBridgeValue::Text(String::from("Carol"))
            ]
        ),
        Ok(SafeNativeBridgeValue::Handle(vector))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.get_at",
            &[
                SafeNativeBridgeValue::Handle(vector),
                SafeNativeBridgeValue::Int(1)
            ]
        ),
        Ok(SafeNativeBridgeValue::Text(String::from("Carol")))
    );
}

/// Verifies SafeNative bridge dispatch rejects cross-resource handle confusion.
///
/// Inputs:
/// - A Vector resource handle passed to a JSON bridge accessor.
///
/// Output:
/// - Test passes when dispatch returns the stable `resource.kind` error code.
///
/// Transformation:
/// - Exercises an adversarial bridge call where the handle is live and valid
///   but points at the wrong resource domain.
#[test]
fn adversarial_safenative_dispatch_rejects_cross_resource_handle_confusion() {
    let mut store = ResourceStore::new();
    let Some(SafeNativeBridgeValue::Handle(vector)) = bridge_dispatch_ok(
        &mut store,
        "std.native.collections.vector.from_list",
        &[SafeNativeBridgeValue::List(vec![
            SafeNativeBridgeValue::Int(1),
            SafeNativeBridgeValue::Int(2),
        ])],
    ) else {
        return;
    };

    let error = dispatch_with_resources(
        &mut store,
        "std.data.json.as_string",
        &[SafeNativeBridgeValue::Handle(vector)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "resource.kind");
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
    let request = http::Request::from_parts_with_metadata(
        "GET",
        "/users/42",
        r#"{"name":"Ada"}"#,
        vec![("id".to_string(), "42".to_string())],
        vec![("tab".to_string(), "profile".to_string())],
        vec![("theme".to_string(), "dark".to_string())],
    );
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

    let request = http::Request::from_parts_with_raw_query_metadata(
        "GET",
        "/users/42",
        "raw body",
        vec![("id".to_string(), "42".to_string())],
        "tab=profile",
        vec![("tab".to_string(), "profile".to_string())],
        vec![("Accept".to_string(), "application/json".to_string())],
        vec![("theme".to_string(), "dark".to_string())],
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.body_text",
            &[SafeNativeValue::HttpRequest(request.clone())],
        ),
        Some(SafeNativeValue::Text("raw body".to_string()))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.method",
            &[SafeNativeValue::HttpRequest(request.clone())],
        ),
        Some(SafeNativeValue::Text("GET".to_string()))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.path",
            &[SafeNativeValue::HttpRequest(request.clone())],
        ),
        Some(SafeNativeValue::Text("/users/42".to_string()))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.param",
            &[
                SafeNativeValue::HttpRequest(request.clone()),
                SafeNativeValue::Text("id".to_string()),
            ],
        ),
        Some(SafeNativeValue::OptionalText(Some("42".to_string())))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.query",
            &[
                SafeNativeValue::HttpRequest(request.clone()),
                SafeNativeValue::Text("tab".to_string()),
            ],
        ),
        Some(SafeNativeValue::OptionalText(Some("profile".to_string())))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.query_string",
            &[SafeNativeValue::HttpRequest(request.clone())],
        ),
        Some(SafeNativeValue::Text("tab=profile".to_string()))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.header",
            &[
                SafeNativeValue::HttpRequest(request.clone()),
                SafeNativeValue::Text("accept".to_string()),
            ],
        ),
        Some(SafeNativeValue::OptionalText(Some(
            "application/json".to_string()
        )))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.request.cookie",
            &[
                SafeNativeValue::HttpRequest(request),
                SafeNativeValue::Text("theme".to_string()),
            ],
        ),
        Some(SafeNativeValue::OptionalText(Some("dark".to_string())))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.set_header",
            &[
                SafeNativeValue::Text("session".to_string()),
                SafeNativeValue::Text("abc123".to_string()),
                SafeNativeValue::Text("/".to_string()),
                SafeNativeValue::Bool(true),
                SafeNativeValue::Bool(false),
            ],
        ),
        Some(SafeNativeValue::Text(
            "session=abc123; HttpOnly; Path=/".to_string()
        ))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.set_header_with_options",
            &[
                SafeNativeValue::Text("session".to_string()),
                SafeNativeValue::Text("abc123".to_string()),
                SafeNativeValue::Text("/account".to_string()),
                SafeNativeValue::Text("example.com".to_string()),
                SafeNativeValue::Int(3600),
                SafeNativeValue::Bool(true),
                SafeNativeValue::Text("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
                SafeNativeValue::Bool(true),
                SafeNativeValue::Bool(true),
                SafeNativeValue::Text("strict".to_string()),
            ],
        ),
        Some(SafeNativeValue::Text(
            "session=abc123; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT".to_string()
        ))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.delete_header",
            &[
                SafeNativeValue::Text("session".to_string()),
                SafeNativeValue::Text("/".to_string()),
            ],
        ),
        Some(SafeNativeValue::Text(
            "session=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string()
        ))
    );

    let request = http::Request::from_parts_with_metadata(
        "GET",
        "/profile",
        "",
        Vec::new(),
        Vec::new(),
        vec![("theme".to_string(), "dark".to_string())],
    );
    let Some(SafeNativeValue::HttpCookieJar(jar)) = dispatch_ok(
        "std.http.request.cookies",
        &[SafeNativeValue::HttpRequest(request)],
    ) else {
        return;
    };
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.get",
            &[
                SafeNativeValue::HttpCookieJar(jar),
                SafeNativeValue::Text("theme".to_string()),
            ],
        ),
        Some(SafeNativeValue::OptionalText(Some("dark".to_string())))
    );

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.json",
        &[
            SafeNativeValue::Json(json::r#bool(true)),
            SafeNativeValue::Int(200),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "true");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.json_text",
        &[
            SafeNativeValue::Text(String::from("{\"ok\":true}")),
            SafeNativeValue::Int(200),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "{\"ok\":true}");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.text",
        &[
            SafeNativeValue::Text(String::from("ok")),
            SafeNativeValue::Int(201),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 201);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "ok");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.html",
        &[
            SafeNativeValue::Text(String::from("<main>ok</main>")),
            SafeNativeValue::Int(202),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 202);
    assert_eq!(response.content_type(), "text/html; charset=utf-8");
    assert_eq!(response.body(), "<main>ok</main>");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.file",
        &[
            SafeNativeValue::Text(String::from("downloads/report.txt")),
            SafeNativeValue::Int(206),
            SafeNativeValue::Text(String::from("text/plain; charset=utf-8")),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 206);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.file_path(), Some("downloads/report.txt"));
    assert_eq!(response.body(), "");

    let Some(SafeNativeValue::HttpResponse(response)) = dispatch_ok(
        "std.http.response.redirect",
        &[
            SafeNativeValue::Text(String::from("/login")),
            SafeNativeValue::Int(301),
        ],
    ) else {
        return;
    };
    assert_eq!(response.status_code(), 301);
    assert_eq!(
        response.headers(),
        &[("Location".to_string(), "/login".to_string())]
    );
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
        .insert(ResourceValue::HttpRequest(
            http::Request::from_parts_with_raw_query_metadata(
                "GET",
                "/users/42",
                r#"{"name":"Ada"}"#,
                vec![("id".to_string(), "42".to_string())],
                "tab=profile",
                vec![("tab".to_string(), "profile".to_string())],
                vec![("Accept".to_string(), "application/json".to_string())],
                vec![("theme".to_string(), "dark".to_string())],
            ),
        ))
        .ok();
    let Some(request) = request else {
        return;
    };

    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.body_text",
            &[SafeNativeBridgeValue::Handle(request)],
        ),
        Some(SafeNativeBridgeValue::Text(r#"{"name":"Ada"}"#.to_string()))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.param",
            &[
                SafeNativeBridgeValue::Handle(request),
                SafeNativeBridgeValue::Text("id".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::OptionalText(Some("42".to_string())))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.query",
            &[
                SafeNativeBridgeValue::Handle(request),
                SafeNativeBridgeValue::Text("tab".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::OptionalText(Some(
            "profile".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.query_string",
            &[SafeNativeBridgeValue::Handle(request)],
        ),
        Some(SafeNativeBridgeValue::Text("tab=profile".to_string()))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.header",
            &[
                SafeNativeBridgeValue::Handle(request),
                SafeNativeBridgeValue::Text("ACCEPT".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::OptionalText(Some(
            "application/json".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.cookie",
            &[
                SafeNativeBridgeValue::Handle(request),
                SafeNativeBridgeValue::Text("theme".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::OptionalText(Some(
            "dark".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set_header",
            &[
                SafeNativeBridgeValue::Text("session".to_string()),
                SafeNativeBridgeValue::Text("abc123".to_string()),
                SafeNativeBridgeValue::Text("/".to_string()),
                SafeNativeBridgeValue::Bool(true),
                SafeNativeBridgeValue::Bool(true),
            ],
        ),
        Some(SafeNativeBridgeValue::Text(
            "session=abc123; HttpOnly; Secure; Path=/".to_string()
        ))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set_header_with_options",
            &[
                SafeNativeBridgeValue::Text("session".to_string()),
                SafeNativeBridgeValue::Text("abc123".to_string()),
                SafeNativeBridgeValue::Text("/account".to_string()),
                SafeNativeBridgeValue::Text("example.com".to_string()),
                SafeNativeBridgeValue::Int(3600),
                SafeNativeBridgeValue::Bool(true),
                SafeNativeBridgeValue::Text("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
                SafeNativeBridgeValue::Bool(true),
                SafeNativeBridgeValue::Bool(true),
                SafeNativeBridgeValue::Text("lax".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::Text(
            "session=abc123; HttpOnly; SameSite=Lax; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT".to_string()
        ))
    );

    let Some(SafeNativeBridgeValue::Handle(jar)) = bridge_dispatch_ok(
        &mut store,
        "std.http.request.cookies",
        &[SafeNativeBridgeValue::Handle(request)],
    ) else {
        return;
    };
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.get",
            &[
                SafeNativeBridgeValue::Handle(jar),
                SafeNativeBridgeValue::Text("theme".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::OptionalText(Some(
            "dark".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set",
            &[
                SafeNativeBridgeValue::Handle(jar),
                SafeNativeBridgeValue::Text("session".to_string()),
                SafeNativeBridgeValue::Text("abc123".to_string()),
                SafeNativeBridgeValue::Text("/".to_string()),
                SafeNativeBridgeValue::Bool(true),
                SafeNativeBridgeValue::Bool(false),
            ],
        ),
        Some(SafeNativeBridgeValue::Unit)
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.delete",
            &[
                SafeNativeBridgeValue::Handle(jar),
                SafeNativeBridgeValue::Text("theme".to_string()),
                SafeNativeBridgeValue::Text("/".to_string()),
            ],
        ),
        Some(SafeNativeBridgeValue::Unit)
    );
    let Some(cookie_jar) = store.http_cookie_jar(jar).ok() else {
        return;
    };
    assert_eq!(
        cookie_jar.mutations(),
        &[
            "session=abc123; HttpOnly; Path=/".to_string(),
            "theme=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string(),
        ]
    );

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
        &[
            SafeNativeBridgeValue::Handle(parsed),
            SafeNativeBridgeValue::Int(200),
        ],
    ) else {
        return;
    };

    let response = store.http_response(response).ok();
    let Some(response) = response else {
        return;
    };
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), r#"{"name":"Ada"}"#);

    let Some(SafeNativeBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.html",
        &[
            SafeNativeBridgeValue::Text("<main>ok</main>".to_string()),
            SafeNativeBridgeValue::Int(200),
        ],
    ) else {
        return;
    };
    let Some(response) = store.http_response(response).ok() else {
        return;
    };
    assert_eq!(response.content_type(), "text/html; charset=utf-8");
    assert_eq!(response.body(), "<main>ok</main>");

    let Some(SafeNativeBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.redirect",
        &[
            SafeNativeBridgeValue::Text("/login".to_string()),
            SafeNativeBridgeValue::Int(302),
        ],
    ) else {
        return;
    };
    let Some(response) = store.http_response(response).ok() else {
        return;
    };
    assert_eq!(response.status_code(), 302);
    assert_eq!(
        response.headers(),
        &[("Location".to_string(), "/login".to_string())]
    );
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

/// Validates bridge dispatch stores and reuses Postgres row handles.
///
/// Inputs:
/// - A Postgres row fixture inserted as an opaque runtime resource.
///
/// Output:
/// - Test passes when row accessors decode through a bridge handle and return
///   stable primitive values.
///
/// Transformation:
/// - Exercises the non-live Postgres resource path used after live query
///   operations return rows to handler code.
#[test]
fn bridge_dispatch_postgres_row_handles_decode_values() {
    let mut store = ResourceStore::new();
    let mut row = postgres::Row::new();
    row.put_string("status", "postgres-ok");
    row.put_int("count", 1);
    row.put_bool("healthy", true);
    let Some(row) = store.insert(ResourceValue::PostgresRow(row)).ok() else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.db.postgres.string",
            &[
                SafeNativeBridgeValue::Handle(row),
                SafeNativeBridgeValue::Text(String::from("status")),
            ],
        ),
        Ok(SafeNativeBridgeValue::Text(String::from("postgres-ok")))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.db.postgres.int",
            &[
                SafeNativeBridgeValue::Handle(row),
                SafeNativeBridgeValue::Text(String::from("count")),
            ],
        ),
        Ok(SafeNativeBridgeValue::Int(1))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.db.postgres.bool",
            &[
                SafeNativeBridgeValue::Handle(row),
                SafeNativeBridgeValue::Text(String::from("healthy")),
            ],
        ),
        Ok(SafeNativeBridgeValue::Bool(true))
    );
}

/// Validates bridge dispatch stores Postgres query rows as handles.
///
/// Inputs:
/// - A disconnected Postgres pool fixture and query arguments.
///
/// Output:
/// - Test passes when pool handles are accepted by query operations and reach
///   the stable adapter error instead of failing as resource type errors.
///
/// Transformation:
/// - Exercises the non-live pool handle path used by handler code before the
///   maintained client reports that no database connection is available.
#[test]
fn bridge_dispatch_postgres_pool_handles_reach_query_adapter() {
    let mut store = ResourceStore::new();
    let Some(pool) = store
        .insert(ResourceValue::PostgresPool(postgres::Pool::disconnected(
            "postgres://127.0.0.1:1/terlan",
        )))
        .ok()
    else {
        return;
    };

    let error = dispatch_with_resources(
        &mut store,
        "std.db.postgres.query_one",
        &[
            SafeNativeBridgeValue::Handle(pool),
            SafeNativeBridgeValue::Text(String::from("SELECT 1::BIGINT AS value")),
            SafeNativeBridgeValue::List(Vec::new()),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "postgres.connect");
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

/// Validates Postgres config dispatch reaches stable adapter errors.
///
/// Inputs:
/// - Valid and invalid Postgres config values.
///
/// Output:
/// - Test passes when invalid URLs preserve `postgres.invalid_url` and valid
///   but unreachable configs reach the stable maintained-driver boundary.
///
/// Transformation:
/// - Exercises the Postgres operation dispatch path without requiring a live
///   database.
#[test]
fn dispatch_postgres_connect_preserves_adapter_error_codes() {
    let invalid = postgres::Config::new("mysql://localhost/terlan");
    let error = dispatch(
        "std.db.postgres.connect",
        &[SafeNativeValue::PostgresConfig(invalid)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.invalid_url");

    let valid = postgres::Config::new("postgres://127.0.0.1:1/terlan");
    let error = dispatch(
        "std.db.postgres.connect",
        &[SafeNativeValue::PostgresConfig(valid)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.connect");
}

/// Validates Postgres query dispatch uses known operation errors.
///
/// Inputs:
/// - Disconnected pool placeholder, SQL text, and empty JSON parameters.
///
/// Output:
/// - Test passes when query operations return stable maintained-driver
///   connection errors rather than falling through as unknown operations.
///
/// Transformation:
/// - Locks the dispatch contract against the maintained Rust/Tokio adapter
///   without requiring a live database in ordinary unit tests.
#[test]
fn dispatch_postgres_query_operations_are_known_driver_operations() {
    let pool = postgres::Pool::disconnected("postgres://127.0.0.1:1/terlan");
    let params = SafeNativeValue::JsonList(Vec::new());

    let error = dispatch(
        "std.db.postgres.query",
        &[
            SafeNativeValue::PostgresPool(pool.clone()),
            SafeNativeValue::Text(String::from("SELECT 1")),
            params.clone(),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.connect");

    let error = dispatch(
        "std.db.postgres.query_one",
        &[
            SafeNativeValue::PostgresPool(pool.clone()),
            SafeNativeValue::Text(String::from("SELECT 1 LIMIT 1")),
            params.clone(),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.connect");

    let error = dispatch(
        "std.db.postgres.execute",
        &[
            SafeNativeValue::PostgresPool(pool),
            SafeNativeValue::Text(String::from("CREATE TABLE users(id BIGINT)")),
            params,
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.connect");
}

/// Validates Postgres transaction dispatch is runtime-bridge gated.
///
/// Inputs:
/// - Disconnected pool placeholder and a stand-in callback argument.
///
/// Output:
/// - Test passes when transaction dispatch reports the required runtime bridge.
///
/// Transformation:
/// - Keeps callback-shaped transaction execution out of pure dispatch until
///   the worker protocol can represent callbacks explicitly.
#[test]
fn dispatch_postgres_transaction_requires_runtime_bridge() {
    let pool = postgres::Pool::disconnected("postgres://localhost/terlan");

    let error = dispatch(
        "std.db.postgres.transaction",
        &[SafeNativeValue::PostgresPool(pool), SafeNativeValue::Unit],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "dispatch.callback_requires_runtime_bridge");
}

/// Validates Postgres row accessors through pure dispatch.
///
/// Inputs:
/// - Row fixture with string, integer, boolean, and JSON columns.
///
/// Output:
/// - Test passes when row accessors decode expected values through operation
///   ids and preserve row errors for bad lookups.
///
/// Transformation:
/// - Exercises the row-decoding dispatch layer independently from a live
///   database client.
#[test]
fn dispatch_postgres_row_accessors_decode_values() {
    let mut row = postgres::Row::new();
    row.put_string("name", "Ada");
    row.put_int("age", 42);
    row.put_bool("active", true);
    row.put_json("meta", json::string("ok"));

    assert_eq!(
        dispatch(
            "std.db.postgres.string",
            &[
                SafeNativeValue::PostgresRow(row.clone()),
                SafeNativeValue::Text(String::from("name")),
            ],
        ),
        Ok(SafeNativeValue::Text(String::from("Ada")))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.int",
            &[
                SafeNativeValue::PostgresRow(row.clone()),
                SafeNativeValue::Text(String::from("age")),
            ],
        ),
        Ok(SafeNativeValue::Int(42))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.bool",
            &[
                SafeNativeValue::PostgresRow(row.clone()),
                SafeNativeValue::Text(String::from("active")),
            ],
        ),
        Ok(SafeNativeValue::Bool(true))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.json",
            &[
                SafeNativeValue::PostgresRow(row.clone()),
                SafeNativeValue::Text(String::from("meta")),
            ],
        ),
        Ok(SafeNativeValue::Json(json::string("ok")))
    );

    let error = dispatch(
        "std.db.postgres.string",
        &[
            SafeNativeValue::PostgresRow(row),
            SafeNativeValue::Text(String::from("missing")),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.row.missing_column");
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
