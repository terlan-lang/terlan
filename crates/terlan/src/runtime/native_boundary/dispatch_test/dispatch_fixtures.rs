use super::*;

use crate::terlan_native_boundary::resource::ResourceStore;

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
pub(super) fn rust_backed_manifest_operations() -> Vec<(&'static str, usize)> {
    include_str!("../../../../../../std/RUST_BACKED_MANIFEST.tsv")
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
pub(super) fn dispatch_ok(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Option<NativeBoundaryValue> {
    let result = dispatch(operation, args);
    assert!(result.is_ok());
    result.ok()
}

pub(super) fn expected_postgres_unreachable_code() -> &'static str {
    "postgres.vm_driver_unavailable"
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
pub(super) fn bridge_dispatch_ok(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Option<NativeBoundaryBridgeValue> {
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
/// - Every manifest operation has exactly one legacy or package-owned provider.
///
/// Transformation:
/// - Compares the release manifest operation inventory to
///   registered binding arities so dispatch cannot silently drift from std.
#[test]
pub(super) fn operation_arities_cover_rust_backed_std_manifest() {
    let operations = rust_backed_manifest_operations();
    assert!(!operations.is_empty());

    for (operation, arity) in operations {
        // The actor-worker adapter intercepts database calls before legacy dispatch.
        let actor_worker = crate::std_native_packages::postgres::operation_arity(operation);
        let legacy = actor_worker.or_else(|| operation_arity(operation));
        let bindings = crate::std_native_packages::VALUE_BINDINGS
            .iter()
            .filter(|binding| binding.operation == operation)
            .collect::<Vec<_>>();
        assert_eq!(
            usize::from(legacy.is_some()) + bindings.len(),
            1,
            "{operation} must have exactly one implementation owner"
        );
        let registered = bindings.first().map(|binding| binding.arity);
        assert_eq!(legacy.or(registered), Some(arity), "{operation}");
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
pub(super) fn dispatch_json_builder_constructors_return_json_values() {
    let Some(NativeBoundaryValue::Json(value)) = dispatch_ok("std.data.json.null", &[]) else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("null")));

    let Some(NativeBoundaryValue::Json(value)) =
        dispatch_ok("std.data.json.bool", &[NativeBoundaryValue::Bool(true)])
    else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("true")));

    let Some(NativeBoundaryValue::Json(value)) =
        dispatch_ok("std.data.json.int", &[NativeBoundaryValue::Int(3)])
    else {
        return;
    };
    assert_eq!(json::stringify(&value), Ok(String::from("3")));

    let Some(NativeBoundaryValue::Json(value)) = dispatch_ok(
        "std.data.json.string",
        &[NativeBoundaryValue::Text(String::from("Ada"))],
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
pub(super) fn operation_arity_rejects_non_manifest_operation() {
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
pub(super) fn bridge_dispatch_json_returns_and_accepts_handles() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[NativeBoundaryBridgeValue::Text(String::from(
            r#"{"name":"Ada"}"#,
        ))],
    ) else {
        return;
    };
    let Some(NativeBoundaryBridgeValue::Handle(name)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.get",
        &[
            NativeBoundaryBridgeValue::Handle(root),
            NativeBoundaryBridgeValue::Text(String::from("name")),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.as_string",
            &[NativeBoundaryBridgeValue::Handle(name)]
        ),
        Ok(NativeBoundaryBridgeValue::Text(String::from("Ada")))
    );
}

/// Validates bridge JSON object keys stay dynamic text.
///
/// Inputs:
/// - JSON source whose object keys look like Vm atom-construction
///   functions.
///
/// Output:
/// - Test passes when bridge lookup returns JSON handles whose values decode
///   as text.
///
/// Transformation:
/// - Exercises NativeBoundary resource dispatch without converting external JSON
///   object keys into atoms or backend symbols.
#[test]
pub(super) fn bridge_dispatch_json_keeps_atom_like_object_keys_as_text() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[NativeBoundaryBridgeValue::Text(String::from(
            r#"{"binary_to_atom":"blocked","list_to_atom":"blocked"}"#,
        ))],
    ) else {
        return;
    };

    for key in ["binary_to_atom", "list_to_atom"] {
        let Some(NativeBoundaryBridgeValue::Handle(value)) = bridge_dispatch_ok(
            &mut store,
            "std.data.json.get",
            &[
                NativeBoundaryBridgeValue::Handle(root),
                NativeBoundaryBridgeValue::Text(key.to_string()),
            ],
        ) else {
            return;
        };

        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.data.json.as_string",
                &[NativeBoundaryBridgeValue::Handle(value)]
            ),
            Ok(NativeBoundaryBridgeValue::Text(String::from("blocked")))
        );
    }
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
pub(super) fn bridge_dispatch_json_array_length_and_at_use_handles() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(root)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.parse",
        &[NativeBoundaryBridgeValue::Text(String::from(
            r#"["Ada",3]"#,
        ))],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.length",
            &[NativeBoundaryBridgeValue::Handle(root)]
        ),
        Ok(NativeBoundaryBridgeValue::Int(2))
    );

    let Some(NativeBoundaryBridgeValue::Handle(name)) = bridge_dispatch_ok(
        &mut store,
        "std.data.json.at",
        &[
            NativeBoundaryBridgeValue::Handle(root),
            NativeBoundaryBridgeValue::Int(0),
        ],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.data.json.as_string",
            &[NativeBoundaryBridgeValue::Handle(name)]
        ),
        Ok(NativeBoundaryBridgeValue::Text(String::from("Ada")))
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
/// - Exercises the NativeBoundary resource dispatch path used by
///   `std.native.collections.Vector` without exposing Rust vectors across the
///   bridge boundary.
#[test]
pub(super) fn bridge_dispatch_native_vector_allocates_and_mutates_handle() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(vector)) = bridge_dispatch_ok(
        &mut store,
        "std.native.collections.vector.from_list",
        &[NativeBoundaryBridgeValue::List(vec![
            NativeBoundaryBridgeValue::Text(String::from("Ada")),
            NativeBoundaryBridgeValue::Text(String::from("Grace")),
        ])],
    ) else {
        return;
    };

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.length",
            &[NativeBoundaryBridgeValue::Handle(vector)]
        ),
        Ok(NativeBoundaryBridgeValue::Int(2))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.get_at",
            &[
                NativeBoundaryBridgeValue::Handle(vector),
                NativeBoundaryBridgeValue::Int(1)
            ]
        ),
        Ok(NativeBoundaryBridgeValue::Text(String::from("Grace")))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.get",
            &[
                NativeBoundaryBridgeValue::Handle(vector),
                NativeBoundaryBridgeValue::Int(1)
            ]
        ),
        Ok(NativeBoundaryBridgeValue::List(vec![
            NativeBoundaryBridgeValue::Text(String::from("Grace"))
        ]))
    );
    for index in [-1, 2, i64::MAX] {
        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.native.collections.vector.get",
                &[
                    NativeBoundaryBridgeValue::Handle(vector),
                    NativeBoundaryBridgeValue::Int(index)
                ]
            ),
            Ok(NativeBoundaryBridgeValue::List(Vec::new()))
        );
    }

    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.set_at",
            &[
                NativeBoundaryBridgeValue::Handle(vector),
                NativeBoundaryBridgeValue::Int(1),
                NativeBoundaryBridgeValue::Text(String::from("Carol"))
            ]
        ),
        Ok(NativeBoundaryBridgeValue::Handle(vector))
    );
    assert_eq!(
        dispatch_with_resources(
            &mut store,
            "std.native.collections.vector.get_at",
            &[
                NativeBoundaryBridgeValue::Handle(vector),
                NativeBoundaryBridgeValue::Int(1)
            ]
        ),
        Ok(NativeBoundaryBridgeValue::Text(String::from("Carol")))
    );
}

