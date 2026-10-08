use std::sync::Arc;

#[path = "aggregate_projection_call_test.rs"]
mod call_test;
#[path = "aggregate_projection_source_test.rs"]
mod source_test;

use super::aggregate_projection::AggregateFieldProjection;
use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_layout, encode_string_append_operation,
    encode_string_prepend_literal_operation, encode_string_prepend_projected_literal_operation,
    ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_typeck::CoreType;

use super::{
    install_native_aggregate_projection_exports, native_aggregate_projections, NativeExpr,
    NativeFunction, NativeModule, NativeTransitionOperation, NativeType,
};

fn aggregate_semantic() -> SemanticTypeId {
    SemanticTypeId::from_canonical(&CoreType::Named("app.Document".to_string()).contract_text())
        .expect("aggregate semantic")
}

fn project(field: usize, argument: NativeExpr) -> NativeExpr {
    NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_aggregate_field_operation(aggregate_semantic(), field)
                .expect("aggregate projection"),
        ),
        args: vec![argument],
    }
}

fn module(body: NativeExpr) -> NativeModule {
    NativeModule {
        name: "app.DocumentTools".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "handle".to_string(),
            public: true,
            arity: 1,
            source_module: "app.DocumentTools".to_string(),
            source_function: "handle".to_string(),
            source_arity: 1,
            callable_captures: Vec::new(),
            params: vec![NativeType::ManagedRef(aggregate_semantic())],
            return_type: NativeType::Int,
            body,
        }],
        continuations: Vec::new(),
        managed_layouts: vec![Arc::from(
            encode_aggregate_layout(
                &ManagedAggregateDescriptor::tuple(
                    &CoreType::Named("app.Document".into()).contract_text(),
                    vec![
                        ManagedFieldType::Reference(
                            crate::runtime::native_image::managed::managed_string_semantic_id()
                        );
                        32
                    ],
                )
                .unwrap(),
            )
            .unwrap(),
        )],
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn projection(body: NativeExpr) -> AggregateFieldProjection {
    native_aggregate_projections(&[module(body)])
        .into_iter()
        .next()
        .expect("handler projection")
        .fields
}

#[test]
fn exact_accessors_produce_a_narrow_field_set() {
    let fields = projection(NativeExpr::Let {
        bindings: vec![
            project(1, NativeExpr::Param(0)),
            project(4, NativeExpr::Param(0)),
        ],
        body: Box::new(NativeExpr::Int(200)),
    });

    assert_eq!(fields, AggregateFieldProjection::Fields([1, 4].into()));
}

#[test]
fn file_body_accessor_is_a_narrow_non_text_projection() {
    let fields = projection(project(10, NativeExpr::Param(0)));
    assert_eq!(fields, AggregateFieldProjection::Fields([10].into()));
}

#[test]
fn fused_projected_string_operation_retains_a_narrow_field_set() {
    let fields = projection(NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_string_prepend_projected_literal_operation(aggregate_semantic(), 4, "prefix:")
                .expect("fused projection"),
        ),
        args: vec![NativeExpr::Param(0)],
    });

    assert_eq!(fields, AggregateFieldProjection::Fields([4].into()));
}

#[test]
fn let_alias_of_aggregate_remains_provably_projectable() {
    let fields = projection(NativeExpr::Let {
        bindings: vec![NativeExpr::Param(0)],
        body: Box::new(project(6, NativeExpr::Param(1))),
    });

    assert_eq!(fields, AggregateFieldProjection::Fields([6].into()));
}

#[test]
fn aggregate_escape_through_call_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::Call {
            function: 1,
            args: vec![NativeExpr::Param(0)],
        }),
        AggregateFieldProjection::Complete
    );
}

#[test]
fn aggregate_use_by_unknown_managed_operation_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_string_append_operation()),
            args: vec![NativeExpr::Param(0), NativeExpr::Param(0)],
        }),
        AggregateFieldProjection::Complete
    );
}

#[test]
fn returning_aggregate_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::Param(0)),
        AggregateFieldProjection::Complete
    );
}

#[test]
fn unused_aggregate_produces_an_empty_projection() {
    assert_eq!(
        projection(NativeExpr::Int(204)),
        AggregateFieldProjection::Fields(Default::default())
    );
}

#[test]
fn suspension_proof_is_persisted_with_aggregate_projection() {
    let projection = native_aggregate_projections(&[module(NativeExpr::Let {
        bindings: vec![project(4, NativeExpr::Param(0))],
        body: Box::new(NativeExpr::Suspend {
            operation: NativeTransitionOperation::Timer,
            arguments: vec![NativeExpr::Int(5)],
            continuation_id: 1,
            values: Vec::new(),
        }),
    })])
    .into_iter()
    .next()
    .expect("suspending handler projection");

    assert!(projection.suspending);
    assert_eq!(
        projection.fields,
        AggregateFieldProjection::Fields([4].into())
    );
}

