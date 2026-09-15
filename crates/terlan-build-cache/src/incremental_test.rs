use super::*;
use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

#[path = "working_session_test.rs"]
mod working_session_test;

#[path = "configuration_retention_test.rs"]
mod configuration_retention_test;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "terlan-build-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(path.join(".git"), "fixture").unwrap();
        fs::create_dir_all(path.join("target/debug/incremental/demo-123")).unwrap();
        Self(path)
    }

    fn profile(&self) -> PathBuf {
        self.0.join("target/debug")
    }

    fn session(&self, name: &str) -> PathBuf {
        let path = self.profile().join("incremental/demo-123").join(name);
        fs::create_dir(&path).unwrap();
        let parsed = layout::session_name(name).unwrap();
        File::create_new(path.parent().unwrap().join(parsed.lock)).unwrap();
        layout::write_header(&path);
        fs::write(path.join("abc.o"), "immutable object").unwrap();
        path
    }

    fn run(&self, prune: bool) -> io::Result<Report> {
        maintain(
            &self.0,
            prune,
            Policy::default(),
            UNIX_EPOCH + Duration::from_secs(3600),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn audit_is_read_only_and_prune_preserves_latest_and_outputs() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    let latest = fixture.session("s-2-1-bbb");
    let binary = fixture.profile().join("terlc");
    fs::write(&binary, "sealed compiler").unwrap();
    let report = fixture.run(false).unwrap();
    assert_eq!(report.redundant_sessions, 1);
    assert_eq!(report.sessions_before, 2);
    assert!(!fixture.profile().join(layout::OWNER_LOCK).exists());
    assert!(!fixture.profile().join(layout::RETIRED).exists());
    assert!(old.exists());
    let report = fixture.run(true).unwrap();
    assert_eq!(report.removed_sessions, 1);
    assert_eq!(report.sessions_after, 1);
    assert!(report.budget_verified);
    assert!(!old.exists());
    assert!(latest.exists());
    assert_eq!(fs::read_to_string(binary).unwrap(), "sealed compiler");
    assert_eq!(fixture.run(true).unwrap().removed_sessions, 0);
}

#[test]
fn rustc_shared_and_exclusive_leases_protect_old_generations() {
    for shared in [true, false] {
        let fixture = Fixture::new();
        let old = fixture.session("s-1-1-aaa");
        fixture.session("s-2-1-bbb");
        let lease = File::open(old.parent().unwrap().join("s-1-1.lock")).unwrap();
        if shared {
            lease.lock_shared().unwrap();
        } else {
            lease.lock().unwrap();
        }
        let report = fixture.run(true).unwrap();
        assert_eq!(report.removed_sessions, 0);
        assert_eq!(report.unmeasured_sessions, 1);
        assert!(!report.budget_verified);
        assert!(old.exists());
        // A concurrently spawned child can inherit the description until exec.
        // Explicit release, like the production Lease, makes the boundary exact.
        lease.unlock().unwrap();
        drop(lease);
        assert_eq!(fixture.run(true).unwrap().removed_sessions, 1);
    }
}

#[test]
fn missing_locks_are_not_adopted_but_abandoned_working_sessions_are_retired() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    let working = fixture.session("s-3-1-working");
    fixture.session("s-2-1-bbb");
    fs::remove_file(old.parent().unwrap().join("s-1-1.lock")).unwrap();
    let report = fixture.run(true).unwrap();
    assert_eq!(report.removed_sessions, 1);
    assert_eq!(report.abandoned_sessions, 1);
    assert_eq!(report.protected_sessions, 2);
    assert_eq!(report.unmeasured_sessions, 1);
    assert!(old.exists() && !working.exists());
}

#[test]
fn unknown_payload_blocks_all_retirement_before_first_delete() {
    let fixture = Fixture::new();
    let first = fixture.session("s-1-1-aaa");
    let invalid = fixture.session("s-2-1-bbb");
    fixture.session("s-3-1-ccc");
    fs::write(invalid.join("source.rs"), "user source").unwrap();
    assert!(fixture.run(true).is_err());
    assert!(first.exists() && invalid.exists());
    assert!(!fixture.profile().join(layout::RETIRED).exists());
}

#[test]
fn symlinks_nested_payloads_and_wrong_rust_versions_fail_closed() {
    for kind in ["symlink", "directory", "version"] {
        let fixture = Fixture::new();
        let old = fixture.session("s-1-1-aaa");
        fixture.session("s-2-1-bbb");
        match kind {
            "symlink" => {
                fs::remove_file(old.join("abc.o")).unwrap();
                symlink(fixture.0.join("Cargo.toml"), old.join("abc.o")).unwrap();
            }
            "directory" => fs::create_dir(old.join("nested.o")).unwrap(),
            "version" => fs::write(old.join("dep-graph.bin"), "foreign bytes").unwrap(),
            _ => unreachable!(),
        }
        assert!(fixture.run(true).is_err(), "{kind}");
        assert!(old.exists());
        assert_eq!(
            fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap(),
            "[workspace]\n"
        );
    }
}

