use super::*;
use crate::support::test_fs::TestDirectory;

fn key() -> String {
    "a".repeat(64)
}
fn output() -> NormalizedExecution {
    NormalizedExecution {
        exit: 0,
        stdout: String::new(),
        stderr: String::new(),
    }
}
fn store(root: &Path) -> Checkpoints {
    Checkpoints::open(root, BTreeSet::from([key()])).unwrap()
}

#[test]
fn proof_checkpoint_reuses_two_independent_successes() {
    let root = TestDirectory::new("proof_checkpoint", "warm");
    let mut cache = store(&root);
    for replica in [1, 2] {
        cache.run(&key(), replica, || Ok(output())).unwrap();
    }
    assert_eq!(cache.completed, 2);
    drop(cache);
    let mut cache = store(&root);
    for replica in [1, 2] {
        cache
            .run(&key(), replica, || {
                panic!("verified replica must not execute")
            })
            .unwrap();
    }
    assert_eq!(cache.reused, 2);
    assert_eq!(cache.completed, 0);
}

#[test]
fn proof_checkpoint_retains_first_success_when_second_replica_fails() {
    let root = TestDirectory::new("proof_checkpoint", "failure");
    let mut cache = store(&root);
    cache.run(&key(), 1, || Ok(output())).unwrap();
    assert!(cache.run(&key(), 2, || Err("proof failed".into())).is_err());
    drop(cache);
    let mut cache = store(&root);
    cache
        .run(&key(), 1, || panic!("first replica replayed"))
        .unwrap();
    cache.run(&key(), 2, || Ok(output())).unwrap();
    assert_eq!((cache.reused, cache.completed), (1, 1));
}

#[test]
fn proof_checkpoint_recovers_complete_pending_publication() {
    let root = TestDirectory::new("proof_checkpoint", "pending");
    let mut cache = store(&root);
    cache.run(&key(), 1, || Ok(output())).unwrap();
    let stem = cache.path.join(format!("{}-1", key()));
    fs::rename(stem.with_extension("json"), stem.with_extension("pending")).unwrap();
    drop(cache);
    let mut cache = store(&root);
    cache
        .run(&key(), 1, || panic!("pending complete proof replayed"))
        .unwrap();
    assert_eq!(cache.reused, 1);
}

#[test]
fn proof_checkpoint_discards_partial_pending_but_rejects_corrupt_complete_receipt() {
    let root = TestDirectory::new("proof_checkpoint", "partial");
    let mut cache = store(&root);
    fs::write(cache.path.join(format!("{}-1.pending", key())), b"{").unwrap();
    cache.run(&key(), 1, || Ok(output())).unwrap();
    fs::write(cache.path.join(format!("{}-1.json", key())), b"corrupt").unwrap();
    assert!(cache
        .run(&key(), 1, || panic!("corruption must be attributed"))
        .is_err());
}

#[test]
fn proof_checkpoint_rejects_foreign_replica_and_unregistered_inputs() {
    let root = TestDirectory::new("proof_checkpoint", "identity");
    let mut cache = store(&root);
    cache.run(&key(), 1, || Ok(output())).unwrap();
    let bytes = fs::read(cache.path.join(format!("{}-1.json", key()))).unwrap();
    fs::write(cache.path.join(format!("{}-2.json", key())), bytes).unwrap();
    assert!(cache.run(&key(), 2, || panic!("foreign receipt")).is_err());
    assert!(cache
        .run(&"b".repeat(64), 1, || panic!("changed input"))
        .is_err());
    assert!(cache
        .run("../escape", 1, || panic!("escaping input"))
        .is_err());
}

#[test]
fn proof_checkpoint_preserves_pinned_old_receipts_and_retires_unreferenced_ones() {
    let root = TestDirectory::new("proof_checkpoint", "retention");
    let old = "b".repeat(64);
    let mut cache = Checkpoints::open(&root, BTreeSet::from([key(), old.clone()])).unwrap();
    cache.run(&key(), 1, || Ok(output())).unwrap();
    cache.run(&old, 1, || Ok(output())).unwrap();
    for name in [key(), old.clone()] {
        File::open(cache.path.join(format!("{name}-1.json")))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
    }
    let old_path = cache.path.join(format!("{old}-1.json"));
    drop(cache);
    let mut cache = store(&root);
    cache
        .run(&key(), 1, || panic!("pinned evidence expired"))
        .unwrap();
    assert!(!old_path.exists());
}

#[test]
fn proof_checkpoint_bounds_contended_lease_and_rejects_symlink_receipts() {
    let root = TestDirectory::new("proof_checkpoint", "lease");
    let mut cache = store(&root);
    assert!(Checkpoints::open_with_timeout(
        &root,
        BTreeSet::from([key()]),
        Duration::from_millis(30)
    )
    .is_err());
    let outside = TestDirectory::new("proof_checkpoint", "outside");
    fs::write(outside.join("receipt"), b"retained").unwrap();
    std::os::unix::fs::symlink(
        outside.join("receipt"),
        cache.path.join(format!("{}-1.json", key())),
    )
    .unwrap();
    assert!(cache.run(&key(), 1, || panic!("symlink receipt")).is_err());
    assert_eq!(fs::read(outside.join("receipt")).unwrap(), b"retained");
}

#[test]
fn proof_checkpoint_ownership_requires_canonical_names() {
    for replica in ["01", "+1", "002", "0", "3"] {
        assert!(!owned_name(&format!("{}-{replica}.json", key())));
    }
    assert!(owned_name(&format!("{}-1.json", key())));
    assert!(owned_name(&format!("{}-2.pending", key())));
}

#[test]
fn proof_checkpoint_survives_real_sigkill() {
    const CHILD: &str = "TERLAN_PROOF_CHECKPOINT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let mut cache = store(&root);
        cache.run(&key(), 1, || Ok(output())).unwrap();
        fs::write(root.join("ready"), b"first replica committed").unwrap();
        loop {
            std::thread::park();
        }
    }
    let root = TestDirectory::new("proof_checkpoint", "kill");
    let test_name = format!(
        "{}::proof_checkpoint_survives_real_sigkill",
        module_path!().split_once("::").unwrap().1
    );
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test_name, "--test-threads=1"])
        .env(CHILD, root.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !root.join("ready").exists() && start.elapsed() < Duration::from_secs(5) {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = root.join("ready").exists();
    child.kill().ok();
    let status = child.wait().unwrap();
    assert!(ready, "child did not commit its first replica: {status}");
    let mut cache = store(&root);
    cache
        .run(&key(), 1, || {
            panic!("completed work replayed after SIGKILL")
        })
        .unwrap();
    cache.run(&key(), 2, || Ok(output())).unwrap();
    assert_eq!((cache.reused, cache.completed), (1, 1));
}
