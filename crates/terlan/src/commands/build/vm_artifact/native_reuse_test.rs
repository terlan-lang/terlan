//! Adversarial tests for source-to-image reuse bindings.

use super::{parse_stamp, render_stamp, NativeReuseStamp};

/// Builds one canonical stamp accepted by the production parser.
fn stamp() -> NativeReuseStamp {
    NativeReuseStamp {
        source_sha256: "a".repeat(64),
        image_name: "app.tvm".to_string(),
        input_sha256: "b".repeat(64),
        policy: "development-v1".to_string(),
        target: "x86_64-unknown-linux-gnu".to_string(),
        adapter_abi: "public-adapter-abi-1:test".to_string(),
        linker_policy: "linker-policy-1".to_string(),
    }
}

/// Proves every admission field survives canonical rendering and parsing.
#[test]
fn native_reuse_stamp_round_trips_every_source_target_and_abi_field() {
    let expected = stamp();
    let text = render_stamp(&expected).expect("render reuse stamp");
    assert_eq!(parse_stamp(&text), Some(expected));
}

/// Proves redirecting any independently meaningful field breaks its binding.
#[test]
fn native_reuse_stamp_rejects_poisoned_source_image_target_abi_and_policy_fields() {
    let text = render_stamp(&stamp()).expect("render reuse stamp");
    for (name, poisoned) in [
        ("source", text.replacen(&"a".repeat(64), &"c".repeat(64), 1)),
        ("input", text.replacen(&"b".repeat(64), &"d".repeat(64), 1)),
        ("image", text.replacen("app.tvm", "other.tvm", 1)),
        ("policy", text.replacen("development-v1", "release-v1", 1)),
        (
            "target",
            text.replacen("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", 1),
        ),
        (
            "ABI",
            text.replacen("public-adapter-abi-1:test", "public-adapter-abi-2:test", 1),
        ),
        (
            "linker policy",
            text.replacen("linker-policy-1", "linker-policy-2", 1),
        ),
    ] {
        assert!(parse_stamp(&poisoned).is_none(), "accepted poisoned {name}");
    }
}

/// Proves malformed, traversing, incomplete, and extended records fail closed.
#[test]
fn native_reuse_stamp_rejects_noncanonical_and_incomplete_records() {
    let text = render_stamp(&stamp()).expect("render reuse stamp");
    let without_binding = text
        .lines()
        .filter(|line| !line.starts_with("binding-sha256 "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(parse_stamp(&without_binding).is_none());
    assert!(parse_stamp(&(text.clone() + "unexpected true\n")).is_none());

    let mut traversal = stamp();
    traversal.image_name = "../app.tvm".to_string();
    assert!(render_stamp(&traversal).is_err());

    let mut uppercase_digest = stamp();
    uppercase_digest.input_sha256 = "A".repeat(64);
    assert!(render_stamp(&uppercase_digest).is_err());

    let mut delimiter = stamp();
    delimiter.target = "x86_64\npoisoned".to_string();
    assert!(render_stamp(&delimiter).is_err());
}
