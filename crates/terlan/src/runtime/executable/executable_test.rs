use super::*;
use std::fs;
use std::io::Write;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let runtime = directory.path().join("app");
    fs::write(&runtime, b"runtime prefix").unwrap();
    let image = directory.path().join("image.tvm");
    fs::write(&image, b"compiled native image").unwrap();
    let asset = directory.path().join("asset");
    fs::write(&asset, "hello 🌍").unwrap();
    append_application(
        &runtime,
        ApplicationEntry::Cli {
            image: "vm/main.tvm".into(),
            function: "app.Main.main".into(),
        },
        &[
            ("vm/main.tvm".into(), image),
            ("assets/hello.txt".into(), asset),
        ],
        vec![],
    )
    .unwrap();
    (directory, runtime)
}

#[test]
fn embedded_application_is_relocatable_and_owns_its_resources() {
    let (source, executable) = fixture();
    let destination = tempfile::tempdir().unwrap();
    let relocated = destination.path().join("renamed app");
    fs::copy(&executable, &relocated).unwrap();
    drop(source);
    let mut application = EmbeddedApplication::open(&relocated).unwrap().unwrap();
    let resources = application.materialize().unwrap();
    assert_eq!(
        fs::read(resources.root().join("vm/main.tvm")).unwrap(),
        b"compiled native image"
    );
    assert_eq!(
        fs::read_to_string(resources.root().join("assets/hello.txt")).unwrap(),
        "hello 🌍"
    );
    let root = resources.root().to_path_buf();
    drop(resources);
    assert!(!root.exists());
}

#[test]
fn damaged_application_is_rejected_before_native_admission() {
    let (_directory, executable) = fixture();
    let mut bytes = fs::read(&executable).unwrap();
    bytes[15] ^= 1;
    fs::write(&executable, &bytes).unwrap();
    assert!(EmbeddedApplication::open(&executable)
        .err()
        .unwrap()
        .contains("application.digest"));
}

#[test]
fn mutation_after_manifest_admission_is_rejected_when_reading_resources() {
    let (_directory, executable) = fixture();
    let mut application = EmbeddedApplication::open(&executable).unwrap().unwrap();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(&executable)
        .unwrap();
    file.seek(SeekFrom::Start(15)).unwrap();
    file.write_all(b"wrong").unwrap();
    assert!(application
        .materialize()
        .err()
        .unwrap()
        .contains("application.digest"));
}

#[test]
fn invalid_lengths_cannot_allocate_or_read_outside_the_executable() {
    let (_directory, executable) = fixture();
    let original = fs::read(&executable).unwrap();
    for (offset, value) in [(16, u64::MAX), (24, u64::MAX), (24, 0)] {
        let mut bytes = original.clone();
        let start = bytes.len() - FOOTER_LEN as usize + offset;
        bytes[start..start + 8].copy_from_slice(&value.to_le_bytes());
        fs::write(&executable, bytes).unwrap();
        assert!(EmbeddedApplication::open(&executable)
            .err()
            .unwrap()
            .contains("application.bounds"));
    }
}

#[test]
fn resource_paths_cannot_escape_or_use_platform_devices() {
    for path in [
        "",
        "/etc/passwd",
        "../outside",
        "a/../b",
        "a//b",
        "a/./b",
        "C:/outside",
        "a\\b",
        "NUL.txt",
        "assets/COM1",
        "trailing.",
        "trailing ",
    ] {
        assert!(validate_path(path).is_err(), "{path}");
    }
    validate_path("web/static/hello world.js").unwrap();
}

#[test]
fn duplicate_paths_and_missing_entries_reject_package_emission() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("input");
    fs::write(&image, b"image").unwrap();
    for sources in [
        vec![
            ("Main.tvm".into(), image.clone()),
            ("main.tvm".into(), image.clone()),
        ],
        vec![("other.tvm".into(), image.clone())],
    ] {
        let executable = directory.path().join("app");
        fs::write(&executable, b"runtime").unwrap();
        assert!(append_application(
            &executable,
            ApplicationEntry::Cli {
                image: "Main.tvm".into(),
                function: "main".into(),
            },
            &sources,
            vec![],
        )
        .is_err());
    }
}

#[test]
fn bare_runtimes_and_packaged_runtimes_are_distinguished() {
    let (directory, executable) = fixture();
    let bare = directory.path().join("runtime");
    fs::write(&bare, [0_u8; 128]).unwrap();
    assert!(!is_application(&bare).unwrap());
    assert!(is_application(&executable).unwrap());
    assert!(EmbeddedApplication::open(&bare).unwrap().is_none());
    assert!(append_application(
        &executable,
        ApplicationEntry::Service {
            web_root: "web".into()
        },
        &[],
        vec![],
    )
    .is_err());
}
