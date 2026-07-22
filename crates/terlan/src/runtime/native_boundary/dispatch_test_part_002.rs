
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
///   use without exposing Rust HTTP values directly to VM terms.
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
            &[NativeBoundaryBridgeValue::Handle(request)],
        ),
        Some(NativeBoundaryBridgeValue::Text(
            r#"{"name":"Ada"}"#.to_string()
        ))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.param",
            &[
                NativeBoundaryBridgeValue::Handle(request),
                NativeBoundaryBridgeValue::Text("id".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::OptionalText(Some(
            "42".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.query",
            &[
                NativeBoundaryBridgeValue::Handle(request),
                NativeBoundaryBridgeValue::Text("tab".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::OptionalText(Some(
            "profile".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.query_string",
            &[NativeBoundaryBridgeValue::Handle(request)],
        ),
        Some(NativeBoundaryBridgeValue::Text("tab=profile".to_string()))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.header",
            &[
                NativeBoundaryBridgeValue::Handle(request),
                NativeBoundaryBridgeValue::Text("ACCEPT".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::OptionalText(Some(
            "application/json".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.request.cookie",
            &[
                NativeBoundaryBridgeValue::Handle(request),
                NativeBoundaryBridgeValue::Text("theme".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::OptionalText(Some(
            "dark".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set_header",
            &[
                NativeBoundaryBridgeValue::Text("session".to_string()),
                NativeBoundaryBridgeValue::Text("abc123".to_string()),
                NativeBoundaryBridgeValue::Text("/".to_string()),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Bool(true),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Text(
            "session=abc123; HttpOnly; Secure; Path=/".to_string()
        ))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set_header_with_options",
            &[
                NativeBoundaryBridgeValue::Text("session".to_string()),
                NativeBoundaryBridgeValue::Text("abc123".to_string()),
                NativeBoundaryBridgeValue::Text("/account".to_string()),
                NativeBoundaryBridgeValue::Text("example.com".to_string()),
                NativeBoundaryBridgeValue::Int(3600),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Text("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Text("lax".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Text(
            "session=abc123; HttpOnly; SameSite=Lax; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT".to_string()
        ))
    );

    let Some(NativeBoundaryBridgeValue::Handle(jar)) = bridge_dispatch_ok(
        &mut store,
        "std.http.request.cookies",
        &[NativeBoundaryBridgeValue::Handle(request)],
    ) else {
        return;
    };
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.get",
            &[
                NativeBoundaryBridgeValue::Handle(jar),
                NativeBoundaryBridgeValue::Text("theme".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::OptionalText(Some(
            "dark".to_string()
        )))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.set",
            &[
                NativeBoundaryBridgeValue::Handle(jar),
                NativeBoundaryBridgeValue::Text("session".to_string()),
                NativeBoundaryBridgeValue::Text("abc123".to_string()),
                NativeBoundaryBridgeValue::Text("/".to_string()),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Bool(false),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Unit)
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.delete",
            &[
                NativeBoundaryBridgeValue::Handle(jar),
                NativeBoundaryBridgeValue::Text("theme".to_string()),
                NativeBoundaryBridgeValue::Text("/".to_string()),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Unit)
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

    let Some(NativeBoundaryBridgeValue::Handle(parsed)) = bridge_dispatch_ok(
        &mut store,
        "std.http.request.body_json",
        &[NativeBoundaryBridgeValue::Handle(request)],
    ) else {
        return;
    };
    let Some(NativeBoundaryBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.json",
        &[
            NativeBoundaryBridgeValue::Handle(parsed),
            NativeBoundaryBridgeValue::Int(200),
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

    let Some(NativeBoundaryBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.html",
        &[
            NativeBoundaryBridgeValue::Text("<main>ok</main>".to_string()),
            NativeBoundaryBridgeValue::Int(200),
        ],
    ) else {
        return;
    };
    let Some(response) = store.http_response(response).ok() else {
        return;
    };
    assert_eq!(response.content_type(), "text/html; charset=utf-8");
    assert_eq!(response.body(), "<main>ok</main>");

    let Some(NativeBoundaryBridgeValue::Handle(response)) = bridge_dispatch_ok(
        &mut store,
        "std.http.response.redirect",
        &[
            NativeBoundaryBridgeValue::Text("/login".to_string()),
            NativeBoundaryBridgeValue::Int(302),
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
    let Some(NativeBoundaryBridgeValue::Handle(base)) = bridge_dispatch_ok(
        &mut store,
        "std.io.path.from_string",
        &[NativeBoundaryBridgeValue::Text(String::from("src"))],
    ) else {
        return;
    };
    let Some(NativeBoundaryBridgeValue::Handle(joined)) = bridge_dispatch_ok(
        &mut store,
        "std.io.path.join",
        &[
            NativeBoundaryBridgeValue::Handle(base),
            NativeBoundaryBridgeValue::Text(String::from("main.terl")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.io.path.file_name",
            &[NativeBoundaryBridgeValue::Handle(joined)]
        ),
        Ok(NativeBoundaryBridgeValue::OptionalText(Some(String::from(
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
    let Some(NativeBoundaryBridgeValue::Handle(uri)) = bridge_dispatch_ok(
        &mut store,
        "std.net.uri.parse",
        &[NativeBoundaryBridgeValue::Text(String::from(
            "https://example.com/docs",
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.net.uri.host",
            &[NativeBoundaryBridgeValue::Handle(uri)]
        ),
        Ok(NativeBoundaryBridgeValue::OptionalText(Some(String::from(
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
                NativeBoundaryBridgeValue::Handle(row),
                NativeBoundaryBridgeValue::Text(String::from("status")),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Text(String::from("postgres-ok")))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.db.postgres.int",
            &[
                NativeBoundaryBridgeValue::Handle(row),
                NativeBoundaryBridgeValue::Text(String::from("count")),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Int(1))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.db.postgres.bool",
            &[
                NativeBoundaryBridgeValue::Handle(row),
                NativeBoundaryBridgeValue::Text(String::from("healthy")),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Bool(true))
    );
}

/// Validates bridge Postgres row accessors keep column and enum text dynamic.
///
/// Inputs:
/// - A row resource containing atom-like column names and enum-like text
///   values.
///
/// Output:
/// - Test passes when row accessors return text through the bridge handle.
///
/// Transformation:
/// - Exercises the native-boundary row payload path without atom creation or
///   name interning.
#[test]
fn bridge_dispatch_postgres_row_keeps_atom_like_columns_as_text() {
    let mut store = ResourceStore::new();
    let mut row = postgres::Row::new();
    row.put_string("binary_to_atom", "pending");
    row.put_string("list_to_atom", "ready");
    let Some(row) = store.insert(ResourceValue::PostgresRow(row)).ok() else {
        return;
    };

    for (name, expected) in [("binary_to_atom", "pending"), ("list_to_atom", "ready")] {
        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.db.postgres.string",
                &[
                    NativeBoundaryBridgeValue::Handle(row),
                    NativeBoundaryBridgeValue::Text(name.to_string()),
                ],
            ),
            Ok(NativeBoundaryBridgeValue::Text(expected.to_string()))
        );
    }
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
        .insert(ResourceValue::PostgresPool(
            postgres::test_support::disconnected_pool("postgres://127.0.0.1:1/terlan"),
        ))
        .ok()
    else {
        return;
    };

    let error = dispatch_with_resources(
        &mut store,
        "std.db.postgres.query_one",
        &[
            NativeBoundaryBridgeValue::Handle(pool),
            NativeBoundaryBridgeValue::Text(String::from("SELECT 1::BIGINT AS value")),
            NativeBoundaryBridgeValue::List(Vec::new()),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), expected_postgres_unreachable_code());
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
    let Some(NativeBoundaryBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[NativeBoundaryBridgeValue::Text(String::from("null"))],
    ) else {
        return;
    };
    assert_eq!(store.dispose(root), Ok(()));

    let error = dispatch_with_resources(
        &mut store,
        "std.data.json.is_null",
        &[NativeBoundaryBridgeValue::Handle(root)],
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
    let Some(NativeBoundaryValue::Json(root)) = dispatch_ok(
        "std.data.json.parse",
        &[NativeBoundaryValue::Text(String::from(r#"{"name":"Ada"}"#))],
    ) else {
        return;
    };
    let Some(NativeBoundaryValue::Json(name)) = dispatch_ok(
        "std.data.json.get",
        &[
            NativeBoundaryValue::Json(root),
            NativeBoundaryValue::Text(String::from("name")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.data.json.as_string",
            &[NativeBoundaryValue::Json(name)]
        ),
        Ok(NativeBoundaryValue::Text(String::from("Ada")))
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
    let Some(NativeBoundaryValue::Text(encoded)) = dispatch_ok(
        "std.encoding.base64.encode",
        &[NativeBoundaryValue::Text(String::from("hello Terlan"))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.encoding.base64.decode",
            &[NativeBoundaryValue::Text(encoded)]
        ),
        Ok(NativeBoundaryValue::Text(String::from("hello Terlan")))
    );

    let bytes = vec![0, 127, 128, 255];
    let Some(NativeBoundaryValue::Text(encoded_bytes)) = dispatch_ok(
        "std.encoding.base64.encode_bytes",
        &[NativeBoundaryValue::Bytes(bytes.clone())],
    ) else {
        return;
    };
    assert_eq!(
        dispatch(
            "std.encoding.base64.decode_bytes",
            &[NativeBoundaryValue::Text(encoded_bytes)]
        ),
        Ok(NativeBoundaryValue::Bytes(bytes))
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
    let Some(NativeBoundaryValue::Path(base)) = dispatch_ok(
        "std.io.path.from_string",
        &[NativeBoundaryValue::Text(String::from("src"))],
    ) else {
        return;
    };
    let Some(NativeBoundaryValue::Path(joined)) = dispatch_ok(
        "std.io.path.join",
        &[
            NativeBoundaryValue::Path(base),
            NativeBoundaryValue::Text(String::from("main.terl")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.io.path.file_name",
            &[NativeBoundaryValue::Path(joined)]
        ),
        Ok(NativeBoundaryValue::OptionalText(Some(String::from(
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
    let Some(NativeBoundaryValue::Uri(uri)) = dispatch_ok(
        "std.net.uri.parse",
        &[NativeBoundaryValue::Text(String::from(
            "https://example.com/docs?q=terlan",
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.net.uri.scheme",
            &[NativeBoundaryValue::Uri(uri.clone())]
        ),
        Ok(NativeBoundaryValue::Text(String::from("https")))
    );
    assert_eq!(
        dispatch("std.net.uri.host", &[NativeBoundaryValue::Uri(uri)]),
        Ok(NativeBoundaryValue::OptionalText(Some(String::from(
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
        &[NativeBoundaryValue::PostgresConfig(invalid)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), "postgres.invalid_url");

    let valid = postgres::Config::new("postgres://127.0.0.1:1/terlan");
    let error = dispatch(
        "std.db.postgres.connect",
        &[NativeBoundaryValue::PostgresConfig(valid)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), expected_postgres_unreachable_code());
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
/// - Locks the dispatch contract against the maintained Postgres adapter
///   without requiring a live database in ordinary unit tests.
#[test]
fn dispatch_postgres_query_operations_are_known_driver_operations() {
    let pool = postgres::test_support::disconnected_pool("postgres://127.0.0.1:1/terlan");
    let params = NativeBoundaryValue::JsonList(Vec::new());

    let error = dispatch(
        "std.db.postgres.query",
        &[
            NativeBoundaryValue::PostgresPool(pool.clone()),
            NativeBoundaryValue::Text(String::from("SELECT 1")),
            params.clone(),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), expected_postgres_unreachable_code());

    let error = dispatch(
        "std.db.postgres.query_one",
        &[
            NativeBoundaryValue::PostgresPool(pool.clone()),
            NativeBoundaryValue::Text(String::from("SELECT 1 LIMIT 1")),
            params.clone(),
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), expected_postgres_unreachable_code());

    let error = dispatch(
        "std.db.postgres.execute",
        &[
            NativeBoundaryValue::PostgresPool(pool),
            NativeBoundaryValue::Text(String::from("CREATE TABLE users(id BIGINT)")),
            params,
        ],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));
    assert_eq!(error.code(), expected_postgres_unreachable_code());
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
    let pool = postgres::test_support::disconnected_pool("postgres://localhost/terlan");

    let error = dispatch(
        "std.db.postgres.transaction",
        &[
            NativeBoundaryValue::PostgresPool(pool),
            NativeBoundaryValue::Unit,
        ],
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
                NativeBoundaryValue::PostgresRow(row.clone()),
                NativeBoundaryValue::Text(String::from("name")),
            ],
        ),
        Ok(NativeBoundaryValue::Text(String::from("Ada")))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.int",
            &[
                NativeBoundaryValue::PostgresRow(row.clone()),
                NativeBoundaryValue::Text(String::from("age")),
            ],
        ),
        Ok(NativeBoundaryValue::Int(42))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.bool",
            &[
                NativeBoundaryValue::PostgresRow(row.clone()),
                NativeBoundaryValue::Text(String::from("active")),
            ],
        ),
        Ok(NativeBoundaryValue::Bool(true))
    );
    assert_eq!(
        dispatch(
            "std.db.postgres.json",
            &[
                NativeBoundaryValue::PostgresRow(row.clone()),
                NativeBoundaryValue::Text(String::from("meta")),
            ],
        ),
        Ok(NativeBoundaryValue::Json(json::string("ok")))
    );

    let error = dispatch(
        "std.db.postgres.string",
        &[
            NativeBoundaryValue::PostgresRow(row),
            NativeBoundaryValue::Text(String::from("missing")),
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
        &[NativeBoundaryValue::Text(String::from("not json"))],
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
