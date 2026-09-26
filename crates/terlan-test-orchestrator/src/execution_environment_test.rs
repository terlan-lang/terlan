use super::*;
use crate::test_orchestrator_test::temporary_fixture;

fn snapshot(entries: &[(&str, &str)], root: &Path) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        entries
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
        root,
    )
    .unwrap()
}

#[test]
fn environment_identity_is_order_independent_and_preserves_empty_values() {
    let fixture = temporary_fixture("environment-order");
    let first = snapshot(&[("EMPTY", ""), ("TOKEN", "secret-token")], &fixture.0);
    let second = snapshot(&[("TOKEN", "secret-token"), ("EMPTY", "")], &fixture.0);
    assert_eq!(first.json(&[]), second.json(&[]));
    assert_eq!(first.value("EMPTY"), Some(OsString::new()));
    assert_eq!(first.value("MISSING"), None);
    assert_eq!(first.value("TOKEN"), Some("secret-token".into()));
    assert_ne!(
        first.json(&[]),
        snapshot(&[("TOKEN", "secret-token")], &fixture.0).json(&[])
    );
    assert_ne!(
        first.json(&[]),
        snapshot(&[("TOKEN", "changed"), ("EMPTY", "")], &fixture.0).json(&[])
    );
    let json = first.json(&[]).to_string();
    for private in ["TOKEN", "EMPTY", "secret-token"] {
        assert!(!json.contains(private));
    }
}

#[test]
fn certificate_defaults_are_frozen_bounded_and_preserve_existing_valid_choices() {
    let fixture = temporary_fixture("environment-certificates");
    let default_file = fixture.0.join("default.pem");
    let custom_file = fixture.0.join("custom.pem");
    std::fs::write(&default_file, "default").unwrap();
    std::fs::write(&custom_file, "custom").unwrap();
    for configured in ["", "missing", "custom.pem"] {
        let original = snapshot(
            &[("SSL_CERT_FILE", configured), ("SSL_CERT_DIR", "")],
            &fixture.0,
        );
        let captured = original
            .with_certificates(openssl_probe::ProbeResult {
                cert_file: Some(default_file.clone()),
                cert_dir: vec![fixture.0.clone()],
            })
            .unwrap();
        assert_eq!(
            captured.value("SSL_CERT_FILE"),
            Some(if configured == "custom.pem" {
                configured.into()
            } else {
                default_file.clone().into_os_string()
            })
        );
        assert_eq!(
            captured.value("SSL_CERT_DIR"),
            Some(fixture.0.clone().into_os_string())
        );
    }
    let full = ExecutionEnvironment::from_entries(
        (0..MAX_ENTRIES).map(|index| (format!("ENTRY_{index}").into(), "value".into())),
        &fixture.0,
    )
    .unwrap();
    assert!(full
        .with_certificates(openssl_probe::ProbeResult {
            cert_file: Some(default_file),
            cert_dir: vec![fixture.0.clone()]
        })
        .is_err());
}

#[test]
fn environment_identity_separates_fields_and_binds_working_directory() {
    let one = temporary_fixture("environment-directory-one");
    let two = temporary_fixture("environment-directory-two");
    let first = snapshot(&[("AB", "C")], &one.0);
    assert_ne!(first.json(&[]), snapshot(&[("A", "BC")], &one.0).json(&[]));
    assert_ne!(first.json(&[]), snapshot(&[("AB", "C")], &two.0).json(&[]));
    assert_eq!(
        first.command(Path::new("tool")).get_current_dir(),
        Some(one.0.as_path())
    );
}

#[test]
fn declared_overrides_bind_effective_values_without_changing_snapshot() {
    let fixture = temporary_fixture("environment-overrides");
    let environment = snapshot(&[("MODE", "inherited")], &fixture.0);
    let before = environment.json(&[]);
    let mut phase = crate::terlan_library_phase();
    phase.environment = vec![("MODE", "phase".into())];
    let report = environment.json(&[phase.clone()]);
    let mut expected = environment.test_command(Path::new("tool"));
    expected.env("MODE", "phase");
    assert_eq!(report["phases"][0]["identity_sha256"], identity(&expected));
    assert_ne!(
        report["phases"][0]["identity_sha256"],
        report["test_identity_sha256"]
    );
    phase.environment.push(("MODE", "inherited".into()));
    assert_eq!(
        environment.json(&[phase])["phases"][0]["identity_sha256"],
        report["test_identity_sha256"]
    );
    assert_eq!(environment.json(&[]), before);
    assert_eq!(environment.value("MODE"), Some("inherited".into()));
}

