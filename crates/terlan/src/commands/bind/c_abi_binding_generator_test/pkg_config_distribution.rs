//! Package-owned pkg-config adapter generation and incremental builds.

use super::*;

#[test]
pub(super) fn pkg_config_external_c_distribution_compiles_package_owned_adapter_sources() {
    let manifest = write_fixture_variant("pkg_config_external_link", |metadata| {
        metadata["c_metadata"]["sources"] = serde_json::json!(["external_adapter.c"]);
        metadata["c_metadata"]["header"] = Value::String("native_boundary.h".to_string());
        metadata["c_metadata"]["external_link"] = serde_json::json!({
            "pkg_config": {
                "package": "libpq",
                "min_version": "14.0",
                "static_link": true
            }
        });
    });
    fs::write(
        manifest
            .parent()
            .expect("variant root")
            .join("external_adapter.c"),
        "#include <libpq-fe.h>\nint terlan_libpq_probe(void) { return PQlibVersion() > 0; }\n",
    )
    .expect("write libpq adapter source");
    let out_dir = temp_dir("pkg_config_external_link_out");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate pkg-config linked C ABI package");
    assert!(out_dir
        .join("native/rust/include/native_boundary.h")
        .is_file());
    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("pkg-config = \"=0.3.33\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("probe.statik(true)"));
    assert!(build.contains("probe.atleast_version(\"14.0\")"));
    assert!(contains_ignoring_whitespace(
        &build,
        "probe.probe(\"libpq\")"
    ));
    assert!(build.contains("library.include_paths"));
    assert!(!build.contains("PathBuf"));

    // This adapter gets its headers exclusively from pkg-config, as libpq
    // does. An absent local header directory must not invalidate every build.
    fs::remove_dir_all(out_dir.join("native/rust/include")).expect("remove unused fixture headers");
    let target_dir = temp_dir("pkg_config_external_link_target");
    let build_output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .args(["--offline", "--quiet", "--lib"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("build generated libpq adapter");
    assert!(
        build_output.status.success(),
        "pkg-config adapter build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    let incremental =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .args(["--offline", "--lib", "--message-format=json"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("repeat build to verify Cargo freshness");
    assert!(incremental.status.success(), "{incremental:?}");
    let artifacts: Vec<Value> = String::from_utf8_lossy(&incremental.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["reason"] == "compiler-artifact")
        .collect();
    assert!(!artifacts.is_empty(), "Cargo must report checked artifacts");
    assert!(
        artifacts.iter().all(|event| event["fresh"] == true),
        "unchanged generated adapter must reuse every artifact: {artifacts:?}"
    );

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant root");
    fs::remove_dir_all(out_dir).expect("remove output");
    fs::remove_dir_all(target_dir).expect("remove target");
}

#[test]
pub(super) fn checked_libpq_package_matches_deterministic_regeneration() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std/native/libpq");
    let checked = package.join("generated");
    let regenerated = temp_dir("checked_libpq_regeneration");

    generate_c_abi_bindings(&package.join("native-binding.json"), &regenerated)
        .expect("regenerate checked libpq package");
    for relative in [
        "native/rust/build.rs",
        "native/rust/src/lib.rs",
        "native/rust/src/bin/native_boundary_helper.rs",
    ] {
        let status = std::process::Command::new("rustfmt")
            .args(["--edition", "2021"])
            .arg(regenerated.join(relative))
            .status()
            .expect("run rustfmt over regenerated libpq Rust source");
        assert!(status.success(), "rustfmt failed for {relative}");
    }
    let checked_files = generated_files(&checked);
    assert_eq!(checked_files, generated_files(&regenerated));
    for relative in checked_files {
        assert_eq!(
            fs::read(checked.join(&relative)).expect("read checked generated file"),
            fs::read(regenerated.join(&relative)).expect("read regenerated file"),
            "generated libpq drift in {}",
            relative.display()
        );
    }

    fs::remove_dir_all(regenerated).expect("remove regenerated package");
}
