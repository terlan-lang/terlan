use super::*;

#[test]
fn map_keys_and_values_preserve_owner_and_generation_checks() {
    let mut store = ResourceStore::new();
    let handle = store
        .insert_for_owner(7, ResourceValue::Json(crate::terlan_native::json::null()))
        .unwrap();
    for pair in [
        (
            NativeBoundaryBridgeValue::Handle(handle),
            NativeBoundaryBridgeValue::Unit,
        ),
        (
            NativeBoundaryBridgeValue::Unit,
            NativeBoundaryBridgeValue::Handle(handle),
        ),
    ] {
        let value = NativeBoundaryBridgeValue::Map(vec![pair]);
        validate_bridge_resource_owner(&store, 7, &value).unwrap();
        assert!(validate_bridge_resource_owner(&store, 8, &value).is_err());
        let mut stale = store.clone();
        stale.dispose_for_owner(handle, 7).unwrap();
        assert!(validate_bridge_resource_owner(&stale, 7, &value).is_err());
    }
}

#[test]
fn map_bridge_roundtrip_preserves_key_types_and_nested_records() {
    let mut store = ResourceStore::new();
    let value = NativeBoundaryBridgeValue::Map(vec![(
        NativeBoundaryBridgeValue::Tuple(vec![NativeBoundaryBridgeValue::Int(42)]),
        NativeBoundaryBridgeValue::Record {
            name: "Entry".into(),
            fields: vec![("children".into(), NativeBoundaryBridgeValue::Map(vec![]))],
        },
    )]);
    let decoded = decode_bridge_args(&store, "application.identity", std::slice::from_ref(&value))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(encode_bridge_result(&mut store, 7, decoded).unwrap(), value);
    let decoded = decode_owned_bridge_value("application.identity", &value).unwrap();
    assert_eq!(encode_bridge_result(&mut store, 7, decoded).unwrap(), value);
}
