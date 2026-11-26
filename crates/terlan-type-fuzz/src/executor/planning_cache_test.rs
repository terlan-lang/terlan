use super::*;

#[cfg(unix)]
#[test]
fn every_warm_profile_failure_is_attributed() {
    use std::os::unix::fs::PermissionsExt;

    for (failure, expected) in [(2, "narrow warm"), (4, "broad warm")] {
        let root = tempfile::tempdir().expect("planning root");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            format!(
                "#!/bin/sh\nprintf 'launch\\n' >> {}\n[ $(wc -l < {}) -ne {failure} ]\n",
                launches.display(),
                launches.display(),
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        let options = RunOptions {
            oracle: Oracle::Native,
            out_dir: root.path().join("cases"),
            ..RunOptions::default()
        };
        assert!(run(&options, &compiler, 2_000)
            .unwrap_err()
            .contains(&format!("{expected} native planning probe")));
    }
}

#[test]
fn planning_root_reset_failures_are_attributed() {
    let root = tempfile::tempdir().expect("planning root");
    let output = root.path().join("cases");
    fs::create_dir(&output).expect("cases root");
    fs::write(output.join("planning"), "not a directory").expect("blocking file");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: output,
        ..RunOptions::default()
    };
    assert!(run(&options, Path::new("missing"), 2_000)
        .unwrap_err()
        .contains("reset planning fixtures"));
}

#[test]
fn planning_fixture_creation_failure_is_attributed_by_run() {
    let root = tempfile::tempdir().expect("planning root");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: root.path().join("x".repeat(300)),
        ..RunOptions::default()
    };
    assert!(run(&options, Path::new("missing"), 2_000)
        .unwrap_err()
        .contains("create planning fixture"));
}
