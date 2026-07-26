use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{VmFatalDiagnosticBundle, VmFatalDiagnosticPolicy};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::VmScheduler;

fn temp_directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "terlan-ignore-cores-parity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ))
}

fn fatal_bundle() -> VmFatalDiagnosticBundle {
    let mut processes = VmProcessTable::default();
    processes.spawn_root(VmProcessSource::new("app.IgnoreCoresParity", "crash", 0));
    VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(4, 32 * 1024).expect("diagnostic policy"),
        29,
        "vm.test_crash",
        &processes,
        &VmScheduler::default(),
        &[],
    )
    .expect("capture diagnostic")
    .expect("enabled diagnostic")
}

#[test]
fn ignore_cores_helper_is_replaced_by_explicit_non_mutating_artifact_publication() {
    let root = temp_directory();
    let ambient = root.join("ambient");
    let output = root.join("artifacts").join("case-29");
    let unrelated_cores = root.join("system-cores");
    fs::create_dir_all(&ambient).expect("create ambient fixture");
    fs::create_dir_all(&unrelated_cores).expect("create unrelated core fixture");
    let marker = ambient.join("ignore_core_files");
    let unrelated_core = unrelated_cores.join("core.4242");
    fs::write(&marker, b"pre-existing marker").expect("write marker fixture");
    fs::write(&unrelated_core, b"unrelated core bytes").expect("write core fixture");

    let cwd_before = std::env::current_dir().expect("read cwd");
    let pwd_before = std::env::var_os("PWD");
    let destination = output.join("fatal-diagnostic.json");
    fatal_bundle()
        .publish_atomic(&destination)
        .expect("publish to explicit artifact directory");

    assert_eq!(
        std::env::current_dir().expect("read cwd after publish"),
        cwd_before
    );
    assert_eq!(std::env::var_os("PWD"), pwd_before);
    assert_eq!(
        fs::read(&marker).expect("read untouched marker"),
        b"pre-existing marker"
    );
    assert_eq!(
        fs::read(&unrelated_core).expect("read untouched core"),
        b"unrelated core bytes"
    );
    assert!(!output.join("ignore_core_files").exists());
    let entries = fs::read_dir(&output)
        .expect("read explicit output directory")
        .map(|entry| entry.expect("read output entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [destination.file_name().expect("destination name")]
    );

    let published = fs::read(&destination).expect("read published diagnostic");
    assert!(fatal_bundle().publish_atomic(&destination).is_err());
    assert_eq!(
        fs::read(&destination).expect("collision preserves diagnostic"),
        published
    );
    assert_eq!(std::env::current_dir().expect("read final cwd"), cwd_before);
    assert_eq!(std::env::var_os("PWD"), pwd_before);
    assert_eq!(
        fs::read(&unrelated_core).expect("read final untouched core"),
        b"unrelated core bytes"
    );

    fs::remove_dir_all(root).expect("remove parity fixture");
}
