//! Real filesystem coverage for bounded object storage and linking leases.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::artifact_cache_retention::{Budget, CacheFamily, RetainedCache};
use crate::support::test_fs::TestDirectory;

fn key(value: u8) -> String {
    format!("{value:064x}")
}

fn budget(bytes: u64, entries: usize) -> Budget {
    Budget {
        bytes,
        entries,
        age: Duration::from_secs(86_400),
    }
}

fn open(root: &Path, keys: &[u8], policy: Budget) -> RetainedCache {
    RetainedCache::open(
        &root.join("units-v2"),
        CacheFamily::NativeObjects,
        keys.iter().copied().map(key).collect(),
        policy,
    )
    .expect("open object cache")
}

fn put(cache: &RetainedCache, value: u8, bytes: usize) {
    cache
        .publish(&key(value), "module.o", &vec![value; bytes], b"manifest")
        .expect("publish object")
}

#[test]
fn byte_pressure_retires_only_unneeded_generations() {
    let root = TestDirectory::new("artifact_cache_retention", "bytes");
    let cache = open(&root, &[1], budget(128, 4));
    put(&cache, 1, 80);
    let old = cache.path(&key(1)).expect("old object path");
    drop(cache);
    let cache = open(&root, &[2], budget(128, 4));
    put(&cache, 2, 80);
    assert!(!old.exists());
    assert_eq!(
        fs::read(cache.path(&key(2)).unwrap().join("module.o")).unwrap(),
        vec![2; 80]
    );
    assert_eq!(
        fs::read_dir(root.join("units-v2/retired")).unwrap().count(),
        0
    );
    drop(cache);
    root.close();
}

