use super::*;

#[test]
fn shape_provenance_does_not_change_pattern_meaning_or_legacy_decoding() {
    let original = pattern_leaf(SyntaxPatternKind::Var, Some("value".into()));
    let mut annotated = original.clone();
    annotated.expanded_from.push(SyntaxShapeOrigin {
        source_module: "provider".into(),
        source_name: "Wrapped".into(),
    });
    assert_eq!(original, annotated);
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("expanded_from").is_none());
    let decoded: SyntaxPatternOutput = serde_json::from_value(legacy).unwrap();
    assert!(decoded.expanded_from.is_empty());
    assert_eq!(
        serde_json::to_value(&annotated).unwrap()["expanded_from"][0]["source_module"],
        "provider"
    );
}
