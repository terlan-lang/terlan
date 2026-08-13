use std::collections::HashMap;
use std::sync::Arc;

use crate::compiler::native_ir::constructors::NativeConstructorLayout;
use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreType};

use super::memory_intrinsics::lower_memory_intrinsic;
use super::{NativeConstructorLayouts, NativeExpr, NativeType};

/// Builds the canonical native `Memory.Layout` constructor inventory.
fn constructors() -> NativeConstructorLayouts {
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(
            "std.core.Memory.Layout",
            vec![
                ("size".to_string(), ManagedFieldType::Int),
                ("alignment".to_string(), ManagedFieldType::Int),
                ("storage".to_string(), ManagedFieldType::Atom),
            ],
        )
        .expect("Memory.Layout descriptor"),
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor).expect("encode Memory.Layout descriptor"),
    );
    let result = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("std.core.Memory.Layout")
            .expect("Memory.Layout semantic identity"),
    );
    HashMap::from([(
        ("std.core.Memory.Layout".to_string(), 3),
        NativeConstructorLayout {
            parameter_core_types: vec![None; 3],
            parameters: vec![NativeType::Int, NativeType::Int, NativeType::Atom],
            result,
            result_core_type: Some(CoreType::Named("std.core.Memory.Layout".to_string())),
            descriptor,
            encoded_layout,
        },
    )])
}

/// Builds one pure memory intrinsic call.
fn call(id: CoreIntrinsicId, args: Vec<CoreExpr>, return_type: CoreType) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id,
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 1),
    }
}

#[test]
fn layout_of_materializes_scalar_and_managed_reference_layouts() {
    let layouts = constructors();
    let int = lower_memory_intrinsic(
        &call(
            CoreIntrinsicId::MemoryLayoutOf(CoreType::Int),
            vec![],
            CoreType::Named("std.core.Memory.Layout".to_string()),
        ),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("memory intrinsic")
    .expect("lower Int layout");
    let string = lower_memory_intrinsic(
        &call(
            CoreIntrinsicId::MemoryLayoutOf(CoreType::String),
            vec![],
            CoreType::Named("std.core.Memory.Layout".to_string()),
        ),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("memory intrinsic")
    .expect("lower String layout");

    assert!(matches!(
        int,
        NativeExpr::Construct { fields, .. }
            if fields == vec![
                NativeExpr::Int(8),
                NativeExpr::Int(8),
                NativeExpr::AtomLiteral(Arc::from("Inline")),
            ]
    ));
    assert!(matches!(
        string,
        NativeExpr::Construct { fields, .. }
            if fields == vec![
                NativeExpr::Int(8),
                NativeExpr::Int(8),
                NativeExpr::AtomLiteral(Arc::from("Managed")),
            ]
    ));
}

#[test]
fn layout_of_materializes_opaque_layout_for_unrepresented_types() {
    let opaque = lower_memory_intrinsic(
        &call(
            CoreIntrinsicId::MemoryLayoutOf(CoreType::Dynamic),
            vec![],
            CoreType::Named("std.core.Memory.Layout".to_string()),
        ),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &constructors(),
    )
    .expect("memory intrinsic")
    .expect("lower opaque layout");

    assert!(matches!(
        opaque,
        NativeExpr::Construct { fields, .. }
            if fields == vec![
                NativeExpr::Int(0),
                NativeExpr::Int(1),
                NativeExpr::AtomLiteral(Arc::from("Opaque")),
            ]
    ));
}

#[test]
fn scalar_sizes_fold_while_managed_sizes_use_heap_operations() {
    let layouts = constructors();
    let int = lower_memory_intrinsic(
        &call(
            CoreIntrinsicId::MemoryRetainedSize(CoreType::Int),
            vec![CoreExpr::Int(42)],
            CoreType::Int,
        ),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("memory intrinsic")
    .expect("lower Int retained size");
    let string = lower_memory_intrinsic(
        &call(
            CoreIntrinsicId::MemoryShallowSize(CoreType::String),
            vec![CoreExpr::Var("value".to_string())],
            CoreType::Int,
        ),
        &HashMap::from([("value".to_string(), 0)]),
        &HashMap::from([("value".to_string(), NativeType::StringRef)]),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("memory intrinsic")
    .expect("lower String shallow size");

    assert_eq!(int, NativeExpr::Int(8));
    assert!(matches!(
        string,
        NativeExpr::ManagedOperation { args, .. }
            if args == vec![NativeExpr::Param(0)]
    ));
}
