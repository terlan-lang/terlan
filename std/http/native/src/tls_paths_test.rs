use super::*;
use crate::tls_config::Settings;
use std::fs;

fn manual() -> Config {
    Settings {
        mode: Some(Mode::Manual),
        cert: Some("cert.pem".into()),
        key: Some("key.pem".into()),
        ..Settings::default()
    }
    .validate()
    .unwrap()
}

#[test]
fn checks_each_manual_field_and_leaves_non_manual_modes_alone() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("cert.pem"), "certificate").unwrap();
    fs::write(dir.path().join("key.pem"), "key").unwrap();
    fs::write(dir.path().join("ca.pem"), "ca").unwrap();
    let mut tls = manual();
    validate_manual_tls_file_references(dir.path(), &tls).unwrap();
    tls.ca = Some("./ca.pem".into());
    validate_manual_tls_file_references(dir.path(), &tls).unwrap();
    for field in ["cert", "key", "ca"] {
        for invalid in ["missing.pem", "", ".", "../outside.pem", "/outside.pem"] {
            let mut invalid_tls = tls.clone();
            match field {
                "cert" => invalid_tls.cert = Some(invalid.into()),
                "key" => invalid_tls.key = Some(invalid.into()),
                _ => invalid_tls.ca = Some(invalid.into()),
            }
            let error = validate_manual_tls_file_references(dir.path(), &invalid_tls).unwrap_err();
            assert!(error.contains(field), "{error}");
        }
    }
    tls.cert = Some("/outside.pem".into());
    for mode in [Mode::Auto, Mode::Internal] {
        tls.mode = mode;
        validate_manual_tls_file_references(dir.path(), &tls).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_bypass_project_containment_for_checks_or_startup() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.pem"), "outside").unwrap();
    fs::write(root.path().join("inside.pem"), "inside").unwrap();
    fs::write(root.path().join("key.pem"), "key").unwrap();
    symlink(root.path().join("inside.pem"), root.path().join("cert.pem")).unwrap();
    validate_manual_tls_file_references(root.path(), &manual()).unwrap();
    symlink(outside.path(), root.path().join("outside")).unwrap();
    for field in ["cert", "key", "ca"] {
        let mut tls = manual();
        match field {
            "cert" => tls.cert = Some("outside/outside.pem".into()),
            "key" => tls.key = Some("outside/outside.pem".into()),
            _ => tls.ca = Some("outside/outside.pem".into()),
        }
        let error = validate_manual_tls_file_references(root.path(), &tls).unwrap_err();
        assert!(error.contains("stay inside the project"), "{error}");
        assert!(crate::tls_runtime::load(root.path(), &tls, |_| panic!("manual mode")).is_err());
    }
    fs::remove_file(root.path().join("cert.pem")).unwrap();
    symlink(
        outside.path().join("outside.pem"),
        root.path().join("cert.pem"),
    )
    .unwrap();
    assert!(validate_manual_tls_file_references(root.path(), &manual()).is_err());
}