#[test]
fn environment_admission_is_bounded_and_errors_do_not_disclose_values() {
    let fixture = temporary_fixture("environment-budget");
    for entries in [
        vec![(OsString::new(), OsString::from("private-empty-key"))],
        vec![(OsString::from("KEY"), OsString::from("private\0value"))],
        vec![(OsString::from("KEY"), OsString::from("x".repeat(MAX_BYTES)))],
        vec![(OsString::from("KEY"), OsString::from("private")); MAX_ENTRIES + 1],
    ] {
        let error = ExecutionEnvironment::from_entries(entries, &fixture.0)
            .err()
            .unwrap();
        assert_eq!(error.outcome, "environment-admission-failed");
        assert!(!error.detail.contains("private"));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_environment_and_path_are_preserved_without_lossy_collisions() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = temporary_fixture("environment-native-bytes");
    let value = OsString::from_vec(vec![b'/', b't', 0xff]);
    let environment = ExecutionEnvironment::from_entries(
        vec![
            ("PATH".into(), value.clone()),
            ("TOKEN".into(), value.clone()),
        ],
        &fixture.0,
    )
    .unwrap();
    assert_eq!(environment.value("TOKEN"), Some(value.clone()));
    assert_eq!(
        env::split_paths(environment.test_path()).last(),
        Some(PathBuf::from(&value))
    );
    let other = snapshot(
        &[
            ("PATH", &value.to_string_lossy()),
            ("TOKEN", &value.to_string_lossy()),
        ],
        &fixture.0,
    );
    assert_ne!(environment.json(&[]), other.json(&[]));
    assert_eq!(
        snapshot(&[("Path", "lower"), ("PATH", "upper")], &fixture.0).value("PATH"),
        Some("upper".into())
    );
}

#[cfg(windows)]
#[test]
fn windows_keys_follow_command_case_insensitive_replacement() {
    let fixture = temporary_fixture("environment-windows-keys");
    let environment = snapshot(&[("Path", "first"), ("PATH", "second")], &fixture.0);
    assert_eq!(environment.value("path"), Some("second".into()));
    assert_eq!(environment.entries.len(), 1);
    let command = environment.test_command(Path::new("tool"));
    assert_eq!(command.get_envs().count(), 1);
    assert_eq!(
        command.get_envs().next().unwrap().1,
        Some(environment.test_path())
    );
}

#[cfg(unix)]
#[test]
fn actual_children_receive_only_snapshot_and_declared_phase_overrides() {
    use std::time::Duration;
    use terlan_process_owner::ProcessControl;
    let fixture = temporary_fixture("environment-real-child");
    let environment = snapshot(&[("FIXTURE_VALUE", "base")], &fixture.0);
    let control = ProcessControl::new(Duration::from_secs(5));
    let mut launched = 0;
    let mut observe = |pid| {
        assert!(pid > 0);
        launched += 1;
        Ok(())
    };
    let mut command = environment.command(Path::new("/bin/sh"));
    command.args([
        "-c",
        "test \"$FIXTURE_VALUE\" = base && test -z \"${CARGO_HOME+x}\" && test -z \"${HOME+x}\"",
    ]);
    control.run(&mut command, &mut observe).unwrap();
    let mut phase = crate::terlan_library_phase();
    phase.environment = vec![("FIXTURE_VALUE", "override".into())];
    phase.args = vec!["-c", "test \"$FIXTURE_VALUE\" = override && test -z \"${HOME+x}\" && test \"${PATH%%:*}\" = \"$PWD/target/debug\" || exit 9; while test \"$1\" != --logfile; do shift; done; printf 'ok fixture\\n' > \"$2\""];
    crate::test_execution::run(
        &phase,
        Path::new("/bin/sh"),
        &environment,
        1,
        Some(crate::test_execution::ExpectedTests {
            passed: ["fixture".to_owned()].into(),
            ignored: Default::default(),
            filtered: 0,
        }),
        control,
        &mut observe,
    )
    .unwrap();
    assert_eq!(launched, 2);
    assert_eq!(environment.value("FIXTURE_VALUE"), Some("base".into()));
}
