use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-support-bundle-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("fixture directory");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"fixture\"\nsecret = \"must-not-leak\"\n",
    )
    .expect("manifest");
    root
}

#[test]
fn support_bundle_is_deterministic_and_redacted() {
    let root = fixture();
    let first = root.join("first.json");
    let second = root.join("second.json");
    write_bundle(&Arguments {
        target: root.clone(),
        diagnostic: None,
        output: first.clone(),
    })
    .expect("first bundle");
    write_bundle(&Arguments {
        target: root.clone(),
        diagnostic: None,
        output: second.clone(),
    })
    .expect("second bundle");
    let first_bytes = fs::read(first).expect("first bytes");
    assert_eq!(first_bytes, fs::read(second).expect("second bytes"));
    let text = String::from_utf8(first_bytes).expect("UTF-8");
    assert!(!text.contains("must-not-leak"));
    assert!(!text.contains(&root.display().to_string()));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn support_bundle_rejects_malformed_diagnostic_report() {
    let root = fixture();
    let diagnostic = root.join("diagnostic.json");
    fs::write(&diagnostic, "not-json").expect("diagnostic");
    let error = write_bundle(&Arguments {
        target: root.clone(),
        diagnostic: Some(diagnostic),
        output: root.join("bundle.json"),
    })
    .expect_err("malformed diagnostic must fail");
    assert!(error.to_string().contains("not valid JSON"));
    fs::remove_dir_all(root).expect("cleanup");
}
