//! Cache byte and discovery mutations must invalidate metadata observations.

use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(10))
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn capture(root: &Path) -> Snapshot {
    observe(root, control(), MAX_ENTRIES, MAX_BYTES).unwrap()
}

#[test]
fn resolver_cache_observes_indexes_manifests_and_discovery_but_not_rust_contents() {
    let fixture = temporary_fixture("resolver-cache-inputs");
    let root = &fixture.0;
    write(root, "registry/index/registry/.cache/pkg", "index");
    write(root, "registry/src/registry/pkg/Cargo.toml", "manifest");
    write(
        root,
        "registry/src/registry/pkg/src/lib.rs",
        "pub fn before() {}",
    );
    write(root, "git/db/repository/HEAD", "object");
    write(
        root,
        "git/checkouts/repository/revision/Cargo.toml",
        "git manifest",
    );
    let mut prior = capture(root);
    assert_eq!(prior, capture(root));
    write(
        root,
        "registry/src/registry/pkg/src/lib.rs",
        "pub fn after() {}",
    );
    assert_eq!(
        prior,
        capture(root),
        "Rust source bytes are not Cargo metadata inputs"
    );
    for (path, contents) in [
        ("registry/index/registry/.cache/pkg", "other index"),
        ("registry/src/registry/pkg/Cargo.toml", "other manifest"),
        ("registry/src/registry/pkg/src/bin/new.rs", "new target"),
        ("git/db/repository/HEAD", "other object"),
        (
            "git/checkouts/repository/revision/Cargo.toml",
            "other git manifest",
        ),
    ] {
        write(root, path, contents);
        let changed = capture(root);
        assert_ne!(prior, changed, "{path} was not bound");
        prior = changed;
    }
    fs::remove_file(root.join("registry/src/registry/pkg/src/bin/new.rs")).unwrap();
    assert_ne!(prior, capture(root));
}

#[test]
fn resolver_cache_binds_absence_and_deterministic_directory_order() {
    let fixture = temporary_fixture("resolver-cache-order");
    let root = &fixture.0;
    let absent = capture(&root.join("missing"));
    fs::create_dir(root.join("missing")).unwrap();
    assert_ne!(absent, capture(&root.join("missing")));
    write(root, "registry/src/registry/pkg/b.rs", "b");
    write(root, "registry/src/registry/pkg/a.rs", "a");
    let prior = capture(root);
    fs::remove_file(root.join("registry/src/registry/pkg/b.rs")).unwrap();
    write(root, "registry/src/registry/pkg/b.rs", "different body");
    assert_eq!(
        prior,
        capture(root),
        "enumeration order and Rust body bytes are irrelevant"
    );
}

#[test]
fn resolver_cache_bounds_contents_entries_and_cancellation() {
    let fixture = temporary_fixture("resolver-cache-budgets");
    let root = &fixture.0;
    write(root, "registry/index/config.json", "content");
    assert!(observe(root, control(), MAX_ENTRIES, 1).is_err());
    assert!(observe(root, control(), 1, MAX_BYTES).is_err());
    let cancelled = AtomicBool::new(true);
    assert!(observe(
        root,
        control().with_cancellation(&cancelled),
        MAX_ENTRIES,
        MAX_BYTES
    )
    .is_err());
    assert!(observe(
        &root.join("missing"),
        control().with_cancellation(&cancelled),
        MAX_ENTRIES,
        MAX_BYTES
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn resolver_cache_rejects_nested_symlinks_and_binds_home_retargeting() {
    use std::os::unix::fs::symlink;
    let fixture = temporary_fixture("resolver-cache-links");
    let root = &fixture.0;
    write(
        root,
        "first/registry/src/registry/pkg/Cargo.toml",
        "same manifest",
    );
    write(
        root,
        "second/registry/src/registry/pkg/Cargo.toml",
        "same manifest",
    );
    let selected = root.join("selected");
    symlink(root.join("first"), &selected).unwrap();
    let prior = capture(&selected);
    fs::remove_file(&selected).unwrap();
    symlink(root.join("second"), &selected).unwrap();
    assert_ne!(prior, capture(&selected));
    symlink(root.join("first"), root.join("second/registry/src/escape")).unwrap();
    assert!(observe(&selected, control(), MAX_ENTRIES, MAX_BYTES).is_err());
}
