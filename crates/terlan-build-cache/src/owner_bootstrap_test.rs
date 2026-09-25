use super::*;
use crate::owner::{run, tests::fixture};
use serde_json::json;
use std::ffi::OsString;
use std::fs;

fn setup(root: &Path) -> (Vec<OsString>, Vec<Value>) {
    fs::create_dir_all(root.join("target/debug")).unwrap();
    let mut rows = Vec::new();
    let mut args: Vec<OsString> = [
        "--receipt",
        "target/support.json",
        "--input-sha256",
        &"a".repeat(64),
        "--timeout-seconds",
        "10",
        "--completed-cargo-log",
        "target/cargo.jsonl",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    for name in ["terlan-build-cache", "terlan-test-orchestrator"] {
        let relative = format!("target/debug/{name}");
        let path = root.join(&relative);
        fs::write(&path, name).unwrap();
        args.extend([OsString::from("--output"), relative.into()]);
        rows.push(
            json!({"reason":"compiler-artifact", "target":{"name":name,"kind":["bin"]},
            "profile":{"test":false}, "executable":path}),
        );
    }
    rows.push(json!({"reason":"build-finished","success":true}));
    (args, rows)
}

fn write_log(root: &Path, rows: &[Value]) {
    let source = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("target/cargo.jsonl"), source).unwrap();
}

#[test]
fn completed_bootstrap_seals_and_warm_owner_never_launches_a_command() {
    let fixture = fixture();
    let (args, rows) = setup(&fixture.0);
    write_log(&fixture.0, &rows);
    assert!(run(&fixture.0, &args).unwrap());
    let receipt = fs::read(fixture.0.join("target/support.json")).unwrap();
    let document: Value = serde_json::from_slice(&receipt).unwrap();
    assert_eq!(
        document["bootstrap_cargo_log_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let mut warm = args.clone();
    warm.drain(6..8);
    warm.extend([OsString::from("--"), OsString::from("/bin/false")]);
    assert!(run(&fixture.0, &warm).unwrap());
    assert_eq!(
        receipt,
        fs::read(fixture.0.join("target/support.json")).unwrap()
    );
    fs::write(
        fixture.0.join("target/debug/terlan-build-cache"),
        "tampered",
    )
    .unwrap();
    assert!(!run(&fixture.0, &warm).unwrap());
}

#[test]
fn incomplete_failed_ambiguous_and_redirected_observations_never_seal() {
    for mode in [
        "truncated",
        "failed",
        "duplicate",
        "trailing",
        "missing",
        "outside",
        "name",
        "test",
        "symlink",
        "malformed",
        "oversized",
    ] {
        let fixture = fixture();
        let (args, mut rows) = setup(&fixture.0);
        match mode {
            "truncated" => {
                rows.pop();
            }
            "failed" => rows[2]["success"] = false.into(),
            "duplicate" => rows.insert(0, rows[0].clone()),
            "trailing" => rows.push(rows[0].clone()),
            "missing" => {
                rows.remove(0);
            }
            "outside" => rows[0]["executable"] = "/tmp/terlan-build-cache".into(),
            "name" => rows[0]["target"]["name"] = "wrong".into(),
            "test" => rows[0]["profile"]["test"] = true.into(),
            _ => {}
        }
        write_log(&fixture.0, &rows);
        if mode == "symlink" {
            fs::rename(
                fixture.0.join("target/cargo.jsonl"),
                fixture.0.join("target/original"),
            )
            .unwrap();
            std::os::unix::fs::symlink("original", fixture.0.join("target/cargo.jsonl")).unwrap();
        } else if mode == "malformed" {
            fs::write(fixture.0.join("target/cargo.jsonl"), "not json").unwrap();
        } else if mode == "oversized" {
            File::create(fixture.0.join("target/cargo.jsonl"))
                .unwrap()
                .set_len(MAX_RECEIPT_BYTES + 1)
                .unwrap();
        }
        assert!(run(&fixture.0, &args).is_err(), "{mode}");
        assert!(!fixture.0.join("target/support.json").exists(), "{mode}");
    }
}
