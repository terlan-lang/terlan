use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

#[test]
fn package_build_emits_one_tvm_image_with_qualified_module_exports() {
    let root = std::env::temp_dir().join(format!(
        "terlan-package-aot-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let project = root.join("project");
    let source_dir = project.join("src/app");
    let output_dir = root.join("build");
    fs::create_dir_all(&source_dir).expect("create package source root");
    fs::write(
        project.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write package manifest");
    fs::write(
        source_dir.join("Main.terl"),
        "module app.Main.\n\nimport app.Math.\n\nprivate_value(): Int ->\n    99.\n\npub value(): Int ->\n    Math.value() + 34.\n\npub main(): Bool ->\n    value() == 41.\n",
    )
    .expect("write package main module");
    let math_source = source_dir.join("Math.terl");
    fs::write(
        &math_source,
        "module app.Math.\n\npub math_value(): Int ->\n    7.\n\npub value(): Int ->\n    math_value().\n",
    )
    .expect("write package math module");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&project)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start package build");
    assert!(
        build.status.success(),
        "package build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let image_name = "app_Main.tvm";
    let image_path = output_dir.join("vm").join(image_name);
    assert!(!output_dir.join("vm/app_Main.tvm.json").exists());
    assert!(!output_dir.join("vm/app_Math.tvm.json").exists());
    let original_sha256 = Sha256::digest(fs::read(&image_path).expect("read package TVM image"));

    fs::write(output_dir.join("vm/stale.tvm"), b"stale image")
        .expect("write stale deployable image fixture");
    fs::write(
        &math_source,
        "module app.Math.\n\npub math_value(): Int ->\n    8.\n\npub value(): Int ->\n    math_value().\n",
    )
    .expect("change package math module");
    let changed_build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("build")
        .arg(&project)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start changed package build");
    assert!(
        changed_build.status.success(),
        "changed package build failed:\n{}",
        String::from_utf8_lossy(&changed_build.stderr)
    );
    assert_ne!(
        Sha256::digest(fs::read(&image_path).expect("read changed package image")),
        original_sha256
    );
    assert!(!output_dir.join("vm/stale.tvm").exists());

    fs::write(
        &math_source,
        "module app.Math.\n\npub math_value(): Int ->\n    7.\n\npub value(): Int ->\n    math_value().\n",
    )
    .expect("restore package math module");
    let restored_build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("build")
        .arg(&project)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env("TERLAN_NATIVE_CACHE_MISS_POLICY", "error")
        .output()
        .expect("start restored package build");
    assert!(
        restored_build.status.success(),
        "restored cache build repeated native work:\n{}",
        String::from_utf8_lossy(&restored_build.stderr)
    );
    assert_eq!(
        Sha256::digest(fs::read(&image_path).expect("read restored package image")),
        original_sha256
    );

    let vm_dir = output_dir.join("vm");
    let tvm_images = fs::read_dir(&vm_dir)
        .expect("read VM output directory")
        .map(|entry| entry.expect("read VM output entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("tvm"))
        .collect::<Vec<_>>();
    assert_eq!(
        tvm_images.len(),
        1,
        "package must emit exactly one TVM image"
    );
    let image_path = &tvm_images[0];
    assert_eq!(
        image_path.file_name().and_then(|value| value.to_str()),
        Some(image_name)
    );

    let main_value = native_export_id("app.Main", "value", 0);
    let math_value = native_export_id("app.Math", "math_value", 0);
    assert_ne!(main_value, math_value, "qualified exports must not collide");

    let private_entry = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(image_path)
        .arg("--entry")
        .arg("app.Main.private_value")
        .output()
        .expect("run private package entry");
    assert!(!private_entry.status.success());
    assert!(String::from_utf8_lossy(&private_entry.stderr)
        .contains("error[pure_native_export_missing]"));

    let run = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(image_path)
        .arg("--entry")
        .arg("app.Main.main")
        .arg("--test-eval")
        .output()
        .expect("run qualified package entry");
    assert!(
        run.status.success(),
        "qualified package entry failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let direct_execution = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(image_path)
        .arg("--entry")
        .arg("app.Main.main")
        .arg("--test-eval")
        .env("TERLAN_NATIVE_WORKER", root.join("worker-must-not-run"))
        .output()
        .expect("run package entry without application worker");
    assert!(
        direct_execution.status.success(),
        "application execution attempted to use TERLAN_NATIVE_WORKER:\n{}",
        String::from_utf8_lossy(&direct_execution.stderr)
    );

    let ambiguous = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(image_path)
        .arg("--entry")
        .arg("value")
        .output()
        .expect("run ambiguous package entry");
    assert!(!ambiguous.status.success());
    assert!(
        String::from_utf8_lossy(&ambiguous.stderr).contains("error[pure_native_export_ambiguous]")
    );

    fs::remove_dir_all(&root).expect("remove package AOT fixture root");
}

fn native_export_id(module: &str, function: &str, arity: usize) -> u64 {
    let mut digest = Sha256::new();
    digest.update(b"terlan-tvm-export-v1\0");
    digest.update(module.as_bytes());
    digest.update(b"\0");
    digest.update(function.as_bytes());
    digest.update(b"\0");
    digest.update(arity.to_le_bytes());
    let bytes = digest.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 export prefix")).max(1)
}
