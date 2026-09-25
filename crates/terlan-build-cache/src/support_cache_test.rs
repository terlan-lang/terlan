use super::*;
use std::fs::{File, FileTimes};
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-support-retention-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::create_dir_all(root.join("target/quality")).unwrap();
    File::create_new(root.join("target/quality/bootstrap-owner.lock")).unwrap();
    let fixture = Fixture(root);
    generation(&fixture, 'a');
    fs::write(cache(&fixture).join("active"), format!("{}\n", digest('a'))).unwrap();
    fixture
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn cache(fixture: &Fixture) -> PathBuf {
    fixture.0.join("target/hermetic-support")
}

fn generation(fixture: &Fixture, character: char) -> PathBuf {
    let root = cache(fixture).join("generations").join(digest(character));
    for relative in ["registry", "git", "target/debug", "target/quality"] {
        fs::create_dir_all(root.join(relative)).unwrap();
    }
    for relative in [
        "last-used",
        "target/debug/terlan-build-cache",
        "target/debug/terlan-test-orchestrator",
        "target/quality/hermetic-support.json",
    ] {
        fs::write(root.join(relative), "preserved producer bytes").unwrap();
    }
    root
}

fn age(path: &Path, timestamp: SystemTime) {
    let metadata = fs::symlink_metadata(path).unwrap();
    if metadata.is_symlink() {
        return;
    }
    if metadata.is_dir() {
        for child in fs::read_dir(path).unwrap() {
            age(&child.unwrap().path(), timestamp);
        }
    }
    File::open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(timestamp))
        .unwrap();
}

fn policy() -> Policy {
    Policy {
        lock_wait: Duration::ZERO,
        ..Policy::default()
    }
}

#[test]
fn support_retention_preserves_current_bytes_and_retires_expired_generations() {
    let fixture = fixture();
    let old = generation(&fixture, 'b');
    let now = SystemTime::now();
    age(&old, now - Duration::from_secs(8 * 86400));
    let current = cache(&fixture).join("generations").join(digest('a'));
    age(&current, now - Duration::from_secs(9 * 86400));
    let executable = current.join("target/debug/terlan-build-cache");
    let inode = fs::metadata(&executable).unwrap().ino();
    let report = maintain(&fixture.0, policy(), now).unwrap();
    assert!(report.budget_verified);
    assert_eq!(report.retired, [digest('b')]);
    assert_eq!(report.generations_after, 1);
    assert!(!old.exists());
    assert_eq!(fs::metadata(&executable).unwrap().ino(), inode);
    assert_eq!(
        fs::read_to_string(&executable).unwrap(),
        "preserved producer bytes"
    );
    let warm = maintain(&fixture.0, policy(), now).unwrap();
    assert!(warm.retired.is_empty());
    assert_eq!(warm.allocated_bytes_before, warm.allocated_bytes_after);
}

#[test]
fn support_retention_byte_entry_and_generation_limits_evict_oldest_eligible_only() {
    for kind in ["bytes", "entries", "generations"] {
        let fixture = fixture();
        let old = generation(&fixture, 'b');
        let now = SystemTime::now();
        age(&old, now - Duration::from_secs(600));
        let current_only = inventory::inspect(&cache(&fixture))
            .unwrap()
            .generations
            .into_iter()
            .filter(|entry| entry.key == digest('a'))
            .collect::<Vec<_>>();
        let (bytes, entries) = totals(&current_only, &BTreeSet::new()).unwrap();
        let mut limits = policy();
        match kind {
            "bytes" => limits.bytes = bytes,
            "entries" => limits.entries = entries,
            _ => limits.generations = 1,
        }
        let report = maintain(&fixture.0, limits, now).unwrap();
        assert!(report.budget_verified, "{kind}");
        assert_eq!(report.retired, [digest('b')], "{kind}");
    }
}

#[test]
fn support_retention_does_not_destroy_current_recent_or_future_data_to_satisfy_limits() {
    let fixture = fixture();
    let future = generation(&fixture, 'b');
    generation(&fixture, 'c');
    let now = SystemTime::now();
    age(&future, now + Duration::from_secs(3600));
    let limits = Policy {
        bytes: 1,
        entries: 1,
        generations: 1,
        ..policy()
    };
    let report = maintain(&fixture.0, limits, now).unwrap();
    assert!(!report.budget_verified);
    assert!(report.retired.is_empty());
    assert_eq!(report.generations_after, 3);
}

#[test]
fn support_retention_recovers_interrupted_retirement_and_legacy_layout() {
    let fixture = fixture();
    let old = generation(&fixture, 'b');
    let retired = cache(&fixture).join("retired");
    fs::create_dir(&retired).unwrap();
    fs::rename(old, retired.join(digest('b'))).unwrap();
    fs::remove_file(retired.join(digest('b')).join("last-used")).unwrap();
    let legacy = cache(&fixture).join("target");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("old-object"), "regenerable").unwrap();
    let now = SystemTime::now();
    age(&legacy, now - Duration::from_secs(600));
    let report = maintain(&fixture.0, policy(), now).unwrap();
    assert!(report.budget_verified);
    assert_eq!(report.recovered, [digest('b')]);
    assert_eq!(report.retired, ["legacy-target"]);
    assert!(!legacy.exists());
    assert_eq!(fs::read_dir(retired).unwrap().count(), 0);
}

