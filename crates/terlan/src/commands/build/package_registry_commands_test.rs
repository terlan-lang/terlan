use super::*;

#[test]
fn adds_and_removes_only_the_named_dependency_section_entry() {
    let source = "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\nlocal = { path = \"../local\" }\n\n[build]\nartifact = \"library\"\n";
    let added = add_dependency(
        source,
        "math",
        ">=1.0.0, <2.0.0",
        "https://registry.example.test",
    )
    .unwrap();
    assert!(added.contains(
        "math = { registry = \"https://registry.example.test\", version = \">=1.0.0, <2.0.0\" }"
    ));
    assert!(added.contains("local = { path = \"../local\" }"));
    assert!(added.find("math =").unwrap() < added.find("[build]").unwrap());

    let removed = remove_dependency(&added, "math").unwrap();
    assert_eq!(removed, source);
    assert!(
        add_dependency(&added, "math", "*", "https://registry.example.test")
            .unwrap_err()
            .contains("registry_dependency_exists")
    );
    assert!(remove_dependency(source, "missing")
        .unwrap_err()
        .contains("registry_dependency_missing"));
}

#[test]
fn creates_a_dependency_section_when_absent() {
    let source = "[package]\nname = \"app\"\nversion = \"1.0.0\"\n";
    let added = add_dependency(source, "math", "=1.0.0", "https://registry.example.test").unwrap();
    assert!(added.contains("\n[dependencies]\nmath ="));
}
