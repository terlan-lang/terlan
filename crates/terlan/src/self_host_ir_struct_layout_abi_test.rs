use super::*;

fn envelope(abi_minor: u32, field: IrStructField) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: ABI_MAJOR,
        abi_minor,
        target: "native".into(),
        module: IrModule {
            name: "example.layout".into(),
            structs: vec![IrStructLayout {
                name: "example.layout.Point".into(),
                fields: vec![field],
            }],
            functions: Vec::new(),
        },
    }
}

#[test]
fn abi_1_2_accepts_typed_struct_fields() {
    let diagnostics = validate(&envelope(
        2,
        IrStructField::Typed {
            name: "x".into(),
            type_name: "Int".into(),
        },
    ));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn abi_1_2_rejects_legacy_struct_fields() {
    let diagnostics = validate(&envelope(2, IrStructField::Legacy("x".into())));

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "legacy-struct-field-in-current-abi"));
}

#[test]
fn abi_1_1_retains_legacy_struct_field_compatibility() {
    let diagnostics = validate(&envelope(1, IrStructField::Legacy("x".into())));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
