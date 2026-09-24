//! Real source -> native image -> isolated durable worker -> separate VM restore.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Output};

fn succeeds(output: Output) {
    assert!(
        output.status.success(),
        "storage fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires separately built compiler, VM, and adjacent native worker with Linux sandbox"]
fn public_storage_aot_survives_a_complete_vm_restart() {
    use std::os::unix::fs::PermissionsExt;
    let compiler = PathBuf::from(std::env::var_os("TERLAN_TEST_COMPILER").expect("built compiler"));
    let vm =
        PathBuf::from(std::env::var_os("TERLAN_TEST_VM").expect("built VM with adjacent worker"));
    let root = tempfile::tempdir().expect("test build root");
    let durable = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("private durable directory");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/durable_storage.terl");
    let source = root.path().join("durable_storage.terl");
    std::fs::copy(fixture, &source).expect("isolate source and AOT caches in the test directory");
    succeeds(
        Command::new(&compiler)
            .args(["build"])
            .arg(source)
            .args(["--target", "terlan-vm", "--out-dir"])
            .arg(root.path())
            .output()
            .expect("build storage image"),
    );
    let image = root.path().join("vm/durable_storage.tvm");
    let denied = Command::new(&vm)
        .arg("run")
        .arg(&image)
        .args(["--entry", "unbound_open", "--test-eval"])
        .output()
        .expect("run without storage authority");
    succeeds(denied);
    for (entry, succeeds_expected) in [
        ("resources_without_authority_are_unsupported", true),
        ("resources_without_authority_cannot_issue_proof", false),
    ] {
        let output = Command::new(&vm)
            .arg("run")
            .arg(&image)
            .args(["--entry", entry, "--test-eval"])
            .output()
            .unwrap();
        if succeeds_expected {
            succeeds(output);
        } else {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("vm.distributed_storage.proof")
            );
        }
    }
    succeeds(
        Command::new(&vm)
            .arg("run")
            .arg(&image)
            .args(["--entry", "local_storage_lifecycle", "--test-eval"])
            .output()
            .expect("run volatile storage without any worker binding"),
    );
    let denied_proof = Command::new(&vm)
        .arg("run")
        .arg(&image)
        .args([
            "--entry",
            "local_storage_cannot_issue_durable_proof",
            "--test-eval",
        ])
        .output()
        .expect("reject volatile durability claim");
    assert!(!denied_proof.status.success());
    assert!(String::from_utf8_lossy(&denied_proof.stderr).contains("vm.distributed_storage.proof"));
    assert!(!durable.path().join("checkpoints.sqlite").exists());
    for entry in [
        "write_checkpoint",
        "read_checkpoint",
        "write_checkpoint",
        "read_checkpoint",
        "transactional_updates",
        "restore_transactions",
        "durable_replication_is_unsupported",
    ] {
        succeeds(
            Command::new(&vm)
                .arg("run")
                .arg(&image)
                .args(["--entry", entry, "--test-eval", "--storage"])
                .arg(format!("primary={}", durable.path().display()))
                .output()
                .expect("run storage image in separate VM process"),
        );
    }
    assert!(
        durable
            .path()
            .join("checkpoints.sqlite")
            .metadata()
            .expect("real durable SQLite file")
            .len()
            > 0
    );
    let identity =
        terlan_storage::CheckpointStore::open(&durable.path().join("checkpoints.sqlite"))
            .unwrap()
            .observation()
            .unwrap()
            .identity;
    let pinned = pinned_binding("primary", identity, durable.path());
    succeeds(run_bound_entry(
        &vm,
        &image,
        &pinned,
        "restore_transactions",
    ));
    let mut wrong_identity = identity;
    wrong_identity[0] ^= 1;
    wrong_identity[1] |= 1;
    assert_ne!(wrong_identity, identity);
    let wrong_pin = pinned_binding("primary", wrong_identity, durable.path());
    succeeds(run_bound_entry(
        &vm,
        &image,
        &wrong_pin,
        "supervisor_identity_mismatch_is_rejected",
    ));
    let denied = run_bound_entry(
        &vm,
        &image,
        &wrong_pin,
        "supervisor_identity_mismatch_cannot_issue_proof",
    );
    assert!(
        !denied.status.success(),
        "mismatching identity cannot grant proof authority"
    );
    assert!(String::from_utf8_lossy(&denied.stderr).contains("vm.distributed_storage.unavailable"));
    let replacement = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let replacement_path = replacement.path().join("checkpoints.sqlite");
    let before = terlan_storage::CheckpointStore::open(&replacement_path)
        .unwrap()
        .observation()
        .unwrap();
    assert_ne!(before.identity, identity);
    succeeds(run_bound_entry(
        &vm,
        &image,
        &pinned_binding("primary", identity, replacement.path()),
        "supervisor_identity_mismatch_is_rejected",
    ));
    let after = terlan_storage::CheckpointStore::open(&replacement_path)
        .unwrap()
        .observation()
        .unwrap();
    assert_eq!(
        before, after,
        "denial leaves replacement checkpoint state unchanged"
    );
    // A new VM still accepts the original resource after both forms of rejection.
    succeeds(run_bound_entry(
        &vm,
        &image,
        &pinned,
        "restore_transactions",
    ));
    let secondary = pinned_binding("secondary", before.identity, replacement.path());
    let replaced_secondary = pinned_binding("secondary", identity, replacement.path());
    for (second, entry) in [
        (&secondary, "resource_validation_with_supervisor_authority"),
        (
            &replaced_secondary,
            "resource_registration_rejects_replaced_backend",
        ),
        (&secondary, "resource_validation_with_supervisor_authority"),
    ] {
        succeeds(
            Command::new(&vm)
                .arg("run")
                .arg(&image)
                .args([
                    "--entry",
                    entry,
                    "--test-eval",
                    "--storage",
                    &pinned,
                    "--storage",
                    second,
                ])
                .output()
                .expect("resource validation through independent authorized workers"),
        );
    }
    // All VM/worker processes have exited. Change only the tail of one stored
    // SHA-256 value, leaving the source-visible 32-bit prefix unchanged.
    // Rejecting this proves the worker validates the full digest, not its display.
    let database_path = durable.path().join("checkpoints.sqlite");
    let digest = {
        let mut store = terlan_storage::CheckpointStore::open(&database_path).unwrap();
        let checkpoint = store.load("sixth").unwrap().unwrap();
        store.flush().unwrap();
        checkpoint.digest()
    };
    let mut bytes = std::fs::read(&database_path).unwrap();
    let matches = bytes
        .windows(digest.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == digest).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "identify exactly one stored test digest");
    bytes[matches[0] + digest.len() - 1] ^= 1;
    std::fs::write(&database_path, bytes).unwrap();
    for (entry, success) in [
        ("corrupted_checkpoint_is_rejected", true),
        ("corrupted_checkpoint_cannot_issue_proof", false),
        ("missing_checkpoint_cannot_issue_proof", false),
    ] {
        let output = Command::new(&vm)
            .arg("run")
            .arg(&image)
            .args(["--entry", entry, "--test-eval", "--storage"])
            .arg(format!("primary={}", durable.path().display()))
            .output()
            .unwrap();
        if success {
            succeeds(output);
        } else {
            assert!(
                !output.status.success(),
                "invalid observation must not return a proof"
            );
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                diagnostic.contains("vm.distributed_storage.proof"),
                "{diagnostic}"
            );
        }
    }
}

fn pinned_binding(name: &str, identity: [u8; 32], directory: &std::path::Path) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(64);
    for byte in identity {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    format!("{name}@{encoded}={}", directory.display())
}

fn run_bound_entry(
    vm: &std::path::Path,
    image: &std::path::Path,
    binding: &str,
    entry: &str,
) -> Output {
    Command::new(vm)
        .arg("run")
        .arg(image)
        .args(["--entry", entry, "--test-eval", "--storage", binding])
        .output()
        .expect("run supervisor-pinned storage image in a fresh VM")
}
