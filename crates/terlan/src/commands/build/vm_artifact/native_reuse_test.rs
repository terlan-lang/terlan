//! Adversarial tests for source-to-image reuse bindings.

use super::{parse_stamp, render_stamp, NativeReuseStamp};

#[test]
#[cfg(feature = "native-codegen")]
fn interface_first_warm_native_images_revalidate_qualified_dependencies_without_imports() {
    let directory =
        crate::support::test_fs::TestDirectory::new("interfaces", "native_qualified_dependency");
    let path = directory.path().join("api.terl");
    let dependency = directory.path().join("dep.terli");
    std::fs::write(
        &path,
        "module api. type Local = dep.Value. pub main(): Int -> 7.",
    )
    .unwrap();
    std::fs::write(&dependency, "module dep. pub type Value = Int.").unwrap();
    let state = crate::CliState {
        incremental: true,
        cache_dir: Some(directory.path().join("cache")),
        out_dir: directory.path().join("out"),
        ..crate::CliState::default()
    };
    let policy = crate::compiler::native_ir::NativeCodegenPolicy::Development;
    let build = || {
        super::super::orchestration::build_one_vm_artifact(path.to_str().unwrap(), &state, policy)
    };
    build().expect("publish a valid native image and reuse stamp");
    assert!(state.out_dir.join("vm/api.tvm").is_file());
    std::fs::write(
        &dependency,
        "module dep. pub type Value = Int. pub invalid(): Missing.",
    )
    .unwrap();
    assert!(
        build().is_err(),
        "warm native reuse bypassed source dependency validation"
    );
}

#[test]
fn interface_first_native_reuse_identity_tracks_contract_presence_and_content() {
    let directory =
        crate::support::test_fs::TestDirectory::new("interfaces", "native_reuse_contract");
    let path = directory.path().join("api.terl");
    let policy = crate::compiler::native_ir::NativeCodegenPolicy::Development;
    let source = "module api. pub value(): Int -> 1.";
    let absent = super::source_identity(&path, source, policy);
    std::fs::write(
        path.with_extension("terli"),
        "module api. pub value(): Int.",
    )
    .unwrap();
    let present = super::source_identity(&path, source, policy);
    assert_ne!(absent, present);
    std::fs::write(
        path.with_extension("terli"),
        "module api. pub value(): String.",
    )
    .unwrap();
    assert_ne!(present, super::source_identity(&path, source, policy));
    std::fs::remove_file(path.with_extension("terli")).unwrap();
    assert_eq!(absent, super::source_identity(&path, source, policy));
}

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