#[test]
fn entry_budget_reserves_the_entire_next_link_set_before_writes() {
    let root = TestDirectory::new("artifact_cache_retention", "entry_count");
    let cache = open(&root, &[1, 2], budget(1024, 2));
    put(&cache, 1, 8);
    put(&cache, 2, 8);
    let old = cache.path(&key(1)).unwrap();
    drop(cache);
    let cache = open(&root, &[2, 3], budget(1024, 2));
    assert!(
        !old.exists(),
        "the third slot is reserved before compilation"
    );
    put(&cache, 3, 8);
    assert_eq!(
        fs::read_dir(root.join("units-v2/entries")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read(cache.path(&key(2)).unwrap().join("module.o")).unwrap(),
        vec![2; 8]
    );
    drop(cache);
    root.close();
}

#[test]
fn pinned_inputs_and_replacement_staging_cannot_exceed_the_byte_budget() {
    let root = TestDirectory::new("artifact_cache_retention", "pinned");
    let cache = open(&root, &[1, 2], budget(100, 2));
    put(&cache, 1, 80);
    let path = cache.path(&key(1)).unwrap().join("module.o");
    assert!(cache
        .publish(&key(2), "module.o", &[2; 80], b"manifest")
        .is_err());
    assert!(cache
        .publish(&key(1), "module.o", &[3; 32], b"manifest")
        .is_err());
    assert_eq!(fs::read(path).unwrap(), vec![1; 80]);
    assert!(!cache.path(&key(2)).unwrap().exists());
    drop(cache);
    root.close();
}

#[test]
fn expired_entries_retire_but_current_link_inputs_remain_pinned() {
    let root = TestDirectory::new("artifact_cache_retention", "age");
    let cache = open(&root, &[1, 2], budget(1024, 4));
    put(&cache, 1, 8);
    put(&cache, 2, 8);
    let old = cache.path(&key(1)).unwrap();
    drop(cache);
    let cache = RetainedCache::open_at(
        &root.join("units-v2"),
        CacheFamily::NativeObjects,
        BTreeSet::from([key(2)]),
        budget(1024, 4),
        SystemTime::now() + Duration::from_secs(172_800),
    )
    .unwrap();
    assert!(!old.exists());
    assert_eq!(
        fs::read(cache.path(&key(2)).unwrap().join("module.o")).unwrap(),
        vec![2; 8]
    );
    drop(cache);
    root.close();
}

#[test]
fn interrupted_private_writes_and_retirements_recover_before_reuse() {
    let root = TestDirectory::new("artifact_cache_retention", "interrupted");
    let cache = open(&root, &[1, 2], budget(1024, 4));
    put(&cache, 1, 8);
    put(&cache, 2, 8);
    let kept = cache.path(&key(1)).unwrap();
    fs::write(kept.join(".module.o.999.1.tmp"), [9; 16]).unwrap();
    fs::rename(
        cache.path(&key(2)).unwrap(),
        root.join("units-v2/retired").join(key(2)),
    )
    .unwrap();
    drop(cache);
    let cache = open(&root, &[1], budget(1024, 4));
    assert_eq!(fs::read(kept.join("module.o")).unwrap(), vec![1; 8]);
    assert!(!kept.join(".module.o.999.1.tmp").exists());
    assert_eq!(
        fs::read_dir(root.join("units-v2/retired")).unwrap().count(),
        0
    );
    drop(cache);
    root.close();
}

#[test]
fn unowned_files_are_not_deleted_to_make_room() {
    let root = TestDirectory::new("artifact_cache_retention", "unowned");
    let cache = open(&root, &[1], budget(1024, 4));
    put(&cache, 1, 8);
    let unrelated = cache.path(&key(1)).unwrap().join("source.rs");
    fs::write(&unrelated, b"user source").unwrap();
    drop(cache);
    assert!(RetainedCache::open(
        &root.join("units-v2"),
        CacheFamily::NativeObjects,
        BTreeSet::new(),
        budget(1, 1)
    )
    .is_err());
    assert_eq!(fs::read(unrelated).unwrap(), b"user source");
    root.close();
}

#[cfg(unix)]
#[test]
fn object_cache_never_follows_generation_symlinks() {
    let root = TestDirectory::new("artifact_cache_retention", "symlink");
    let outside = TestDirectory::new("artifact_cache_retention", "outside");
    let cache = open(&root, &[], budget(1024, 4));
    drop(cache);
    fs::write(outside.join("source.rs"), b"outside data").unwrap();
    std::os::unix::fs::symlink(&*outside, root.join("units-v2/entries").join(key(1))).unwrap();
    assert!(RetainedCache::open(
        &root.join("units-v2"),
        CacheFamily::NativeObjects,
        BTreeSet::new(),
        budget(1, 1)
    )
    .is_err());
    assert_eq!(
        fs::read(outside.join("source.rs")).unwrap(),
        b"outside data"
    );
    root.close();
    outside.close();
}

#[test]
fn link_lease_prevents_retirement_until_the_consumer_releases_paths() {
    let root = TestDirectory::new("artifact_cache_retention", "link_lease");
    let cache = open(&root, &[1], budget(128, 1));
    put(&cache, 1, 80);
    let input = cache.path(&key(1)).unwrap().join("module.o");
    let next_root = root.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    let next = std::thread::spawn(move || {
        let next = open(&next_root, &[2], budget(128, 1));
        put(&next, 2, 80);
        sender.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
    assert_eq!(fs::read(&input).unwrap(), vec![1; 80]);
    drop(cache);
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("lease released");
    next.join().unwrap();
    assert!(!input.exists());
    root.close();
}

#[test]
fn checked_retention_recovers_only_its_own_payload_scratch() {
    let root = TestDirectory::new("artifact_cache_retention", "checked_scratch");
    let cache = RetainedCache::open(
        &root,
        CacheFamily::CheckedImplementations,
        BTreeSet::from([key(1)]),
        budget(128, 4),
    )
    .unwrap();
    cache
        .publish(&key(1), "checked.json", b"verified payload", b"manifest")
        .unwrap();
    assert!(cache
        .publish(&key(1), "module.o", b"wrong family", b"manifest")
        .is_err());
    let path = cache.path(&key(1)).unwrap();
    fs::write(path.join(".checked.json.999.1.tmp"), b"interrupted payload").unwrap();
    drop(cache);
    let cache = RetainedCache::open(
        &root,
        CacheFamily::CheckedImplementations,
        BTreeSet::from([key(1)]),
        budget(128, 4),
    )
    .unwrap();
    assert_eq!(
        fs::read(path.join("checked.json")).unwrap(),
        b"verified payload"
    );
    assert!(!path.join(".checked.json.999.1.tmp").exists());
    drop(cache);
    root.close();
}
