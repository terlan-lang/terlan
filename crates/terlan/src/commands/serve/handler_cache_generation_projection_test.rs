//! Persisted observations stay generic and malformed proof metadata fails closed.

use super::*;
use crate::runtime::native_image::aggregate_projection::{
    AggregateFieldProjection, NativeAggregateProjection,
};
use crate::runtime::native_image::managed::SemanticTypeId;

#[test]
fn persisted_aggregate_metadata_rejects_missing_proofs_and_rebuilds_legacy_schema() {
    let _guard = generation_test_guard();
    let root = test_fs::temp_path("serve", "aggregate_generation_metadata");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&web_root).unwrap();
    fs::write(&source_path, source(29)).unwrap();
    drop(cached_source_entry(&web_root, &source_path, MODULE).unwrap());
    invalidate_vm_handler_cache();
    let path = source_generation::active_generation_metadata_path(&web_root, MODULE)
        .unwrap()
        .unwrap();
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(original["schema"], "terlan-serve-generation-v25");
    assert!(original["aggregate_projections"].is_array());
    assert!(original.get("request_projections").is_none());

    let unrelated = NativeAggregateProjection {
        module: MODULE.into(),
        function: "value".into(),
        arity: 0,
        semantic: SemanticTypeId::from_canonical("Named(app.Document)").unwrap(),
        fields: AggregateFieldProjection::Fields([4].into_iter().collect()),
        scalar_entry: Some("not_an_http_entry".into()),
        scalar_field: Some(4),
        suspending: true,
    };
    let mut value = original.clone();
    value["aggregate_projections"] = serde_json::json!([unrelated]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let entry = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
    assert_eq!(
        entry.runtime.request_projection(MODULE, "value", 0),
        terlan_http_native::RequestFieldProjection::Complete
    );
    assert!(entry
        .runtime
        .scalar_request_ingress(MODULE, "value", 0)
        .is_none());
    assert_eq!(execute(&entry.runtime), 29);
    drop(entry);
    invalidate_vm_handler_cache();

    value["aggregate_projections"][0]
        .as_object_mut()
        .unwrap()
        .remove("suspending");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let error = match cached_source_entry(&web_root, &source_path, MODULE) {
        Ok(_) => panic!("missing suspension proof must not imply synchronous execution"),
        Err(error) => error,
    };
    assert!(error.contains("missing field `suspending`"), "{error}");

    value["schema"] = "terlan-serve-generation-v15".into();
    value
        .as_object_mut()
        .unwrap()
        .remove("aggregate_projections");
    value["request_projections"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let entry = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
    assert_eq!(execute(&entry.runtime), 29);
    drop(entry);
    invalidate_vm_handler_cache();
    let path = source_generation::active_generation_metadata_path(&web_root, MODULE)
        .unwrap()
        .unwrap();
    let rebuilt: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(rebuilt["schema"], "terlan-serve-generation-v25");
    assert!(rebuilt["aggregate_projections"].is_array());
    assert!(rebuilt.get("request_projections").is_none());
    // v18 Request records used maps; v19 Session used a combined acquisition
    // binding. v20 lacked interprocedural record field-use proofs; v21 stored
    // a WebSocket room prefix instead of a source naming callback; v22 invoked
    // the per-role matched callback directly instead of its source pair adapter.
    // v23 Session records lacked source-owned renewal lifetime and used unary
    // create/rotate bindings. v24 selected paired broadcast behavior natively
    // and did not use source Result callbacks with optional peer context.
    for schema in [
        "terlan-serve-generation-v18",
        "terlan-serve-generation-v19",
        "terlan-serve-generation-v20",
        "terlan-serve-generation-v21",
        "terlan-serve-generation-v22",
        "terlan-serve-generation-v23",
        "terlan-serve-generation-v24",
    ] {
        let path = source_generation::active_generation_metadata_path(&web_root, MODULE)
            .unwrap()
            .unwrap();
        let mut previous_layout = rebuilt.clone();
        previous_layout["schema"] = schema.into();
        fs::write(&path, serde_json::to_vec(&previous_layout).unwrap()).unwrap();
        let entry = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
        assert_eq!(execute(&entry.runtime), 29);
        drop(entry);
        invalidate_vm_handler_cache();
        let path = source_generation::active_generation_metadata_path(&web_root, MODULE)
            .unwrap()
            .unwrap();
        let rebuilt: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(rebuilt["schema"], "terlan-serve-generation-v25");
    }
    fs::remove_dir_all(root).unwrap();
}
