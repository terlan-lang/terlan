use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::ffi::OsString;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn environment(root: &Path, entries: &[(&str, OsString)]) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        entries
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone())),
        root,
    )
    .unwrap()
}

fn binding(path: &Path) -> ToolConfiguration {
    ToolConfiguration {
        before: observe(&BTreeSet::from([path.to_owned()]), control(), MAX_BYTES).unwrap(),
        after: None,
        cargo_order: Vec::new(),
        settings: Default::default(),
    }
}

#[test]
fn configuration_paths_cover_ancestors_and_frozen_tool_homes() {
    let fixture = temporary_fixture("tool-configuration-paths");
    let root = fixture.0.join("project/nested");
    fs::create_dir_all(&root).unwrap();
    let environment = environment(
        &root,
        &[
            ("CARGO_HOME", "cargo-home".into()),
            ("RUSTUP_HOME", "rustup-home".into()),
        ],
    );
    let paths = paths(&environment).unwrap();
    for ancestor in root.ancestors() {
        for relative in [
            ".cargo/config",
            ".cargo/config.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            assert!(paths.contains(&ancestor.join(relative)));
        }
    }
    for relative in [
        "cargo-home/config",
        "cargo-home/config.toml",
        "rustup-home/settings.toml",
    ] {
        assert!(paths.contains(&root.join(relative)));
    }
    let mut observed = ToolConfiguration::capture(&environment, control()).unwrap();
    assert!(!observed.verified());
    observed.verify(control()).unwrap();
    assert!(observed.verified());
    assert!(observed.verify(control()).is_err());
}

#[test]
fn empty_tool_home_values_use_cargo_and_rustup_home_resolution() {
    let fixture = temporary_fixture("tool-configuration-home");
    let user = fixture.0.join("user");
    let environment = environment(
        &fixture.0,
        &[
            ("HOME", user.as_os_str().into()),
            ("USERPROFILE", user.as_os_str().into()),
            ("CARGO_HOME", OsString::new()),
            ("RUSTUP_HOME", OsString::new()),
        ],
    );
    let paths = paths(&environment).unwrap();
    assert!(paths.contains(&user.join(".cargo/config")));
    assert!(paths.contains(&user.join(".rustup/settings.toml")));
    let no_home = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    assert!(ToolConfiguration::capture(&no_home, control()).is_err());
}

#[cfg(unix)]
#[test]
fn rustup_system_fallback_and_its_override_are_bound() {
    let fixture = temporary_fixture("tool-configuration-fallback");
    let environment = environment(
        &fixture.0,
        &[
            ("CARGO_HOME", "cargo".into()),
            ("RUSTUP_HOME", "rustup".into()),
            (
                "RUSTUP_OVERRIDE_UNIX_FALLBACK_SETTINGS",
                "custom-fallback.toml".into(),
            ),
        ],
    );
    let paths = paths(&environment).unwrap();
    assert!(paths.contains(Path::new("/etc/rustup/settings.toml")));
    assert!(paths.contains(&fixture.0.join("custom-fallback.toml")));
    let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
    fs::write(
        fixture.0.join("custom-fallback.toml"),
        "default_toolchain = 'other'\n",
    )
    .unwrap();
    assert!(binding.verify(control()).is_err());
}

#[test]
fn absent_created_changed_and_removed_configuration_cannot_match() {
    let fixture = temporary_fixture("tool-configuration-changes");
    let path = fixture.0.join("config.toml");
    let mut missing = binding(&path);
    fs::write(&path, "compiler = 'a'\n").unwrap();
    assert!(missing.verify(control()).is_err());
    assert_eq!(missing.json()["before"][0]["present"], false);
    assert_eq!(missing.json()["after"][0]["present"], true);
    let mut changed = binding(&path);
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "compiler = 'b'\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(changed.verify(control()).is_err());
    assert_ne!(
        changed.json()["before"][0]["identity_sha256"],
        changed.json()["after"][0]["identity_sha256"]
    );
    let mut removed = binding(&path);
    fs::remove_file(&path).unwrap();
    assert!(removed.verify(control()).is_err());
    assert_eq!(removed.json()["after"][0]["present"], false);
}

#[test]
fn configuration_observation_is_bounded_and_never_reports_contents() {
    let fixture = temporary_fixture("tool-configuration-bounds");
    let path = fixture.0.join("config.toml");
    fs::write(&path, "secret = 'private-token-value'\n").unwrap();
    let paths = BTreeSet::from([path.clone()]);
    assert!(observe(&paths, control(), 1).is_err());
    assert!(observe(&BTreeSet::new(), control(), MAX_BYTES).is_err());
    let too_many = (0..=MAX_PATHS)
        .map(|index| fixture.0.join(index.to_string()))
        .collect();
    assert!(observe(&too_many, control(), MAX_BYTES).is_err());
    assert!(observe(&BTreeSet::from([fixture.0.clone()]), control(), MAX_BYTES).is_err());
    let report = binding(&path).json().to_string();
    assert!(!report.contains("private-token-value"));
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let error = observe(&paths, control().with_cancellation(&cancelled), MAX_BYTES)
        .err()
        .unwrap();
    assert_eq!(error.outcome, "cancelled");
}

#[cfg(unix)]
#[test]
fn configuration_symlink_retarget_permissions_and_fifo_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = temporary_fixture("tool-configuration-links");
    let first = fixture.0.join("first");
    let second = fixture.0.join("second");
    fs::write(&first, "same").unwrap();
    fs::write(&second, "same").unwrap();
    let link = fixture.0.join("config");
    symlink(&first, &link).unwrap();
    let mut linked = binding(&link);
    fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert!(linked.verify(control()).is_err());
    fs::set_permissions(&second, fs::Permissions::from_mode(0o644)).unwrap();
    let mut permissions = binding(&second);
    fs::set_permissions(&second, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(permissions.verify(control()).is_err());
    let fifo = fixture.0.join("fifo");
    let mut command = std::process::Command::new("mkfifo");
    command.arg(&fifo);
    control().run(&mut command, |_| Ok(())).unwrap();
    assert!(observe(&BTreeSet::from([fifo]), control(), MAX_BYTES).is_err());
    fs::remove_file(&second).unwrap();
    assert!(observe(&BTreeSet::from([link]), control(), MAX_BYTES).is_err());
}

#[cfg(unix)]
#[test]
fn native_configuration_paths_keep_unambiguous_byte_identities() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = temporary_fixture("tool-configuration-native-path");
    let path = fixture.0.join(OsString::from_vec(vec![b'c', 0xff]));
    fs::write(&path, "configuration").unwrap();
    let mut configuration = binding(&path);
    configuration.verify(control()).unwrap();
    let report = configuration.json();
    assert!(report["before"][0]["path"].is_null());
    assert_eq!(
        report["before"][0]["path_identity_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(report["verified"], true);
}
