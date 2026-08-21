use super::*;

#[test]
fn rejects_secret_and_unbounded_values_before_host_boundary() {
    assert!(matches!(
        FieldSet::try_new([Field {
            name: "access_token".into(),
            value: Scalar::String("hidden".into()),
        }]),
        Err(FieldError::SecretBearingName(_))
    ));
    assert_eq!(
        FieldSet::try_new([Field {
            name: "message".into(),
            value: Scalar::String("x".repeat(MAX_FIELD_VALUE_BYTES + 1)),
        }]),
        Err(FieldError::ValueTooLong)
    );
}
