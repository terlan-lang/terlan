use super::*;
use serde_json::Value;

/// Builds a JSON resource fixture.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - JSON resource wrapping a stable string value.
///
/// Transformation:
/// - Uses the JSON adapter constructor to avoid depending on raw
///   `serde_json` values outside tests.
fn json_resource() -> ResourceValue {
    ResourceValue::Json(json::Json::from_serde(Value::String(String::from("Ada"))))
}

/// Builds a path resource fixture.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Path resource for `src/main.terl`, or an empty path after a failing
///   assertion.
///
/// Transformation:
/// - Converts adapter parsing into a resource value without unwrap/expect.
fn path_resource() -> ResourceValue {
    let parsed = path::from_string("src/main.terl");
    assert!(parsed.is_ok());
    ResourceValue::Path(parsed.unwrap_or_else(|_| path::Path::from_path_buf(Default::default())))
}

/// Validates resources are stored and retrieved by matching handles.
///
/// Inputs:
/// - One JSON resource.
///
/// Output:
/// - Test passes when the returned handle retrieves the same kind.
///
/// Transformation:
/// - Moves a resource into the store and checks handle-based lookup.
#[test]
fn insert_returns_live_handle_for_resource() {
    let mut store = ResourceStore::new();
    let result = store.insert(json_resource());
    assert!(result.is_ok());
    let Some(handle) = result.ok() else {
        return;
    };

    assert_eq!(store.kind(handle), Ok(ResourceKind::Json));
    assert!(store.json(handle).is_ok());
}

/// Validates resource-kind mismatches use a stable error code.
///
/// Inputs:
/// - One path resource accessed as JSON.
///
/// Output:
/// - Test passes when lookup returns `resource.kind`.
///
/// Transformation:
/// - Exercises live-handle type validation after liveness succeeds.
#[test]
fn rejects_wrong_resource_kind_with_stable_error_code() {
    let mut store = ResourceStore::new();
    let result = store.insert(path_resource());
    assert!(result.is_ok());
    let Some(handle) = result.ok() else {
        return;
    };

    let error = store
        .json(handle)
        .err()
        .unwrap_or_else(|| ResourceError::new("missing", ""));
    assert_eq!(error.code(), "resource.kind");
}

/// Validates disposed handles become stale.
///
/// Inputs:
/// - One JSON resource and its handle.
///
/// Output:
/// - Test passes when dispose succeeds once and later lookup fails stale.
///
/// Transformation:
/// - Removes a resource through a matching handle, then checks stale-handle
///   rejection for the old handle.
#[test]
fn dispose_removes_resource_and_rejects_stale_handle() {
    let mut store = ResourceStore::new();
    let result = store.insert(json_resource());
    assert!(result.is_ok());
    let Some(handle) = result.ok() else {
        return;
    };

    assert_eq!(store.dispose(handle), Ok(()));
    let error = store
        .json(handle)
        .err()
        .unwrap_or_else(|| ResourceError::new("missing", ""));
    assert_eq!(error.code(), "resource.stale_handle");
}

/// Validates stale generations are rejected.
///
/// Inputs:
/// - One JSON resource and a handle with a modified generation.
///
/// Output:
/// - Test passes when lookup returns `resource.stale_handle`.
///
/// Transformation:
/// - Exercises generation-tag validation without disposing the resource.
#[test]
fn rejects_stale_generation_with_stable_error_code() {
    let mut store = ResourceStore::new();
    let result = store.insert(json_resource());
    assert!(result.is_ok());
    let Some(mut handle) = result.ok() else {
        return;
    };
    handle.generation += 1;

    let error = store
        .json(handle)
        .err()
        .unwrap_or_else(|| ResourceError::new("missing", ""));
    assert_eq!(error.code(), "resource.stale_handle");
}

/// Validates id allocation overflow is rejected before insertion.
///
/// Inputs:
/// - Store whose next id is `u64::MAX`.
///
/// Output:
/// - Test passes when insertion returns `resource.id_overflow`.
///
/// Transformation:
/// - Exercises checked resource-id allocation.
#[test]
fn insert_rejects_id_overflow() {
    let mut store = ResourceStore {
        next_id: u64::MAX,
        resources: BTreeMap::new(),
    };

    let error = store
        .insert(json_resource())
        .err()
        .unwrap_or_else(|| ResourceError::new("missing", ""));
    assert_eq!(error.code(), "resource.id_overflow");
}
