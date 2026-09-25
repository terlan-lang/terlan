use super::*;

fn snapshot(entries: &[(&str, &str)]) -> Snapshot {
    Snapshot::from_entries(
        entries
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
    )
    .unwrap()
}

#[test]
fn build_inputs_are_order_independent_and_sensitive_to_absence_and_values() {
    let first = snapshot(&[("PATH", "/bin"), ("RUSTFLAGS", "-Cdebuginfo=1")]);
    let reordered = snapshot(&[("RUSTFLAGS", "-Cdebuginfo=1"), ("PATH", "/bin")]);
    assert_eq!(first.digest(), reordered.digest());
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_BUILD_TARGET",
        "RUSTC_WRAPPER",
        "CUSTOM_BUILD_INPUT",
    ] {
        let absent = snapshot(&[]);
        let empty = snapshot(&[(key, "")]);
        let value = snapshot(&[(key, "one")]);
        let changed = snapshot(&[(key, "two")]);
        assert_ne!(absent.digest(), empty.digest(), "{key}");
        assert_ne!(empty.digest(), value.digest(), "{key}");
        assert_ne!(value.digest(), changed.digest(), "{key}");
    }
}

#[test]
fn make_scheduling_does_not_invalidate_matching_builds() {
    let first = snapshot(&[("RUSTFLAGS", "checked")]);
    let nested = snapshot(&[
        ("RUSTFLAGS", "checked"),
        ("MAKELEVEL", "3"),
        ("MAKEFLAGS", "-j4"),
        ("CARGO_BUILD_JOBS", "1"),
        ("SHLVL", "2"),
    ]);
    assert_eq!(first.digest(), nested.digest());
}

#[test]
fn frozen_child_configuration_is_exact_without_plaintext_report_values() {
    let snapshot = snapshot(&[
        ("PRIVATE_FIXTURE_VALUE", "not-for-reports"),
        ("MAKELEVEL", "2"),
    ]);
    let mut command = Command::new("not-launched");
    command.env("UNDECLARED", "not-in-snapshot");
    snapshot.configure(&mut command);
    let observed: BTreeMap<_, _> = command
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.unwrap().to_owned()))
        .collect();
    assert_eq!(observed, snapshot.entries);
    assert_eq!(snapshot.digest().len(), 64);
    assert!(!snapshot.digest().contains("not-for-reports"));
}

#[test]
fn invalid_or_oversized_environment_is_rejected_without_echoing_values() {
    for entries in [
        vec![("INVALID=KEY".into(), "private".into())],
        vec![("KEY".into(), "private\0value".into())],
        vec![("KEY".into(), "x".repeat(1024 * 1024).into())],
        vec![("KEY".into(), "one".into()), ("KEY".into(), "two".into())],
    ] {
        let error = Snapshot::from_entries(entries).err().unwrap().to_string();
        assert!(!error.contains("private"));
        assert!(error.starts_with("invalid owner") || error.starts_with("owner environment"));
    }
}
