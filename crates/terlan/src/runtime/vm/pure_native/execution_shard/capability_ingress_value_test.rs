use super::*;

#[test]
fn map_capability_ingress_preserves_nested_maps_and_tuple_keys() {
    let value = ReplValue::Map(vec![(
        ReplValue::Tuple(vec![ReplValue::Int(42), ReplValue::Atom("ready".into())]),
        ReplValue::Record {
            name: "Payload".into(),
            fields: vec![("entries".into(), ReplValue::Map(vec![]))],
        },
    )]);
    let term = crate::runtime::vm::pure_native::repl_value_to_boundary_term(value.clone()).unwrap();
    assert_eq!(managed_capability_term(term).unwrap(), value);
}

#[test]
fn map_capability_ingress_rejects_resource_handles_in_keys_and_values() {
    for (key, value) in [
        (
            NativeBoundaryTerm::Handle {
                id: 1,
                generation: 1,
            },
            NativeBoundaryTerm::Unit,
        ),
        (
            NativeBoundaryTerm::Unit,
            NativeBoundaryTerm::Handle {
                id: 1,
                generation: 1,
            },
        ),
    ] {
        assert!(managed_capability_term(NativeBoundaryTerm::Map(vec![(key, value)])).is_err());
    }
}
