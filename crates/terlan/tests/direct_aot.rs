use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use object::{Object, ObjectSection, ObjectSymbol};

#[path = "support/aot_fixture_directory.rs"]
mod fixture_directory;
use fixture_directory::FixtureDirectory;

#[path = "support/direct_aot_managed.rs"]
mod managed_suite;
use managed_suite::assert_direct_managed_execution;
#[path = "support/direct_aot_rejection.rs"]
mod rejection_suite;
#[path = "support/direct_aot_shard.rs"]
mod shard_suite;
#[test]
fn terlan_consumer_executes_descriptor_bearing_tvm_image() {
    let directory = FixtureDirectory::new("direct-aot");
    let root = directory.path();
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::write(&source, shard_suite::source()).expect("write Terlan AOT consumer fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start terlc");
    assert!(
        build.status.success(),
        "terlc failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let cached_build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env("TERLAN_NATIVE_CACHE_MISS_POLICY", "error")
        .output()
        .expect("start cached terlc build");
    assert!(
        cached_build.status.success(),
        "cached build repeated native work:\n{}",
        String::from_utf8_lossy(&cached_build.stderr)
    );

    let image_path = output_dir.join("vm/direct_aot.tvm");
    assert_eq!(
        image_path.extension().and_then(|value| value.to_str()),
        Some("tvm")
    );
    let image_bytes = fs::read(&image_path).expect("read TVM image");
    let image = object::File::parse(&*image_bytes).expect("parse TVM image");
    assert!(image.symbols().any(|symbol| {
        symbol.name().is_ok_and(|name| {
            name == "terlan_native_dispatch_v3" || name == "_terlan_native_dispatch_v3"
        })
    }));
    assert!(
        image.entry() != 0
            || image.symbols().any(|symbol| {
                symbol.name().is_ok_and(|name| {
                    name == "terlan_tvm_image_entry_v1" || name == "_terlan_tvm_image_entry_v1"
                })
            })
    );
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("embedded TVM descriptor section")
        .data()
        .expect("read embedded TVM descriptor");
    assert_eq!(&descriptor[..8], b"TVMDSC01");
    assert!(output_dir.join(".terlan/native-aot").is_dir());
    assert!(!output_dir.join("vm/native").exists());
    shard_suite::assert_execution(&image_path);
    // Native status, capture shape, and resume-authority assertions use the
    // real ABI-3 backend in direct_backend_compiled_test. Application calls
    // below exercise the production VM; the capability worker does not run AOT images.

    let load = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("load")
        .arg(&image_path)
        .output()
        .expect("load self-describing TVM image");
    assert!(
        load.status.success(),
        "terlan-vm could not load the native image:\n{}",
        String::from_utf8_lossy(&load.stderr)
    );

    let run = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("main")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run Terlan consumer");
    assert!(
        run.status.success(),
        "terlan-vm failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let yielded = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run yielded Terlan consumer");
    assert!(
        yielded.status.success(),
        "terlan-vm could not resume native continuation:\n{}",
        String::from_utf8_lossy(&yielded.stderr)
    );

    assert_direct_managed_execution(&image_path, descriptor);

    let yielded_local = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded_local")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run live-local Terlan consumer");
    assert!(
        yielded_local.status.success(),
        "terlan-vm could not resume live local:\n{}",
        String::from_utf8_lossy(&yielded_local.stderr)
    );

    let yielded_twice = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded_twice")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run repeated-yield Terlan consumer");
    assert!(
        yielded_twice.status.success(),
        "terlan-vm could not drive repeated native transitions:\n{}",
        String::from_utf8_lossy(&yielded_twice.stderr)
    );

    let sent_to_self = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("send_to_self")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run native Send Terlan consumer");
    assert!(
        sent_to_self.status.success(),
        "terlan-vm could not service native Send transition:\n{}",
        String::from_utf8_lossy(&sent_to_self.stderr)
    );

    let received_from_self = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("send_then_receive_call")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run native Receive Terlan consumer");
    assert!(
        received_from_self.status.success(),
        "terlan-vm could not service native Receive transition:\n{}",
        String::from_utf8_lossy(&received_from_self.stderr)
    );

    let mut repl = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("repl")
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env_remove("TERLAN_NATIVE_WORKER")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start direct-AOT REPL");
    repl.stdin
        .as_mut()
        .expect("REPL stdin")
        .write_all(b"1 + 2.\n:quit\n")
        .expect("write REPL expression");
    drop(repl.stdin.take());
    let repl_output = repl.wait_with_output().expect("wait for REPL");
    assert!(repl_output.status.success());
    assert!(String::from_utf8_lossy(&repl_output.stdout).contains("repl> 3\n"));

    directory.close();
}

/// Proves memory introspection survives source checking, AOT lowering, and VM execution.
#[test]
fn terlan_memory_intrinsics_execute_in_a_native_image() {
    let directory = FixtureDirectory::new("direct-aot-memory");
    let root = directory.path();
    let source = root.join("memory_aot.terl");
    let output = root.join("build");
    fs::write(&source, include_str!("fixtures/memory_aot.terl")).expect("write memory fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output)
        .output()
        .expect("start terlc");
    assert!(
        build.status.success(),
        "terlc failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let execution = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(output.join("vm/memory_aot.tvm"))
        .arg("--entry")
        .arg("memory_aot.memory_contract")
        .arg("--test-eval")
        .output()
        .expect("start terlan-vm");
    assert!(
        execution.status.success(),
        "memory contract failed:\n{}",
        String::from_utf8_lossy(&execution.stderr)
    );

    directory.close();
}
