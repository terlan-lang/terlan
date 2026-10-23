use super::*;

#[test]
fn skipped_labels_do_not_collide_with_existing_overload_labels() {
    let skipped = [
        "Element.closest",
        "Element.closest#2",
        "Element.closest",
        "Element.closest#2",
        "Element.closest",
    ]
    .map(|source| DomSkippedDeclaration {
        source: source.into(),
        reason: "ts_bindgen.unsupported_type",
        detail: "unsupported overload".into(),
    });
    let entries = skipped_manifest_entries(&skipped);
    let labels = entries
        .iter()
        .map(|entry| entry["source"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Element.closest",
            "Element.closest#2",
            "Element.closest#3",
            "Element.closest#2#2",
            "Element.closest#4"
        ]
    );
}
