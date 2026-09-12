use super::*;
use crate::support::test_fs::TestDirectory;
use std::collections::BTreeMap;

fn host(root: &Path) -> ProofEnvironment {
    ProofEnvironment::from_lookup(|key| match key {
        "PATH" => Some(root.as_os_str().into()),
        "ELAN_HOME" => Some(root.as_os_str().into()),
        "SystemRoot" => Some(root.as_os_str().into()),
        _ => None,
    })
    .unwrap()
}

#[test]
fn proof_environment_pins_tools_and_drops_undeclared_variables() {
    let root = TestDirectory::new("proof_environment", "closed");
    let environment = host(&root);
    let mut command = Command::new("unused");
    command
        .env("LEAN_PATH", "outside")
        .env("ELAN_TOOLCHAIN", "other")
        .env("LD_PRELOAD", "injected")
        .env("TERLAN_PROOF_UNDECLARED", "outside");
    environment
        .configure(&mut command, &root, "leanprover/lean4:v4.31.0")
        .unwrap();
    let values = command
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.unwrap().to_owned()))
        .collect::<BTreeMap<_, _>>();
    for forbidden in ["LEAN_PATH", "LD_PRELOAD", "TERLAN_PROOF_UNDECLARED"] {
        assert!(!values.contains_key(std::ffi::OsStr::new(forbidden)));
    }
    assert_eq!(
        values[std::ffi::OsStr::new("ELAN_TOOLCHAIN")],
        "leanprover/lean4:v4.31.0"
    );
    assert_eq!(
        values[std::ffi::OsStr::new("HOME")],
        fs::canonicalize(&root).unwrap().join("home")
    );
    assert_eq!(
        values[std::ffi::OsStr::new("TMPDIR")],
        fs::canonicalize(&root).unwrap().join("tmp")
    );
    assert_eq!(values[std::ffi::OsStr::new("LC_ALL")], "C");
}

#[test]
fn proof_environment_rejects_cwd_search_and_relative_elan_home() {
    let root = TestDirectory::new("proof_environment", "paths");
    for path in [
        OsString::new(),
        OsString::from("."),
        std::env::join_paths([root.path(), Path::new("")]).unwrap(),
    ] {
        let result = ProofEnvironment::from_lookup(|key| match key {
            "PATH" => Some(path.clone()),
            "ELAN_HOME" | "SystemRoot" => Some(root.as_os_str().into()),
            _ => None,
        });
        assert!(result.is_err());
    }
    for elan in ["", ".", "relative"] {
        let result = ProofEnvironment::from_lookup(|key| match key {
            "PATH" | "SystemRoot" => Some(root.as_os_str().into()),
            "ELAN_HOME" => Some(elan.into()),
            _ => None,
        });
        assert!(result.is_err());
    }
}

#[test]
fn proof_environment_uses_home_only_to_locate_installed_elan() {
    let root = TestDirectory::new("proof_environment", "default_elan");
    fs::create_dir(root.join(".elan")).unwrap();
    let environment = ProofEnvironment::from_lookup(|key| match key {
        "PATH" | "HOME" | "USERPROFILE" | "SystemRoot" => Some(root.as_os_str().into()),
        _ => None,
    })
    .unwrap();
    assert_eq!(environment.elan_home, root.join(".elan"));
}

#[test]
fn proof_environment_selects_absolute_tool_and_rejects_missing_program() {
    let root = TestDirectory::new("proof_environment", "tools");
    let environment = host(&root);
    assert!(environment.program("lake").is_err());
    assert!(environment.program("../lake").is_err());
    let path = root.join(if cfg!(windows) { "lake.exe" } else { "lake" });
    fs::write(&path, b"fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(environment.program("lake").is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(environment.program("lake").unwrap(), path);
}

#[cfg(unix)]
#[test]
fn proof_environment_preserves_elan_proxy_name() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = TestDirectory::new("proof_environment", "proxy");
    let target = root.join("elan");
    fs::write(&target, b"fixture").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&target, root.join("lake")).unwrap();
    assert_eq!(host(&root).program("lake").unwrap(), root.join("lake"));
}

#[cfg(unix)]
#[test]
fn proof_environment_real_child_has_private_home_and_no_ambient_flags() {
    let root = TestDirectory::new("proof_environment", "child");
    let environment = host(&root);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", r#"test -z "${LEAN_PATH+x}" && test -z "${TERLAN_PROOF_UNDECLARED+x}" && test "$ELAN_TOOLCHAIN" = "pinned" && test "$HOME" = "$USERPROFILE" && test "$TMP" = "$TMPDIR" && test "$TEMP" = "$TMPDIR" && test -d "$HOME" && test -d "$TMPDIR" && printf retained > "$HOME/file" && printf scratch > "$TMPDIR/file""#]);
    command
        .env("LEAN_PATH", "outside")
        .env("TERLAN_PROOF_UNDECLARED", "outside");
    environment
        .configure(&mut command, &root, "pinned")
        .unwrap();
    let output = crate::commands::process_runner::run_command_with_timeout(
        &mut command,
        "proof environment fixture",
        std::time::Duration::from_secs(2),
    )
    .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read(root.join("home/file")).unwrap(), b"retained");
    assert_eq!(fs::read(root.join("tmp/file")).unwrap(), b"scratch");
}

#[test]
fn proof_environment_requires_exact_version_banner() {
    assert!(matches_version(
        "Lean (version 4.31.0, x86_64-unknown-linux-gnu, Release)\n",
        "4.31.0"
    ));
    for value in [
        "Lean (version 4.31.01, Release)",
        "Lean (version 4.31.0-rc1, Release)",
        "prefix Lean (version 4.31.0, Release)",
        "Lean (version 4.31.0, Release)\nextra",
        "Lean (version 4.31.0, Release)\nLean (version 4.31.0, Release)",
    ] {
        assert!(!matches_version(value, "4.31.0"), "{value}");
    }
}
