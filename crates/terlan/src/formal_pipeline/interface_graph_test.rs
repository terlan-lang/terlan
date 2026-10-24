use super::*;

#[test]
fn source_graph_loads_generated_std_parents_before_struct_inclusion() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/typed-css/src/typed_css/Styles.terl");
    let source = std::fs::read_to_string(&path).unwrap();
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(&source).unwrap();
    let graph = SourceInterfaceGraph::collect([(&*path, &syntax)], None);
    assert!(graph.interfaces.contains_key("std.css.properties.Width"));
    assert!(graph.interfaces.contains_key("std.css.properties.Height"));
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &graph.interfaces);
    let (_, diagnostics) = crate::terlan_typeck::expand_syntax_includes(syntax, &resolved.module);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