#[test]
fn support_retention_refuses_occupied_missing_or_redirected_bootstrap_lease() {
    for kind in ["occupied", "missing", "redirected"] {
        let fixture = fixture();
        let old = generation(&fixture, 'b');
        let now = SystemTime::now();
        age(&old, now - Duration::from_secs(8 * 86400));
        let path = fixture.0.join("target/quality/bootstrap-owner.lock");
        let lock = File::open(&path).unwrap();
        if kind == "occupied" {
            lock.lock().unwrap();
        } else {
            fs::remove_file(&path).unwrap();
            if kind == "redirected" {
                let outside = fixture.0.join("protected-lock");
                File::create_new(&outside).unwrap();
                symlink(outside, &path).unwrap();
            }
        }
        assert!(maintain(&fixture.0, policy(), now).is_err(), "{kind}");
        assert!(old.exists(), "{kind}");
    }
}

#[test]
fn support_retention_validates_all_entries_before_deleting_anything() {
    for kind in [
        "unknown",
        "redirected-generation",
        "redirected-retirement",
        "marker",
        "output-parent",
    ] {
        let fixture = fixture();
        let old = generation(&fixture, 'b');
        let now = SystemTime::now();
        age(&old, now - Duration::from_secs(8 * 86400));
        let root = cache(&fixture);
        match kind {
            "unknown" => {
                fs::write(root.join("keep"), "not owned").unwrap();
            }
            "redirected-generation" => {
                symlink(&fixture.0, root.join("generations").join(digest('c'))).unwrap();
            }
            "redirected-retirement" => {
                fs::create_dir(root.join("retired")).unwrap();
                symlink(&fixture.0, root.join("retired").join(digest('c'))).unwrap();
            }
            "output-parent" => {
                let path = root
                    .join("generations")
                    .join(digest('a'))
                    .join("target/debug");
                fs::rename(&path, fixture.0.join("protected-debug")).unwrap();
                symlink(fixture.0.join("protected-debug"), path).unwrap();
            }
            _ => {
                fs::write(root.join("active"), "../not-owned\n").unwrap();
            }
        }
        assert!(maintain(&fixture.0, policy(), now).is_err(), "{kind}");
        assert!(old.exists(), "{kind}");
    }
}

#[test]
fn support_retention_unlinks_interior_symlinks_without_following_targets() {
    let fixture = fixture();
    let old = generation(&fixture, 'b');
    let outside = fixture.0.join("preserved");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "not cache").unwrap();
    symlink(&outside, old.join("registry/external")).unwrap();
    // Use a future observation so the symlink's own timestamp is outside grace.
    let now = SystemTime::now() + Duration::from_secs(8 * 86400);
    let report = maintain(&fixture.0, policy(), now).unwrap();
    assert_eq!(report.retired, [digest('b')]);
    assert_eq!(
        fs::read_to_string(outside.join("keep")).unwrap(),
        "not cache"
    );
}

#[test]
fn support_retention_counts_shared_inodes_once_and_preserves_remaining_links() {
    let fixture = fixture();
    let old = generation(&fixture, 'b');
    let current = cache(&fixture).join("generations").join(digest('a'));
    let retained = current.join("registry/shared");
    fs::write(&retained, vec![1_u8; 32768]).unwrap();
    fs::hard_link(&retained, old.join("registry/shared")).unwrap();
    let now = SystemTime::now();
    age(&old, now - Duration::from_secs(8 * 86400));
    let snapshot = inventory::inspect(&cache(&fixture)).unwrap();
    let naive: u64 = snapshot
        .generations
        .iter()
        .flat_map(|entry| entry.inodes.values())
        .sum();
    let (actual, _) = totals(&snapshot.generations, &BTreeSet::new()).unwrap();
    assert!(actual < naive);
    let report = maintain(&fixture.0, policy(), now).unwrap();
    assert_eq!(report.retired, [digest('b')]);
    assert_eq!(fs::read(retained).unwrap(), vec![1_u8; 32768]);
}

#[test]
fn support_retention_recovers_a_retired_key_rebuilt_as_the_current_generation() {
    let fixture = fixture();
    let root = cache(&fixture);
    let current = root.join("generations").join(digest('a'));
    let retired = root.join("retired").join(digest('a'));
    fs::create_dir(root.join("retired")).unwrap();
    fs::rename(&current, &retired).unwrap();
    // A crashed retirement followed by a return to this toolchain/lock can
    // recreate the live key before maintenance resumes. Paths, not equal
    // digest strings, distinguish the new live bytes from disposable residue.
    generation(&fixture, 'a');
    let output = current.join("target/debug/terlan-build-cache");
    fs::write(&output, "newly verified output").unwrap();
    let report = maintain(&fixture.0, policy(), SystemTime::now()).unwrap();
    assert!(report.budget_verified);
    assert_eq!(report.recovered, [digest('a')]);
    assert!(report.retired.is_empty());
    assert!(!retired.exists());
    assert_eq!(fs::read_to_string(output).unwrap(), "newly verified output");
}