/// Verifies NativeBoundary bridge dispatch rejects cross-resource handle confusion.
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
pub(super) fn adversarial_native_boundary_dispatch_rejects_cross_resource_handle_confusion() {
    let mut store = ResourceStore::new();
    let Some(NativeBoundaryBridgeValue::Handle(vector)) = bridge_dispatch_ok(
        &mut store,
        "std.native.collections.vector.from_list",
        &[NativeBoundaryBridgeValue::List(vec![
            NativeBoundaryBridgeValue::Int(1),
            NativeBoundaryBridgeValue::Int(2),
        ])],
    ) else {
        return;
    };

    let error = dispatch_with_resources(
        &mut store,
        "std.data.json.as_string",
        &[NativeBoundaryBridgeValue::Handle(vector)],
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "", 0));

    assert_eq!(error.code(), "resource.kind");
}

/// Cookie and response dispatch retain native behavior without Request accessors.
#[test]
pub(super) fn dispatch_http_cookie_and_response_operations_return_native_values() {
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.encode",
            &[
                NativeBoundaryValue::Text("session".to_string()),
                NativeBoundaryValue::Text("abc123".to_string()),
                NativeBoundaryValue::Text("/".to_string()),
                NativeBoundaryValue::OptionalText(None),
                NativeBoundaryValue::Record {
                    name: "None".into(),
                    fields: vec![]
                },
                NativeBoundaryValue::OptionalText(None),
                NativeBoundaryValue::Bool(true),
                NativeBoundaryValue::Bool(false),
                NativeBoundaryValue::OptionalText(None),
            ],
        ),
        Some(NativeBoundaryValue::Text(
            "session=abc123; HttpOnly; Path=/".to_string()
        ))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.encode",
            &[
                NativeBoundaryValue::Text("session".to_string()),
                NativeBoundaryValue::Text("abc123".to_string()),
                NativeBoundaryValue::Text("/account".to_string()),
                NativeBoundaryValue::OptionalText(Some("example.com".into())),
                NativeBoundaryValue::Record { name: "Some".into(), fields: vec![("value".into(), NativeBoundaryValue::Int(3600))] },
                NativeBoundaryValue::OptionalText(Some("Wed, 21 Oct 2015 07:28:00 GMT".into())),
                NativeBoundaryValue::Bool(true),
                NativeBoundaryValue::Bool(true),
                NativeBoundaryValue::OptionalText(Some("strict".into())),
            ],
        ),
        Some(NativeBoundaryValue::Text(
            "session=abc123; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT".to_string()
        ))
    );
    assert_eq!(
        dispatch_ok(
            "std.http.cookies.encode",
            &[
                NativeBoundaryValue::Text("session".to_string()),
                NativeBoundaryValue::Text(String::new()),
                NativeBoundaryValue::Text("/".to_string()),
                NativeBoundaryValue::OptionalText(None),
                NativeBoundaryValue::Record {
                    name: "Some".into(),
                    fields: vec![("value".into(), NativeBoundaryValue::Int(0))]
                },
                NativeBoundaryValue::OptionalText(Some("Thu, 01 Jan 1970 00:00:00 GMT".into())),
                NativeBoundaryValue::Bool(false),
                NativeBoundaryValue::Bool(false),
                NativeBoundaryValue::OptionalText(None),
            ],
        ),
        Some(NativeBoundaryValue::Text(
            "session=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string()
        ))
    );
}

/// Verifies direct NativeBoundary dispatch cannot bypass VM stream ownership.
#[test]
pub(super) fn dispatch_http_stream_response_requires_vm_runtime() {
    let error = dispatch(
        "std.http.response.stream",
        &[
            NativeBoundaryValue::Unit,
            NativeBoundaryValue::Int(200),
            NativeBoundaryValue::Text("text/plain".to_string()),
            NativeBoundaryValue::Int(16_384),
            NativeBoundaryValue::Int(128),
        ],
    )
    .expect_err("direct NativeBoundary streaming must fail");

    assert_eq!(error.code(), "dispatch.unknown_operation");
}