#[test]
fn hardlinks_are_counted_once_and_external_links_survive_retirement() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    let latest = fixture.session("s-2-1-bbb");
    for name in ["abc.o", "dep-graph.bin"] {
        fs::remove_file(latest.join(name)).unwrap();
        fs::hard_link(old.join(name), latest.join(name)).unwrap();
    }
    let external = fixture.profile().join("sealed.o");
    fs::hard_link(old.join("abc.o"), &external).unwrap();
    let expected: u64 = ["abc.o", "dep-graph.bin"]
        .iter()
        .map(|name| fs::metadata(old.join(name)).unwrap().blocks() * 512)
        .sum();
    let report = fixture.run(true).unwrap();
    assert_eq!(report.allocated_bytes_before, expected);
    assert_eq!(report.allocated_bytes_after, expected);
    assert_eq!(fs::read_to_string(external).unwrap(), "immutable object");
}

#[test]
fn interrupted_retirement_recovers_after_removing_the_version_header() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    fixture.session("s-2-1-bbb");
    let retired = fixture.profile().join(layout::RETIRED);
    fs::create_dir(&retired).unwrap();
    let destination = retired.join("demo-123--s-1-1-aaa");
    fs::rename(old, &destination).unwrap();
    // A process crash between individual unlinks may remove the version header.
    fs::remove_file(destination.join("dep-graph.bin")).unwrap();
    let audit = fixture.run(false).unwrap();
    assert_eq!(audit.retired_sessions, 1);
    assert!(!audit.budget_verified);
    assert!(destination.exists());
    let report = fixture.run(true).unwrap();
    assert_eq!(report.recovered_sessions, 1);
    assert!(report.budget_verified);
    assert!(!destination.exists());
}

#[test]
fn unknown_retired_payload_preserves_current_and_retired_data() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    fixture.session("s-2-1-bbb");
    let retired = fixture
        .profile()
        .join(layout::RETIRED)
        .join("demo-123--s-0-1-aaa");
    fs::create_dir_all(&retired).unwrap();
    fs::write(retired.join("notes.txt"), "not disposable").unwrap();
    assert!(fixture.run(true).is_err());
    assert!(old.exists() && retired.exists());
}

#[test]
fn age_grace_and_future_timestamps_preserve_recent_generations() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    fixture.session("s-2-1-bbb");
    for now in [UNIX_EPOCH, UNIX_EPOCH + Duration::from_secs(299)] {
        let report = maintain(&fixture.0, true, Policy::default(), now).unwrap();
        assert_eq!(report.removed_sessions, 0);
        assert!(old.exists());
    }
}

#[test]
fn pinned_floor_exceeding_byte_or_entry_budget_is_not_reported_as_pass() {
    for policy in [
        Policy {
            bytes: 0,
            ..Policy::default()
        },
        Policy {
            sessions: 0,
            ..Policy::default()
        },
    ] {
        let fixture = Fixture::new();
        let latest = fixture.session("s-1-1-aaa");
        let report = maintain(
            &fixture.0,
            true,
            policy,
            UNIX_EPOCH + Duration::from_secs(3600),
        )
        .unwrap();
        assert!(!report.budget_verified);
        assert_eq!(report.removed_sessions, 0);
        assert!(latest.exists());
    }
}

#[test]
fn concurrent_maintenance_is_rejected_without_deleting_a_session() {
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    fixture.session("s-2-1-bbb");
    let lease = File::create_new(fixture.profile().join(layout::OWNER_LOCK)).unwrap();
    lease.lock().unwrap();
    assert!(fixture.run(true).is_err());
    assert!(fixture.run(false).is_err());
    assert!(old.exists());
}

#[test]
fn symlinked_profile_or_owner_lock_is_rejected() {
    for component in ["profile", "lock"] {
        let fixture = Fixture::new();
        if component == "profile" {
            let saved = fixture.0.join("saved-profile");
            fs::rename(fixture.profile(), &saved).unwrap();
            symlink(saved, fixture.profile()).unwrap();
        } else {
            symlink(
                fixture.0.join("Cargo.toml"),
                fixture.profile().join(layout::OWNER_LOCK),
            )
            .unwrap();
        }
        assert!(fixture.run(true).is_err());
    }
}

#[test]
fn timestamps_are_base36_microseconds_and_names_cannot_escape() {
    assert_eq!(
        layout::session_name("s-10-a-b").unwrap().created,
        UNIX_EPOCH + Duration::from_micros(36)
    );
    for name in [
        "../s-1-1-aaa",
        "s-1-1",
        "s-X-1-aaa",
        "s-1-1-a-b",
        "s--1-aaa",
    ] {
        assert!(layout::session_name(name).is_err(), "{name}");
    }
    for name in ["../demo-123", "demo--123", "demo-", "demo-ABC"] {
        assert!(!layout::crate_name(name), "{name}");
    }
}

