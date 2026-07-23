use std::sync::Arc;

use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_string_append_operation,
    encode_string_prepend_projected_literal_operation, SemanticTypeId,
};
use crate::terlan_typeck::CoreType;

use super::{native_request_projections, NativeExpr, NativeFunction, NativeModule, NativeType};

fn request_semantic() -> SemanticTypeId {
    SemanticTypeId::from_canonical(&CoreType::Named("Request".to_string()).contract_text())
        .expect("request semantic")
}

fn project(field: usize, argument: NativeExpr) -> NativeExpr {
    NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_aggregate_field_operation(request_semantic(), field)
                .expect("request projection"),
        ),
        args: vec![argument],
    }
}

fn module(body: NativeExpr) -> NativeModule {
    NativeModule {
        name: "app.Handler".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "handle".to_string(),
            public: true,
            arity: 1,
            callable_captures: Vec::new(),
            params: vec![NativeType::ManagedRef(request_semantic())],
            return_type: NativeType::Int,
            body,
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn projection(body: NativeExpr) -> RequestFieldProjection {
    native_request_projections(&[module(body)])
        .into_iter()
        .next()
        .expect("handler projection")
        .fields
}

#[test]
fn exact_accessors_produce_a_narrow_field_set() {
    let fields = projection(NativeExpr::Let {
        bindings: vec![
            project(RequestFieldProjection::METHOD, NativeExpr::Param(0)),
            project(RequestFieldProjection::BODY, NativeExpr::Param(0)),
        ],
        body: Box::new(NativeExpr::Int(200)),
    });

    assert!(fields.requires(RequestFieldProjection::METHOD));
    assert!(fields.requires(RequestFieldProjection::BODY));
    assert!(!fields.requires(RequestFieldProjection::HEADERS));
}

#[test]
fn fused_projected_string_operation_retains_a_narrow_field_set() {
    let fields = projection(NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_string_prepend_projected_literal_operation(
                request_semantic(),
                RequestFieldProjection::BODY,
                "prefix:",
            )
            .expect("fused projection"),
        ),
        args: vec![NativeExpr::Param(0)],
    });

    assert_eq!(
        fields,
        RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY)
    );
}

#[test]
fn let_alias_of_request_remains_provably_projectable() {
    let fields = projection(NativeExpr::Let {
        bindings: vec![NativeExpr::Param(0)],
        body: Box::new(project(RequestFieldProjection::QUERY, NativeExpr::Param(1))),
    });

    assert_eq!(fields, RequestFieldProjection::Fields(1 << 6));
}

#[test]
fn request_escape_through_call_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::Call {
            function: 1,
            args: vec![NativeExpr::Param(0)],
        }),
        RequestFieldProjection::Complete
    );
}

#[test]
fn request_use_by_unknown_managed_operation_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_string_append_operation()),
            args: vec![NativeExpr::Param(0), NativeExpr::Param(0)],
        }),
        RequestFieldProjection::Complete
    );
}

#[test]
fn returning_request_falls_back_to_complete() {
    assert_eq!(
        projection(NativeExpr::Param(0)),
        RequestFieldProjection::Complete
    );
}

#[test]
fn unused_request_produces_an_empty_projection() {
    assert_eq!(
        projection(NativeExpr::Int(204)),
        RequestFieldProjection::empty()
    );
}
