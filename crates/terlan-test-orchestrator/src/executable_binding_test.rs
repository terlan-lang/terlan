use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn executable(root: &Path, name: &str, contents: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn independent_bindings_honor_the_callers_remaining_byte_budget() {
    let fixture = temporary_fixture("executable-remaining-budget");
    let path = executable(&fixture.0, "program", "ten bytes!");
    assert!(
        ExecutableBinding::capture_bounded(&[("program", path.clone())], 10, control()).is_ok()
    );
    assert!(
        ExecutableBinding::capture_bounded(&[("program", path.clone())], 9, control()).is_err()
    );
    assert!(ExecutableBinding::capture_bounded(
        &[("program", path)],
        MAX_EXECUTABLE_BYTES + 1,
        control()
    )
    .is_err());
}

#[test]
fn test_program_outputs_bind_after_build_and_reject_later_changes_or_rebinding() {
    let fixture = temporary_fixture("test-program-output-admission");
    let compiler = executable(&fixture.0, "compiler", "compiler input");
    let output = fixture.0.join("cli");
    let mut binding = ExecutableBinding::capture(&[("compiler", compiler)], control()).unwrap();
    assert!(binding
        .append_programs(&[("terlc", output.clone())], control())
        .is_err());
    assert_eq!(binding.json()["before"].as_array().unwrap().len(), 1);
    executable(&fixture.0, "cli", "newly built program");
    binding
        .append_programs(&[("terlc", output.clone())], control())
        .unwrap();
    assert!(binding
        .append_programs(&[("terlc", output.clone())], control())
        .is_err());
    binding.verify_program("terlc", control()).unwrap();
    fs::write(output, "changed after test admission").unwrap();
    assert!(binding.verify(control()).is_err());
}

#[test]
fn extra_program_admission_preserves_shared_budgets_and_is_atomic() {
    let fixture = temporary_fixture("test-program-output-budget");
    let compiler = executable(&fixture.0, "compiler", "compiler input");
    let mut binding =
        ExecutableBinding::capture(&[("compiler", compiler.clone())], control()).unwrap();
    let before = binding.json();
    assert!(binding
        .append_programs(
            &[("output", compiler.clone()), ("output", compiler.clone())],
            control()
        )
        .is_err());
    assert_eq!(binding.json(), before);
    assert!(binding
        .append_programs(&vec![("output", compiler); 16], control())
        .is_err());
    assert_eq!(binding.json(), before);
    binding.verify(control()).unwrap();
    assert!(binding.append_programs(&[], control()).is_err());
}

#[test]
fn executable_identity_requires_real_nonempty_executable_files() {
    let fixture = temporary_fixture("executable-admission");
    assert!(ExecutableBinding::capture(&[], control()).is_err());
    let missing = fixture.0.join("missing");
    assert!(ExecutableBinding::capture(&[("tool", missing)], control()).is_err());
    let empty = executable(&fixture.0, "empty", "");
    assert!(ExecutableBinding::capture(&[("tool", empty)], control()).is_err());
    assert!(ExecutableBinding::capture(&[("directory", fixture.0.clone())], control()).is_err());
    let tool = executable(&fixture.0, "tool", "real bytes");
    assert!(ExecutableBinding::capture(
        &[("tool", tool.clone()), ("tool", tool.clone())],
        control()
    )
    .is_err());
    let mut binding = ExecutableBinding::capture(&[("tool", tool)], control()).unwrap();
    assert!(!binding.verified());
    binding.verify(control()).unwrap();
    assert!(binding.verified());
    assert!(binding.verify(control()).is_err());
}

#[test]
fn same_length_timestamp_restored_replacement_changes_executable_identity() {
    let fixture = temporary_fixture("executable-bytes");
    let path = executable(&fixture.0, "tool", "before");
    let time = fs::metadata(&path).unwrap().modified().unwrap();
    let mut binding = ExecutableBinding::capture(&[("tool", path.clone())], control()).unwrap();
    fs::write(&path, "change").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(time)
        .unwrap();
    assert!(binding.verify(control()).is_err());
    let report = binding.json();
    assert_ne!(
        report["before"][0]["identity_sha256"],
        report["after"][0]["identity_sha256"]
    );
    assert_eq!(report["verified"], false);
}

#[test]
fn harness_replacement_is_rejected_before_another_launch() {
    let fixture = temporary_fixture("executable-harness");
    let tool = executable(&fixture.0, "driver", "driver");
    let harness = executable(&fixture.0, "harness", "harness-one");
    let mut binding = ExecutableBinding::capture(&[("driver", tool)], control()).unwrap();
    assert!(binding.verify_harness(control()).is_err());
    binding.bind_harness(&harness, control()).unwrap();
    binding.verify_harness(control()).unwrap();
    assert!(binding.bind_harness(&harness, control()).is_err());
    let replacement = executable(&fixture.0, "replacement", "harness-two");
    fs::remove_file(&harness).unwrap();
    fs::rename(replacement, &harness).unwrap();
    assert!(binding.verify_harness(control()).is_err());
    assert!(binding.verify(control()).is_err());
}

#[cfg(unix)]
#[test]
fn executable_symlink_retarget_and_permission_changes_are_observed() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = temporary_fixture("executable-links");
    let one = executable(&fixture.0, "one", "same bytes");
    let two = executable(&fixture.0, "two", "same bytes");
    let link = fixture.0.join("link");
    symlink(&one, &link).unwrap();
    let mut binding = ExecutableBinding::capture(&[("tool", link.clone())], control()).unwrap();
    fs::remove_file(&link).unwrap();
    symlink(&two, &link).unwrap();
    assert!(binding.verify(control()).is_err());
    fs::set_permissions(&two, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(ExecutableBinding::capture(&[("tool", link)], control()).is_err());
}

#[test]
fn program_resolution_observes_path_order_and_explicit_paths() {
    let fixture = temporary_fixture("executable-path");
    let first = fixture.0.join("first");
    let second = fixture.0.join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let name = format!("tool{}", std::env::consts::EXE_SUFFIX);
    let one = executable(&first, &name, "first");
    let two = executable(&second, &name, "second");
    let path = std::env::join_paths([&first, &second]).unwrap();
    assert_eq!(resolve_program("tool", &path).unwrap(), one);
    let reversed = std::env::join_paths([&second, &first]).unwrap();
    assert_eq!(resolve_program("tool", &reversed).unwrap(), two);
    assert_eq!(
        resolve_program(one.to_str().unwrap(), OsStr::new("")).unwrap(),
        one
    );
    assert!(resolve_program("missing", &path).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&one, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(resolve_program("tool", &path).unwrap(), two);
    }
}

#[test]
fn executable_hashing_shares_budgets_and_cancellation_with_source_inputs() {
    let fixture = temporary_fixture("executable-limits");
    let path = executable(&fixture.0, "tool", "bytes");
    assert!(capture_file("tool", &path, 4, control(), Instant::now()).is_err());
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let error = capture_file(
        "tool",
        &path,
        5,
        control().with_cancellation(&cancelled),
        Instant::now(),
    )
    .unwrap_err();
    assert_eq!(error.outcome, "cancelled");
}

#[cfg(unix)]
#[test]
fn admitted_invocation_path_preserves_dispatch_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let fixture = temporary_fixture("executable-dispatch");
    let real = executable(
        &fixture.0,
        "real-tool",
        "#!/bin/sh\ncase \"$0\" in */dispatch-probe) exit 0;; *) exit 9;; esac\n",
    );
    let alias = fixture.0.join("dispatch-probe");
    symlink(&real, &alias).unwrap();
    let binding = ExecutableBinding::capture(&[("tool", alias.clone())], control()).unwrap();
    let admitted = binding.verify_program("tool", control()).unwrap();
    assert_eq!(admitted, alias);
    control()
        .run(&mut std::process::Command::new(admitted), |_| Ok(()))
        .unwrap();
    assert!(control()
        .run(&mut std::process::Command::new(real), |_| Ok(()))
        .is_err());
}
