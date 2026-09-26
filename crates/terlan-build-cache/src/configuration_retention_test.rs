use super::*;

const NOW: u64 = 10 * 24 * 3600;

fn base36(mut value: u64) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit((value % 36) as u32, 36).unwrap());
        value /= 36;
        if value == 0 {
            return digits.into_iter().rev().collect();
        }
    }
}

fn configuration(fixture: &Fixture, krate: &str, created: u64, working: bool) -> PathBuf {
    let directory = fixture.profile().join("incremental").join(krate);
    fs::create_dir_all(&directory).unwrap();
    let hash = if working { "working" } else { "aaa" };
    let name = format!("s-{}-1-{hash}", base36(created * 1_000_000));
    let path = directory.join(&name);
    fs::create_dir(&path).unwrap();
    File::create_new(directory.join(layout::session_name(&name).unwrap().lock)).unwrap();
    layout::write_header(&path);
    fs::write(path.join("abc.o"), b"regenerable codegen").unwrap();
    path
}

fn maintain_at(fixture: &Fixture, prune: bool) -> Report {
    maintain(
        &fixture.0,
        prune,
        Policy::default(),
        UNIX_EPOCH + Duration::from_secs(NOW),
    )
    .unwrap()
}

#[test]
fn expired_configuration_is_audited_then_retired_without_touching_compiled_outputs() {
    let fixture = Fixture::new();
    let old = configuration(&fixture, "demo-123", 1, false);
    let current = configuration(&fixture, "demo-456", NOW - 600, false);
    let dependency = configuration(&fixture, "dependency-123", 1, false);
    fs::create_dir(fixture.profile().join("deps")).unwrap();
    let rlib = fixture.profile().join("deps/libdependency.rlib");
    fs::write(&rlib, b"retained dependency").unwrap();
    let report = maintain_at(&fixture, false);
    assert_eq!(report.superseded_configurations, 1);
    assert_eq!(report.redundant_sessions, 0);
    assert_eq!(report.candidate_paths.len(), 1);
    assert!(old.exists());
    assert!(!fixture.profile().join(layout::OWNER_LOCK).exists());
    let report = maintain_at(&fixture, true);
    assert_eq!(report.removed_sessions, 1);
    assert_eq!(report.superseded_configurations, 1);
    assert!(!old.exists());
    assert!(current.exists() && dependency.exists());
    assert_eq!(fs::read(rlib).unwrap(), b"retained dependency");
    assert_eq!(maintain_at(&fixture, true).removed_sessions, 0);
}

#[test]
fn recent_and_equal_generation_configurations_are_preserved() {
    for created in [NOW - 72 * 3600 + 1, NOW - 600] {
        let fixture = Fixture::new();
        let old = configuration(&fixture, "demo-123", created, false);
        let current = configuration(&fixture, "demo-456", NOW - 600, false);
        assert_eq!(maintain_at(&fixture, true).removed_sessions, 0);
        assert!(old.exists() && current.exists());
    }
}

#[test]
fn age_boundary_retires_superseded_but_not_only_configuration() {
    let fixture = Fixture::new();
    let old = configuration(&fixture, "demo-123", NOW - 72 * 3600, false);
    let current = configuration(&fixture, "demo-456", NOW - 600, false);
    let only = configuration(&fixture, "single-123", 1, false);
    assert_eq!(maintain_at(&fixture, true).removed_sessions, 1);
    assert!(!old.exists());
    assert!(current.exists() && only.exists());
}

#[test]
fn working_or_future_configuration_does_not_supersede_completed_cache() {
    for (created, working) in [(NOW - 1, true), (NOW + 1, false)] {
        let fixture = Fixture::new();
        let old = configuration(&fixture, "demo-123", 1, false);
        configuration(&fixture, "demo-456", created, working);
        assert_eq!(maintain_at(&fixture, true).removed_sessions, 0);
        assert!(old.exists());
    }
}

#[test]
fn shared_exclusive_and_missing_rustc_leases_protect_expired_configurations() {
    for kind in ["shared", "exclusive", "missing"] {
        let fixture = Fixture::new();
        let old = configuration(&fixture, "demo-123", 1, false);
        configuration(&fixture, "demo-456", NOW - 600, false);
        let lock = old.parent().unwrap().join(
            layout::session_name(layout::name(&old).unwrap())
                .unwrap()
                .lock,
        );
        let lease = File::open(&lock).unwrap();
        match kind {
            "shared" => lease.lock_shared().unwrap(),
            "exclusive" => lease.lock().unwrap(),
            _ => fs::remove_file(&lock).unwrap(),
        }
        let report = maintain_at(&fixture, true);
        assert_eq!(report.removed_sessions, 0);
        assert_eq!(report.unmeasured_sessions, 1);
        assert!(!report.budget_verified);
        assert!(old.exists());
    }
}

#[test]
fn interrupted_configuration_retirement_is_recovered() {
    let fixture = Fixture::new();
    let old = configuration(&fixture, "demo-123", 1, false);
    let current = configuration(&fixture, "demo-456", NOW - 600, false);
    let retired = fixture.profile().join(layout::RETIRED);
    fs::create_dir(&retired).unwrap();
    let inventory = inspect(
        &fixture.profile(),
        Policy::default(),
        UNIX_EPOCH + Duration::from_secs(NOW),
    )
    .unwrap();
    assert_eq!(inventory.candidates.len(), 1);
    assert!(inventory.candidates[0].superseded);
    begin_retirement(&inventory.candidates[0]).unwrap();
    drop(inventory);
    assert!(!old.exists());
    let report = maintain_at(&fixture, true);
    assert_eq!(report.recovered_sessions, 1);
    assert!(current.exists());
    assert_eq!(fs::read_dir(retired).unwrap().count(), 0);
}