#[test]
fn dangling_retirement_symlinks_are_not_mistaken_for_absence() {
    let fixture = Fixture::new();
    fixture.session("s-1-1-aaa");
    symlink(
        fixture.0.join("missing"),
        fixture.profile().join(layout::RETIRED),
    )
    .unwrap();
    assert!(fixture.run(false).is_err());
    assert!(fixture.run(true).is_err());
}

fn rustc_build(fixture: &Fixture) {
    use std::process::{Command, Stdio};
    use std::time::Instant;
    let mut child = Command::new("rustc")
        .args(["--crate-name", "cache_probe", "--crate-type", "rlib", "-C"])
        .arg(format!(
            "incremental={}",
            fixture.profile().join("incremental").display()
        ))
        .args(["-C", "debuginfo=line-tables-only", "-o"])
        .arg(fixture.profile().join("probe.rlib"))
        .arg(fixture.0.join("probe.rs"))
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "rustc failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("rustc cache fixture exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_rustc_sessions_preserve_artifact_and_reuse_current_objects_after_prune() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("probe.rs"),
        "pub fn answer() -> u64 { 42 }\n",
    )
    .unwrap();
    rustc_build(&fixture);
    rustc_build(&fixture);
    let artifact = fs::read(fixture.profile().join("probe.rlib")).unwrap();
    let policy = Policy {
        grace: Duration::ZERO,
        ..Policy::default()
    };
    let report = maintain(&fixture.0, true, policy, SystemTime::now()).unwrap();
    assert!(report.removed_sessions >= 1);
    assert_eq!(report.sessions_after, 1);
    assert!(report.budget_verified);
    assert!(fs::read(fixture.profile().join("probe.rlib")).unwrap() == artifact);
    let retained = current_objects(&fixture);
    assert!(!retained.is_empty());
    rustc_build(&fixture);
    // rustc's rlib archive member names vary even on untouched warm rebuilds.
    // Test the actual cache contract: all current codegen objects are reused
    // by inode, rather than assuming a byte-identical regenerated container.
    assert_eq!(current_objects(&fixture), retained);
}

fn current_objects(fixture: &Fixture) -> Vec<(String, u64, u64)> {
    let crates = layout::children(&fixture.profile().join("incremental")).unwrap();
    let krate = crates
        .iter()
        .find(|path| layout::name(path).unwrap().starts_with("cache_probe-"))
        .unwrap();
    let sessions = layout::children(krate).unwrap();
    let current = sessions
        .iter()
        .filter_map(|path| {
            layout::session_name(layout::name(path).unwrap())
                .ok()
                .map(|name| (name.created, path))
        })
        .max_by_key(|(created, _)| *created)
        .unwrap()
        .1;
    layout::payload(current, true)
        .unwrap()
        .into_iter()
        .filter_map(|(path, metadata)| {
            let name = layout::name(&path).unwrap();
            name.ends_with(".o")
                .then(|| (name.to_owned(), metadata.dev(), metadata.ino()))
        })
        .collect()
}

#[test]
fn killed_retirement_owner_releases_leases_and_recovers_partial_unlinks() {
    use std::process::{Command, Stdio};
    use std::time::Instant;
    const CHILD_ROOT: &str = "TERLAN_BUILD_CACHE_KILL_FIXTURE";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let fixture = Fixture(PathBuf::from(root));
        let profile = fixture.profile();
        let owner = File::create_new(profile.join(layout::OWNER_LOCK)).unwrap();
        owner.lock().unwrap();
        let inventory = inspect(
            &profile,
            Policy::default(),
            UNIX_EPOCH + Duration::from_secs(3600),
        )
        .unwrap();
        assert_eq!(inventory.candidates.len(), 1);
        fs::create_dir(profile.join(layout::RETIRED)).unwrap();
        let candidate = &inventory.candidates[0];
        begin_retirement(candidate).unwrap();
        fs::remove_file(candidate.destination.join("dep-graph.bin")).unwrap();
        fs::write(fixture.0.join("retired.ready"), "ready").unwrap();
        loop {
            std::thread::park();
        }
    }
    let fixture = Fixture::new();
    let old = fixture.session("s-1-1-aaa");
    let latest = fixture.session("s-2-1-bbb");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "incremental::incremental_test::killed_retirement_owner_releases_leases_and_recovers_partial_unlinks", "--nocapture"])
        .env(CHILD_ROOT, &fixture.0).stdin(Stdio::null()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fixture.0.join("retired.ready").exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = fixture.0.join("retired.ready").exists();
    let blocked = ready && fixture.run(true).is_err();
    // Always terminate/reap the owned subprocess before an assertion can unwind.
    let _ = child.kill();
    child.wait().unwrap();
    assert!(ready && blocked);
    assert!(!old.exists());
    let report = fixture.run(true).unwrap();
    assert_eq!(report.recovered_sessions, 1);
    assert_eq!(report.sessions_after, 1);
    assert!(report.budget_verified && latest.exists());
}