#[test]
fn suspension_proof_follows_application_global_cross_module_calls() {
    let caller = module(NativeExpr::TailCall {
        function: 1,
        args: vec![project(10, NativeExpr::Param(0))],
        yield_continuation_id: None,
    });
    let callee = NativeModule {
        name: "app.Storage".to_string(),
        functions: vec![NativeFunction {
            export_id: 2,
            name: "store".to_string(),
            public: false,
            arity: 1,
            source_module: "app.Storage".to_string(),
            source_function: "store".to_string(),
            source_arity: 1,
            callable_captures: Vec::new(),
            params: vec![NativeType::StringRef],
            return_type: NativeType::Int,
            body: NativeExpr::Suspend {
                operation: NativeTransitionOperation::Capability,
                arguments: vec![NativeExpr::Int(1)],
                continuation_id: 9,
                values: Vec::new(),
            },
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    };

    let projection = native_aggregate_projections(&[caller, callee])
        .into_iter()
        .find(|projection| projection.function == "handle")
        .expect("caller aggregate projection");

    assert!(projection.suspending);
    assert_eq!(
        projection.fields,
        AggregateFieldProjection::Fields([10].into())
    );
}

#[test]
fn scalar_ingress_preserves_fused_literal_semantics_without_aggregate_graph() {
    let mut modules = vec![module(NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_string_prepend_projected_literal_operation(aggregate_semantic(), 4, "prefix:")
                .expect("fused projection"),
        ),
        args: vec![NativeExpr::Param(0)],
    })];

    let projections = install_native_aggregate_projection_exports(&mut modules);
    let projection = projections.first().expect("aggregate projection");
    assert_eq!(projection.scalar_field, Some(4));
    let entry = projection.scalar_entry.as_ref().expect("scalar entry");
    let generated = modules[0]
        .functions
        .iter()
        .find(|function| &function.name == entry)
        .expect("generated scalar function");
    assert_eq!(generated.params, vec![NativeType::StringRef]);
    assert_eq!(
        generated.body,
        NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_string_prepend_literal_operation("prefix:")
                    .expect("ordinary prepend operation")
            ),
            args: vec![NativeExpr::Param(0)],
        }
    );
}

#[test]
fn wide_aggregate_and_zero_index_are_not_limited_by_http_field_numbers() {
    for field in [0, 16, 31] {
        let mut modules = vec![module(project(field, NativeExpr::Param(0)))];
        modules[0].functions[0].return_type = NativeType::StringRef;
        let projections = install_native_aggregate_projection_exports(&mut modules);
        assert_eq!(
            projections[0].fields,
            AggregateFieldProjection::Fields([field].into())
        );
        assert_eq!(projections[0].scalar_field, Some(field));
        assert_eq!(modules[0].functions[1].body, NativeExpr::Param(0));
        assert_eq!(
            modules[0].functions[0].body,
            project(field, NativeExpr::Param(0))
        );
    }
}

#[test]
fn missing_malformed_non_string_and_conflicting_layouts_do_not_specialize() {
    let base = module(project(4, NativeExpr::Param(0)));
    let integer_layout: Arc<[u8]> = Arc::from(
        encode_aggregate_layout(
            &ManagedAggregateDescriptor::tuple(
                &CoreType::Named("app.Document".into()).contract_text(),
                vec![ManagedFieldType::Int; 32],
            )
            .unwrap(),
        )
        .unwrap(),
    );
    for layouts in [
        vec![],
        vec![Arc::from(&b"invalid"[..])],
        vec![integer_layout.clone()],
        vec![base.managed_layouts[0].clone(), integer_layout],
    ] {
        let mut input = base.clone();
        input.managed_layouts = layouts;
        let mut modules = vec![input];
        let projections = install_native_aggregate_projection_exports(&mut modules);
        assert!(projections[0].scalar_entry.is_none());
        assert!(projections[0].scalar_field.is_none());
        assert_eq!(modules[0].functions.len(), 1);
    }
}

#[test]
fn out_of_bounds_projection_and_wrong_semantic_fail_closed() {
    let mut modules = vec![module(project(32, NativeExpr::Param(0)))];
    assert!(install_native_aggregate_projection_exports(&mut modules)[0]
        .scalar_entry
        .is_none());
    let foreign = SemanticTypeId::from_canonical("Named(app.Other)").unwrap();
    modules[0].functions[0].params = vec![NativeType::ManagedRef(foreign)];
    let projections = install_native_aggregate_projection_exports(&mut modules);
    assert_eq!(projections[0].fields, AggregateFieldProjection::Complete);
    assert!(projections[0].scalar_entry.is_none());
}
