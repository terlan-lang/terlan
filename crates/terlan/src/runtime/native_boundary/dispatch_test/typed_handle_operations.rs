use super::*;
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;

#[test]
fn source_cookie_policy_operations_are_absent_from_native_dispatch() {
    for operation in [
        "std.http.cookies.set_header",
        "std.http.cookies.delete_header",
        "std.http.cookies.set_header_with_options",
    ] {
        assert_eq!(operation_arity(operation), None);
        for count in [0, 2, 5, 10] {
            let args = vec![NativeBoundaryValue::Text("value".into()); count];
            assert_eq!(
                dispatch(operation, &args).unwrap_err().code(),
                "dispatch.unknown_operation"
            );
            let mut store = ResourceStore::new();
            let args = vec![NativeBoundaryBridgeValue::Text("value".into()); count];
            assert_eq!(
                dispatch_with_resources(&mut store, operation, &args)
                    .unwrap_err()
                    .code(),
                "dispatch.unknown_operation"
            );
        }
    }
}

/// Cookie codecs and JSON serialization retain their package resource boundaries.
#[test]
pub(super) fn bridge_dispatch_cookie_codecs_and_json_serialization() {
    let mut store = ResourceStore::new();
    let optional_text = |value: Option<&str>| match value {
        Some(value) => NativeBoundaryBridgeValue::Record {
            name: "Some".into(),
            fields: vec![(
                "value".into(),
                NativeBoundaryBridgeValue::Text(value.into()),
            )],
        },
        None => NativeBoundaryBridgeValue::Record {
            name: "None".into(),
            fields: vec![],
        },
    };
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.encode",
            &[
                NativeBoundaryBridgeValue::Text("session".to_string()),
                NativeBoundaryBridgeValue::Text("abc123".to_string()),
                NativeBoundaryBridgeValue::Text("/".to_string()),
                optional_text(None),
                NativeBoundaryBridgeValue::Record {
                    name: "None".into(),
                    fields: vec![]
                },
                optional_text(None),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Bool(true),
                optional_text(None),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Text(
            "session=abc123; HttpOnly; Secure; Path=/".to_string()
        ))
    );
    assert_eq!(
        bridge_dispatch_ok(
            &mut store,
            "std.http.cookies.encode",
            &[
                NativeBoundaryBridgeValue::Text("session".to_string()),
                NativeBoundaryBridgeValue::Text("abc123".to_string()),
                NativeBoundaryBridgeValue::Text("/account".to_string()),
                optional_text(Some("example.com")),
                NativeBoundaryBridgeValue::Record { name: "Some".into(), fields: vec![("value".into(), NativeBoundaryBridgeValue::Int(3600))] },
                optional_text(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
                NativeBoundaryBridgeValue::Bool(true),
                NativeBoundaryBridgeValue::Bool(true),
                optional_text(Some("lax")),
            ],
        ),
        Some(NativeBoundaryBridgeValue::Text(
            "session=abc123; HttpOnly; SameSite=Lax; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT".to_string()
        ))
    );

    let body = NativeBoundaryBridgeValue::Text(r#"{"name":"Ada"}"#.into());
    let Some(NativeBoundaryBridgeValue::Handle(parsed)) =
        bridge_dispatch_ok(&mut store, "std.data.json.parse", &[body])
    else {
        return;
    };
    let serialized = bridge_dispatch_ok(
        &mut store,
        "std.data.json.to_string",
        &[NativeBoundaryBridgeValue::Handle(parsed)],
    )
    .expect("serialize live JSON resource");
    assert_eq!(
        serialized,
        NativeBoundaryBridgeValue::Text(r#"{"name":"Ada"}"#.into())
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
pub(super) fn bridge_dispatch_path_returns_and_accepts_handles() {
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

/// Validates regex line matching crosses the resource bridge as a typed list.
///
/// Inputs:
/// - A compiled regex handle and multi-line UTF-8 source text.
///
/// Output:
/// - One-based matching line numbers in source order.
///
/// Transformation:
/// - Exercises compilation, opaque-handle storage, aggregate integer return
///   projection, and bridge decoding through the same path used by validators.
#[test]
pub(super) fn bridge_dispatch_regex_matching_line_numbers_returns_typed_list() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(regex)) = bridge_dispatch_ok(
        &mut store,
        "std.regex.regex.compile",
        &[NativeBoundaryBridgeValue::Text(String::from(
            "(?i)cuda|ptx",
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.regex.regex.matching_line_numbers",
            &[
                NativeBoundaryBridgeValue::Handle(regex),
                NativeBoundaryBridgeValue::Text(String::from(
                    "CUDA kernel\nordinary text\nptx module\n",
                )),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::List(vec![
            NativeBoundaryBridgeValue::Int(1),
            NativeBoundaryBridgeValue::Int(3),
        ]))
    );
}

/// Removed URI handle operations must not silently execute legacy adapters.
#[test]
pub(super) fn bridge_dispatch_rejects_removed_uri_handle_operations() {
    let mut store = ResourceStore::new();
    assert!(dispatch_with_resources(
        &mut store,
        "std.net.uri.parse",
        &[NativeBoundaryBridgeValue::Text(
            "https://example.com".into()
        )],
    )
    .is_err());
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
pub(super) fn bridge_dispatch_postgres_row_handles_decode_values() {
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
pub(super) fn bridge_dispatch_postgres_row_keeps_atom_like_columns_as_text() {
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
pub(super) fn bridge_dispatch_postgres_pool_handles_reach_query_adapter() {
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
pub(super) fn bridge_dispatch_rejects_stale_handle_with_stable_error_code() {
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
pub(super) fn dispatches_json_parse_get_and_as_string() {
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

/// Validates JSON array and object mutations retain VM-owned handle identity.
#[test]
pub(super) fn bridge_dispatches_json_mutable_receiver_operations() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(array)) =
        bridge_dispatch_ok(&mut store, "std.data.json.array", &[])
    else {
        return;
    };
    let Some(NativeBoundaryBridgeValue::Handle(value)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.string",
        &[NativeBoundaryBridgeValue::Text(String::from("Ada"))],
    ) else {
        return;
    };
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.array_push",
            &[
                NativeBoundaryBridgeValue::Handle(array),
                NativeBoundaryBridgeValue::Handle(value),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Handle(array))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.length",
            &[NativeBoundaryBridgeValue::Handle(array)],
        ),
        Ok(NativeBoundaryBridgeValue::Int(1))
    );
    let Some(NativeBoundaryBridgeValue::Handle(replacement)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.string",
        &[NativeBoundaryBridgeValue::Text(String::from("Grace"))],
    ) else {
        return;
    };
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.array_set",
            &[
                NativeBoundaryBridgeValue::Handle(array),
                NativeBoundaryBridgeValue::Int(0),
                NativeBoundaryBridgeValue::Handle(replacement),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Handle(array))
    );
    let Some(NativeBoundaryBridgeValue::Handle(object)) =
        bridge_dispatch_ok(&mut store, "std.data.json.object", &[])
    else {
        return;
    };
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.object_put",
            &[
                NativeBoundaryBridgeValue::Handle(object),
                NativeBoundaryBridgeValue::Text(String::from("name")),
                NativeBoundaryBridgeValue::Handle(value),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Handle(object))
    );
    assert!(matches!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.get",
            &[
                NativeBoundaryBridgeValue::Handle(object),
                NativeBoundaryBridgeValue::Text(String::from("name")),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Handle(_))
    ));
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.object_remove",
            &[
                NativeBoundaryBridgeValue::Handle(object),
                NativeBoundaryBridgeValue::Text(String::from("name")),
            ],
        ),
        Ok(NativeBoundaryBridgeValue::Handle(object))
    );
    assert!(dispatch_with_resources(
        &mut store,
        "std.data.json.get",
        &[
            NativeBoundaryBridgeValue::Handle(object),
            NativeBoundaryBridgeValue::Text(String::from("name")),
        ],
    )
    .is_err());
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
pub(super) fn dispatches_base64_round_trip() {
    let Some(NativeBoundaryValue::Text(encoded)) = dispatch_ok(
        "std.encoding.base64.encode",
        &[NativeBoundaryValue::Text(String::from("hello Terlan"))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch(
            "std.encoding.base64.decode_text",
            &[NativeBoundaryValue::Text(encoded)]
        ),
        Ok(NativeBoundaryValue::Record {
            name: "Ok".into(),
            fields: vec![(
                "value".into(),
                NativeBoundaryValue::Text(String::from("hello Terlan"))
            )]
        })
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
            "std.encoding.base64.decode_octets",
            &[NativeBoundaryValue::Text(encoded_bytes)]
        ),
        Ok(NativeBoundaryValue::Record {
            name: "Ok".into(),
            fields: vec![("value".into(), NativeBoundaryValue::Bytes(bytes))]
        })
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
pub(super) fn dispatches_path_join_and_file_name() {
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

/// Accessors are Terlan source bodies, not runtime handle operations.
#[test]
pub(super) fn dispatcher_rejects_removed_uri_component_operations() {
    for operation in [
        "std.net.uri.scheme",
        "std.net.uri.host",
        "std.net.uri.path",
        "std.net.uri.query",
        "std.net.uri.fragment",
        "std.net.uri.to_string",
    ] {
        assert!(dispatch(
            operation,
            &[NativeBoundaryValue::Text("not a handle".into())]
        )
        .is_err());
    }
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
pub(super) fn dispatch_postgres_connect_preserves_adapter_error_codes() {
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
pub(super) fn dispatch_postgres_query_operations_are_known_driver_operations() {
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
pub(super) fn dispatch_postgres_transaction_requires_runtime_bridge() {
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
pub(super) fn dispatch_postgres_row_accessors_decode_values() {
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
pub(super) fn rejects_wrong_arity_with_stable_error_code() {
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
pub(super) fn rejects_wrong_type_with_stable_error_code() {
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
pub(super) fn rejects_unknown_operation_with_stable_error_code() {
    let error = dispatch("std.unknown.nope", &[])
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "dispatch.unknown_operation");
}

/// Retired request operations must fail before argument or handle decoding.
#[test]
fn source_request_operations_are_absent_from_native_dispatch() {
    for method in [
        "body_file_path",
        "body_text",
        "body_json",
        "method",
        "path",
        "param",
        "query",
        "query_string",
        "header",
        "cookie",
        "cookies",
    ] {
        let operation = format!("std.http.request.{method}");
        assert_eq!(operation_arity(&operation), None);
        for args in [
            vec![],
            vec![NativeBoundaryValue::Text("request".into())],
            vec![
                NativeBoundaryValue::Text("request".into()),
                NativeBoundaryValue::Text("key".into()),
            ],
        ] {
            assert_eq!(
                dispatch(&operation, &args).unwrap_err().code(),
                "dispatch.unknown_operation"
            );
        }
        let mut store = ResourceStore::new();
        for args in [
            vec![],
            vec![NativeBoundaryBridgeValue::Text("request".into())],
            vec![NativeBoundaryBridgeValue::Handle(NativeBoundaryHandle {
                id: u64::MAX,
                generation: u64::MAX,
            })],
        ] {
            assert_eq!(
                dispatch_with_resources(&mut store, &operation, &args)
                    .unwrap_err()
                    .code(),
                "dispatch.unknown_operation",
                "{operation}"
            );
        }
    }
}

#[test]
fn json_render_rejects_invalid_resources_and_retired_http_json_dispatch() {
    let mut store = ResourceStore::new();
    let handle = store
        .insert(ResourceValue::Json(json::string("kept")))
        .unwrap();
    for arguments in [
        vec![],
        vec![
            NativeBoundaryBridgeValue::Handle(handle),
            NativeBoundaryBridgeValue::Int(200),
        ],
        vec![NativeBoundaryBridgeValue::Handle(NativeBoundaryHandle {
            id: u64::MAX,
            generation: u64::MAX,
        })],
    ] {
        assert_eq!(
            dispatch_with_resources(&mut store, "std.http.response.json", &arguments)
                .unwrap_err()
                .code(),
            "dispatch.unknown_operation"
        );
    }
    assert_eq!(operation_arity("std.http.response.json"), None);
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.to_string",
            &[NativeBoundaryBridgeValue::Handle(handle)]
        )
        .unwrap(),
        NativeBoundaryBridgeValue::Text(r#""kept""#.into())
    );
    assert_eq!(
        dispatch("std.data.json.to_string", &[NativeBoundaryValue::Int(0)])
            .unwrap_err()
            .code(),
        "dispatch.type"
    );
    store.dispose(handle).unwrap();
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.to_string",
            &[NativeBoundaryBridgeValue::Handle(handle)]
        )
        .unwrap_err()
        .code(),
        "resource.stale_handle"
    );
}
