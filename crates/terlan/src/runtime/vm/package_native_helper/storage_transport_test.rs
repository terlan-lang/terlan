//! Binding admission rejects ambiguous authority before attempting to launch a worker.

use super::*;

#[test]
fn installed_storage_worker_is_selected_only_from_the_vm_directory() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("installed VM");
    let executable = directory.join("terlan-vm");
    let worker = installed_worker_path(&executable).unwrap();
    assert_eq!(worker.parent(), Some(directory.as_path()));
    assert_eq!(
        worker.file_name().unwrap(),
        if cfg!(windows) {
            "terlan-native-worker.exe"
        } else {
            "terlan-native-worker"
        }
    );
    for invalid in [
        Path::new(""),
        Path::new("terlan-vm"),
        Path::new("relative/terlan-vm"),
    ] {
        assert!(installed_worker_path(invalid)
            .unwrap_err()
            .contains("absolute executable"));
    }
}

#[test]
fn unconfigured_actor_execution_does_not_require_an_installed_storage_worker() {
    let mut helpers = super::super::VmPackageNativeHelpers::default();
    helpers.configure_storage(&[]).unwrap();
    assert!(helpers.storage_workers.is_empty());
}

#[test]
fn storage_bindings_are_unique_and_bounded_before_worker_spawn() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    let worker = directory.join("worker-does-not-exist");
    let primary = VmStorageBinding {
        name: "primary".into(),
        expected_identity: None,
        directory: directory.clone(),
    };
    for (bindings, expected) in [
        (
            vec![primary.clone(), primary.clone()],
            "duplicate backend name",
        ),
        (
            vec![
                primary.clone(),
                VmStorageBinding {
                    name: "other".into(),
                    expected_identity: None,
                    directory: directory.join("."),
                },
            ],
            "duplicate durable directory",
        ),
        (vec![primary.clone(); 17], "at most 16"),
        (
            vec![VmStorageBinding {
                name: "missing".into(),
                expected_identity: None,
                directory: directory.join("missing"),
            }],
            "must already exist",
        ),
    ] {
        let Err(error) = VmStorageWorkers::start(&bindings, &worker) else {
            panic!("invalid binding admitted")
        };
        assert!(error.contains(expected), "{error}");
    }
    assert!(VmStorageWorkers::start(&[], &worker).unwrap().is_empty());
}

#[test]
fn storage_binding_parser_preserves_path_but_rejects_noncanonical_names() {
    let root = tempfile::tempdir().unwrap();
    let directory = root
        .path()
        .canonicalize()
        .unwrap()
        .join("data=archive with spaces");
    let parsed = VmStorageBinding::parse(&format!("primary_2={}", directory.display())).unwrap();
    assert_eq!(parsed.name, "primary_2");
    assert_eq!(parsed.directory, directory);
    assert_eq!(parsed.expected_identity, None);
    for name in [
        "",
        "UPPER",
        "../escape",
        "white space",
        "two.names",
        "2first",
    ] {
        assert!(VmStorageBinding::parse(&format!("{name}={}", directory.display())).is_err());
    }
    assert!(
        VmStorageBinding::parse(&format!("{}={}", "a".repeat(65), directory.display())).is_err()
    );
}

#[test]
fn storage_binding_identity_is_explicit_canonical_and_nonzero() {
    let encoded = "0123456789abcdef".repeat(4);
    let binding = VmStorageBinding::parse(&format!("primary@{encoded}=/srv/data=archive")).unwrap();
    assert_eq!(binding.name, "primary");
    assert_eq!(binding.directory, PathBuf::from("/srv/data=archive"));
    assert_eq!(
        binding.expected_identity,
        Some([
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ])
    );
    for identity in [
        String::new(),
        "0".repeat(64),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
        format!("{encoded}@{encoded}"),
    ] {
        assert!(VmStorageBinding::parse(&format!("primary@{identity}=/srv/data")).is_err());
    }
    assert!(VmStorageBinding::parse(&format!("@{encoded}=/srv/data")).is_err());
    assert!(VmStorageBinding::parse(&format!("primary@{encoded}=relative")).is_err());
}

#[test]
fn different_identity_pins_do_not_allow_duplicate_backend_names() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().canonicalize().unwrap();
    let bindings = [1u8, 2].map(|byte| {
        VmStorageBinding::parse(&format!(
            "primary@{}={}",
            format!("{byte:02x}").repeat(32),
            path.display()
        ))
        .unwrap()
    });
    let Err(error) = VmStorageWorkers::start(&bindings, &path.join("missing-worker")) else {
        panic!("duplicate authority admitted")
    };
    assert!(error.contains("duplicate backend name"));
}
