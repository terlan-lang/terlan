use super::*;

fn projection(fields: &[usize]) -> NativeAggregateProjection {
    NativeAggregateProjection {
        module: "app".into(),
        function: "handle".into(),
        arity: 1,
        semantic: SemanticTypeId::from_canonical("Named(Request)").unwrap(),
        fields: AggregateFieldProjection::Fields(fields.iter().copied().collect()),
        scalar_entry: Some("scalar".into()),
        scalar_field: fields.first().copied(),
        suspending: true,
    }
}

#[test]
fn unrelated_aggregate_is_not_an_http_request() {
    let mut input = projection(&[4]);
    input.semantic = SemanticTypeId::from_canonical("Named(app.Document)").unwrap();
    assert!(request_projections(vec![input]).is_empty());
}

#[test]
fn http_adapter_preserves_verified_fields_and_suspension() {
    let output = request_projections(vec![projection(&[4])]).pop().unwrap();
    assert_eq!(output.fields, RequestFieldProjection::Fields(1 << 4));
    assert_eq!(output.scalar_field, Some(4));
    assert_eq!(output.scalar_entry.as_deref(), Some("scalar"));
    assert!(output.suspending);
    assert_eq!(output.function, "handle");
}

#[test]
fn out_of_contract_fields_cannot_enable_a_scalar_ingress() {
    for fields in [vec![0], vec![10], vec![11], vec![usize::MAX]] {
        let output = request_projections(vec![projection(&fields)])
            .pop()
            .unwrap();
        assert_eq!(output.fields, RequestFieldProjection::Complete);
        assert!(output.scalar_field.is_none());
        assert!(output.scalar_entry.is_none());
    }
    for fields in [vec![], vec![3], vec![4, 5]] {
        let output = request_projections(vec![projection(&fields)])
            .pop()
            .unwrap();
        assert!(output.scalar_field.is_none());
        assert!(output.scalar_entry.is_none());
    }
    let mut input = projection(&[4]);
    input.fields = AggregateFieldProjection::Complete;
    let output = request_projections(vec![input]).pop().unwrap();
    assert_eq!(output.fields, RequestFieldProjection::Complete);
    assert!(output.scalar_entry.is_none());
}
