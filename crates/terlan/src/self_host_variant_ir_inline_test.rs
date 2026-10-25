use super::*;

fn document(payload: &str) -> VariantLayoutDocument {
    VariantLayoutDocument {
        schema: VARIANT_LAYOUT_SCHEMA.to_string(),
        variants: vec![VariantLayout {
            owner: "Option".to_string(),
            name: "Some".to_string(),
            discriminant: 0,
            type_parameters: vec!["T".to_string()],
            payload_types: vec![OptionalType {
                known: true,
                name: payload.to_string(),
            }],
        }],
    }
}

#[test]
fn identical_module_catalogs_merge_into_one_application_authority() {
    let first = VariantCatalog::from_document(document("T")).expect("first catalog");
    let second = VariantCatalog::from_document(document("T")).expect("second catalog");
    let merged = VariantCatalog::merge([first, second]).expect("identical layouts merge");
    let resolved = merged
        .resolve("Option", "Some", "Option[Int]")
        .expect("merged constructor resolves");
    assert_eq!(resolved.payload_types, vec![Some("Int".to_string())]);
}

#[test]
fn conflicting_module_catalogs_fail_closed() {
    let first = VariantCatalog::from_document(document("T")).expect("first catalog");
    let second = VariantCatalog::from_document(document("List[T]")).expect("second catalog");
    let diagnostics =
        VariantCatalog::merge([first, second]).expect_err("conflicting layouts must not merge");
    assert!(diagnostics
        .iter()
        .any(|value| value.contains("conflicting variant layout `Option.Some`")));
}
