use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::analyze_syntax_bindings;

#[test]
fn navigation_index_keeps_nested_same_spelled_bindings_separate() {
    let source = r#"
module lsp_binding_identity.
run(value: Int): Int ->
    case value {
        value -> value
    }.
"#;
    let module = parse_module_as_syntax_output(source).expect("parse LSP binding fixture");
    let analysis = analyze_syntax_bindings(&module);
    let index = BindingNavigationIndex::build(source, &analysis);
    let values = analysis
        .evidence
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    let outer = index.occurrences_for(values[0].id);
    let inner = index.occurrences_for(values[1].id);
    assert_eq!(outer.len(), 2, "{outer:#?}");
    assert_eq!(inner.len(), 2, "{inner:#?}");
    assert_ne!(outer[0].binding, inner[0].binding);
}

#[test]
fn duplicate_code_action_targets_only_the_second_declaration() {
    let source = r#"
module lsp_duplicate_action.
run(value: Int): Int ->
    let value = value + 1;
    value.
"#;
    let module = parse_module_as_syntax_output(source).expect("parse duplicate fixture");
    let analysis = analyze_syntax_bindings(&module);
    let collision = analysis.collisions.first().expect("collision");
    let message = collision.diagnostic().message;
    let (span, replacement) =
        duplicate_binding_replacement(source, &analysis, 0, source.len(), &message)
            .expect("duplicate replacement");
    assert_eq!(&source[span.start..span.end], "value");
    assert_eq!(replacement, "value_2");
    assert!(span.start > source.find("run(value").expect("outer binding"));
}

#[test]
fn exact_inner_rename_leaves_outer_and_sibling_occurrences_unchanged() {
    let source = r#"
module lsp_exact_binding_rename.
run(value: Int, flag: Bool): Int ->
    case flag {
        true ->
            case value {
                value -> value
            };
        false ->
            case value {
                value -> value
            }
    }.
"#;
    let module = parse_module_as_syntax_output(source).expect("parse exact rename fixture");
    let analysis = analyze_syntax_bindings(&module);
    let index = BindingNavigationIndex::build(source, &analysis);
    let values = analysis
        .evidence
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 3, "{values:#?}");

    let mut renamed = source.to_string();
    let mut occurrences = index.occurrences_for(values[1].id);
    assert_eq!(occurrences.len(), 2, "{occurrences:#?}");
    occurrences.sort_by_key(|occurrence| std::cmp::Reverse(occurrence.span.start));
    for occurrence in occurrences {
        renamed.replace_range(occurrence.span.start..occurrence.span.end, "nested");
    }

    assert_eq!(renamed.matches("nested").count(), 2, "{renamed}");
    assert_eq!(renamed.matches("value").count(), 5, "{renamed}");
    assert!(renamed.contains("run(value: Int"), "{renamed}");
    assert_eq!(
        renamed.matches("case value").count(),
        2,
        "outer and sibling scrutinees changed: {renamed}"
    );
}
