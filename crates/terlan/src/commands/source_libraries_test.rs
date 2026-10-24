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
fn standard_sources_preserve_contextual_keywords_and_generated_directory_names() {
    let active = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    for (module, expected) in [
        ("std.css.Values", "std/css/Values.terl"),
        ("std.css.properties.Width", "std/css/properties/Width.terl"),
        ("std.core.Int", "std/core/Int.terl"),
        ("std.js.Dom.Window", "std/js/dom/Window.terl"),
    ] {
        let path =
            imported_std_source_path(module, &active).unwrap_or_else(|| panic!("missing {module}"));
        assert!(path.ends_with(expected), "{module}: {}", path.display());
    }
}
