use super::*;

use crate::support::test_fs::temp_dir as shared_temp_dir;

/// Writes a runnable Terlan script fixture.
fn write_script(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create script parent");
    }
    fs::write(path, "Unit.\n").expect("write script");
}

/// Verifies runnable scripts are discovered from the conventional scripts tree.
#[test]
fn discover_project_scripts_lists_convention_scripts() {
    let root = shared_temp_dir("scripts_command", "lists_convention_scripts");
    let script = root.join("scripts").join("SeedDatabase.terls");
    write_script(&script);

    let scripts = discover_project_scripts(&root).expect("discover scripts");

    assert_eq!(
        scripts,
        vec![ProjectScriptEntry {
            name: "seed_database".to_string(),
            path: script,
            configured: false,
        }]
    );
}

/// Verifies manifest aliases override discovered names with stable labels.
#[test]
fn discover_project_scripts_marks_configured_aliases() {
    let root = shared_temp_dir("scripts_command", "marks_configured_aliases");
    let script = root.join("scripts").join("SeedDatabase.terls");
    write_script(&script);
    fs::write(
        root.join("terlan.toml"),
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
seed = \"scripts/SeedDatabase.terls\"
",
    )
    .expect("write manifest");

    let scripts = discover_project_scripts(&root).expect("discover scripts");

    assert_eq!(scripts.len(), 2);
    assert!(scripts.iter().any(|script| script.name == "seed_database"));
    assert!(scripts
        .iter()
        .any(|script| script.name == "seed" && script.configured));
}

/// Verifies named script resolution returns the executable source path.
#[test]
fn resolve_project_script_accepts_configured_alias() {
    let root = shared_temp_dir("scripts_command", "resolve_configured_alias");
    let script = root.join("scripts").join("SmokeHttp.terls");
    write_script(&script);
    fs::write(
        root.join("terlan.toml"),
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
smoke = \"scripts/SmokeHttp.terls\"
",
    )
    .expect("write manifest");

    assert_eq!(
        resolve_project_script(&root, "smoke").expect("resolve script"),
        script
    );
}

/// Verifies configured aliases fail fast when they use module source syntax.
#[test]
fn discover_project_scripts_rejects_configured_module_source() {
    let root = shared_temp_dir("scripts_command", "rejects_configured_module_source");
    let script = root.join("scripts").join("Broken.terl");
    fs::create_dir_all(script.parent().expect("script parent")).expect("create scripts dir");
    fs::write(&script, "module scripts.Broken.\n").expect("write broken script");
    fs::write(
        root.join("terlan.toml"),
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
broken = \"scripts/Broken.terl\"
",
    )
    .expect("write manifest");

    let message = discover_project_scripts(&root).expect_err("expected invalid script");

    assert!(message.contains("must point to a .terls file"), "{message}");
}

/// Verifies manifest aliases cannot silently shadow a different convention
/// script with the same discovered name.
#[test]
fn discover_project_scripts_rejects_configured_alias_shadowing_discovered_script() {
    let root = shared_temp_dir("scripts_command", "rejects_shadowing_alias");
    write_script(&root.join("scripts").join("SeedDatabase.terls"));
    write_script(&root.join("scripts").join("Other.terls"));
    fs::write(
        root.join("terlan.toml"),
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
seed_database = \"scripts/Other.terls\"
",
    )
    .expect("write manifest");

    let message = discover_project_scripts(&root).expect_err("expected alias conflict");

    assert!(
        message.contains("conflicts with discovered script"),
        "{message}"
    );
    assert!(message.contains("seed_database"), "{message}");
    assert!(message.contains("Other.terls"), "{message}");
    assert!(message.contains("SeedDatabase.terls"), "{message}");
}
