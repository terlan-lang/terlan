use super::*;

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
