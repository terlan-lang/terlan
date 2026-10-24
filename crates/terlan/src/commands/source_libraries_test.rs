use super::*;

#[test]
fn library_lookup_preserves_literal_keyword_namespaces_and_finds_dom_bindings() {
    let root = crate::support::test_fs::temp_path("source_libraries", "namespace_layout");
    for relative in ["std/template/Html.terl", "std/js/dom/Element.terl"] {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
    let active = root.join("std/http/Consumer.terl");
    assert_eq!(
        imported_std_source_path("std.template.Html", &active),
        Some(root.join("std/template/Html.terl"))
    );
    assert_eq!(
        imported_std_source_path("std.js.Dom.Element", &active),
        Some(root.join("std/js/dom/Element.terl"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn application_sources_resolve_across_manifest_source_roots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("terlan.toml"), "[package]\nname = \"example\"\nversion = \"0.0.1\"\n[build]\nsource_roots = [\"server\", \"client\"]\n").unwrap();
    let shared = root.path().join("server/example/shared/Messages.terl");
    std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
    std::fs::write(
        &shared,
        "module example.shared.Messages. pub kind(): String -> \"sync\".",
    )
    .unwrap();
    let active = root.path().join("client/example/ui/Protocol.terl");
    assert_eq!(
        imported_std_source_path("example.shared.Messages", &active),
        Some(shared)
    );
}
