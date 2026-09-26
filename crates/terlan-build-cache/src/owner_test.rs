use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct Fixture(pub(super) PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn fixture() -> Fixture {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("terlan-build-owner-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).unwrap();
    Fixture(path)
}

fn args(receipt: &str, input: &str, output: &str) -> Vec<OsString> {
    args_with_script(receipt, input, output, "printf generated > target/output")
}

fn args_with_script(receipt: &str, input: &str, output: &str, script: &str) -> Vec<OsString> {
    [
        "--receipt",
        receipt,
        "--input-sha256",
        input,
        "--timeout-seconds",
        "10",
        "--output",
        output,
        "--",
        "/bin/sh",
        "-c",
        script,
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[test]
fn failed_owner_never_publishes_a_success_receipt() {
    let fixture = fixture();
    fs::create_dir(fixture.0.join("target")).unwrap();
    let input = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let options = args_with_script(
        "target/owner.json",
        input,
        "target/output",
        "printf partial > target/output; exit 17",
    );
    assert!(!run(&fixture.0, &options).unwrap());
    assert!(!fixture.0.join("target/owner.json").exists());
    assert!(!fixture.0.join("target/owner.json.pending").exists());
}

#[test]
fn owner_runs_once_then_reuses_matching_output_receipt() {
    let fixture = fixture();
    fs::create_dir(fixture.0.join("target")).unwrap();
    let input = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let options = args("target/owner.json", input, "target/output");
    assert!(run(&fixture.0, &options).unwrap());
    let first = fs::read(fixture.0.join("target/owner.json")).unwrap();
    assert!(run(&fixture.0, &options).unwrap());
    assert_eq!(
        first,
        fs::read(fixture.0.join("target/owner.json")).unwrap()
    );
}

#[test]
fn owner_rejects_invalid_input_and_receipt_paths() {
    let fixture = fixture();
    let input = "not-a-fingerprint";
    assert!(parse(&fixture.0, &args("owner.json", input, "output")).is_err());
    let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let mut escaped = args("../owner.json", valid, "output");
    assert!(parse(&fixture.0, &escaped).is_err());
    escaped[1] = OsString::from("owner.json");
    escaped.retain(|value| value != "--output");
    assert!(parse(&fixture.0, &escaped).is_err());
}

#[test]
fn owner_rejects_stale_output_bytes_and_replaces_receipt_atomically() {
    let fixture = fixture();
    fs::create_dir(fixture.0.join("target")).unwrap();
    let input = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let options = args("target/owner.json", input, "target/output");
    assert!(run(&fixture.0, &options).unwrap());
    fs::write(fixture.0.join("target/output"), b"mutated").unwrap();
    assert!(run(&fixture.0, &options).unwrap());
    assert!(!fixture.0.join("target/owner.json.pending").exists());
    let document: Value =
        serde_json::from_slice(&fs::read(fixture.0.join("target/owner.json")).unwrap()).unwrap();
    assert_eq!(document["outcome"], "pass");
}

#[test]
fn owner_rejects_redirected_outputs_before_launch() {
    use std::os::unix::fs::symlink;
    let external = fixture();
    let fixture = fixture();
    symlink(&external.0, fixture.0.join("target")).unwrap();
    let options = args("owner.json", &"a".repeat(64), "target/output");
    assert!(run(&fixture.0, &options).is_err());
    assert!(!external.0.join("output").exists());
}

#[test]
fn owner_rejects_redirected_receipts_before_launch() {
    use std::os::unix::fs::symlink;
    let external = fixture();
    let fixture = fixture();
    fs::create_dir(fixture.0.join("target")).unwrap();
    symlink(&external.0, fixture.0.join("receipts")).unwrap();
    let options = args("receipts/owner.json", &"a".repeat(64), "target/output");
    assert!(run(&fixture.0, &options).is_err());
    assert!(!fixture.0.join("target/output").exists());
    assert!(!external.0.join("owner.json").exists());
}
