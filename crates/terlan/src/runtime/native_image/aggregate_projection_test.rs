use super::*;

#[test]
fn generic_projection_metadata_round_trips_without_library_types() {
    for semantic in ["Named(app.Document)", "Tuple(Int,String)"] {
        for fields in [
            AggregateFieldProjection::Complete,
            AggregateFieldProjection::Fields([1, 300].into_iter().collect()),
        ] {
            let projection = NativeAggregateProjection {
                module: "app".into(),
                function: "inspect".into(),
                arity: 1,
                semantic: SemanticTypeId::from_canonical(semantic).unwrap(),
                fields,
                scalar_entry: None,
                scalar_field: None,
                suspending: true,
            };
            let value = serde_json::to_value(&projection).unwrap();
            assert_eq!(
                serde_json::from_value::<NativeAggregateProjection>(value.clone()).unwrap(),
                projection
            );
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove("suspending");
            assert!(serde_json::from_value::<NativeAggregateProjection>(missing).is_err());
            let mut unknown = value.clone();
            unknown["request"] = true.into();
            assert!(serde_json::from_value::<NativeAggregateProjection>(unknown).is_err());
            for invalid in [
                serde_json::json!([]),
                serde_json::json!(vec![256; 16]),
                serde_json::Value::Null,
            ] {
                let mut malformed = value.clone();
                malformed["semantic"] = invalid;
                assert!(serde_json::from_value::<NativeAggregateProjection>(malformed).is_err());
            }
        }
    }
}
