use crate::runtime::native_image::managed::{
    decode_collection_layout, ManagedCollectionKind, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic,
    CoreTupleTypeElem, CoreType,
};

use super::{managed_collection_layouts, managed_expression_collection_layouts};

#[test]
/// Inventories nested schemas once and emits deterministic canonical ordering.
fn inventories_nested_collection_schemas_deterministically() {
    let list = CoreType::List(Box::new(CoreType::Int));
    let map = CoreType::Apply {
        constructor: "Map".to_owned(),
        args: vec![CoreType::String, list.clone()],
    };
    let set = CoreType::Apply {
        constructor: "std.collections.Set".to_owned(),
        args: vec![CoreType::Binary],
    };

    let layouts = managed_collection_layouts([&set, &map, &list]).expect("collection inventory");
    assert_eq!(layouts.len(), 3);
    assert!(layouts.windows(2).all(|pair| pair[0] < pair[1]));
    let descriptors = layouts
        .iter()
        .map(|layout| decode_collection_layout(layout).expect("collection schema"))
        .collect::<Vec<_>>();
    assert!(descriptors
        .iter()
        .any(|descriptor| descriptor.kind() == ManagedCollectionKind::List));
    let map_descriptor = descriptors
        .iter()
        .find_map(|descriptor| descriptor.map_descriptor())
        .expect("map descriptor");
    assert_eq!(
        map_descriptor.key_type(),
        ManagedFieldType::Reference(
            SemanticTypeId::from_canonical("std.core.String").expect("string identity")
        )
    );
    assert_eq!(
        map_descriptor.value_type(),
        ManagedFieldType::Reference(
            SemanticTypeId::from_canonical(&list.contract_text()).expect("list identity")
        )
    );
}

#[test]
/// Rejects dynamic slots and malformed generic collection applications.
fn rejects_nonconcrete_collection_slots_and_invalid_arity() {
    let dynamic = CoreType::List(Box::new(CoreType::Dynamic));
    assert!(managed_collection_layouts([&dynamic])
        .expect_err("dynamic collection slot")
        .contains("native_ir.collection_type"));

    let malformed = CoreType::Apply {
        constructor: "Map".to_owned(),
        args: vec![CoreType::Int],
    };
    assert!(managed_collection_layouts([&malformed])
        .expect_err("invalid map arity")
        .contains("native_ir.collection_arity"));
}

#[test]
fn iterator_intrinsics_inventory_their_physical_list_storage() {
    let pair = CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::String),
        CoreTupleTypeElem::Type(CoreType::Int),
    ]);
    let iterator = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapIterator),
        args: vec![CoreExpr::Var("values".to_string())],
        return_type: CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![pair],
        },
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: Span { start: 0, end: 0 },
    });

    let layouts =
        managed_expression_collection_layouts([&iterator]).expect("iterator collection inventory");
    let descriptors = layouts
        .iter()
        .map(|layout| decode_collection_layout(layout).expect("iterator storage schema"))
        .collect::<Vec<_>>();

    assert!(descriptors
        .iter()
        .any(|descriptor| descriptor.canonical_type() == "List(Tuple(String,Int))"));
}
