use super::*;
use crate::test_orchestrator_test::temporary_fixture;

#[test]
fn proxy_path_inserts_only_missing_entries_and_preserves_existing_order() {
    let fixture = temporary_fixture("cargo-proxy-path");
    let home = fixture.0.join("cargo-home");
    let toolchain = fixture.0.join("toolchain");
    let first = fixture.0.join("first");
    let last = fixture.0.join("last");
    for existing in [false, true] {
        let original = if existing {
            vec![first.clone(), home.join("bin"), last.clone()]
        } else {
            vec![first.clone(), last.clone()]
        };
        let path = std::env::join_paths(&original).unwrap();
        let merged = merge(&path, &home, &toolchain, false, None).unwrap();
        let expected = if existing {
            original
        } else {
            vec![home.join("bin"), first.clone(), last.clone()]
        };
        assert_eq!(std::env::split_paths(&merged).collect::<Vec<_>>(), expected);
    }
}

#[test]
fn windows_policy_appends_prepends_or_omits_toolchain_bin_without_duplicates() {
    let fixture = temporary_fixture("cargo-proxy-windows-path");
    let home = fixture.0.join("cargo-home");
    let toolchain = fixture.0.join("toolchain");
    let inherited = fixture.0.join("inherited");
    let path = std::env::join_paths([&inherited]).unwrap();
    for (policy, expected) in [
        (
            None,
            vec![home.join("bin"), inherited.clone(), toolchain.join("bin")],
        ),
        (
            Some(OsStr::new("append")),
            vec![home.join("bin"), inherited.clone(), toolchain.join("bin")],
        ),
        (
            Some(OsStr::new("0")),
            vec![home.join("bin"), inherited.clone()],
        ),
        (
            Some(OsStr::new("1")),
            vec![home.join("bin"), toolchain.join("bin"), inherited.clone()],
        ),
    ] {
        let merged = merge(&path, &home, &toolchain, true, policy).unwrap();
        assert_eq!(std::env::split_paths(&merged).collect::<Vec<_>>(), expected);
        assert_eq!(
            merge(&merged, &home, &toolchain, true, policy).unwrap(),
            merged
        );
    }
}

#[cfg(unix)]
#[test]
fn real_rustup_proxy_changes_tool_lookup_only_when_cargo_home_bin_is_absent() {
    use crate::cargo_tool_settings::CargoToolSettings;
    use crate::configured_cargo_tools::ConfiguredCargoTools;
    use crate::executable_binding::resolve_program;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::process::Command;
    use std::time::Duration;
    use terlan_process_owner::ProcessControl;
    let fixture = temporary_fixture("cargo-proxy-real");
    let control = ProcessControl::new(Duration::from_secs(20));
    let rustup = resolve_program("rustup", &std::env::var_os("PATH").unwrap()).unwrap();
    let output = control
        .capture_stdout(
            Command::new(&rustup).args(["which", "cargo"]),
            64 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let cargo = std::path::PathBuf::from(std::str::from_utf8(&output).unwrap().trim());
    let toolchain = cargo.parent().unwrap().parent().unwrap();
    fs::create_dir_all(fixture.0.join(".cargo")).unwrap();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::create_dir(fixture.0.join("proxy")).unwrap();
    let proxy = fixture.0.join("proxy/cargo");
    symlink(&rustup, &proxy).unwrap();
    fs::write(
        fixture.0.join("Cargo.toml"),
        "[package]\nname = 'proxy_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = 'proxy_fixture'\nversion = '0.0.0'\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("src/lib.rs"),
        "// Probe exits before compilation.\n",
    )
    .unwrap();
    for (directory, marker) in [
        ("cargo-home/bin", "cargo-home"),
        ("inherited", "inherited"),
        ("forced", "forced"),
    ] {
        let directory = fixture.0.join(directory);
        fs::create_dir_all(&directory).unwrap();
        let compiler = directory.join("probe-compiler");
        fs::write(
            &compiler,
            format!("#!/bin/sh\nprintf '%s\\n' '{marker}' > \"$FIXTURE_OUTPUT\"\nexit 89\n"),
        )
        .unwrap();
        fs::set_permissions(compiler, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for case in [
        "native",
        "proxy-add",
        "proxy-preserve",
        "proxy-force",
        "proxy-nonforce",
    ] {
        let output = fixture.0.join(format!("{case}.output"));
        let mut search = vec![
            fixture.0.join("inherited"),
            "/usr/bin".into(),
            "/bin".into(),
        ];
        if case == "proxy-preserve" {
            search.push(fixture.0.join("cargo-home/bin"));
        }
        let environment = ExecutionEnvironment::from_entries(
            [
                ("PATH".into(), std::env::join_paths(search).unwrap()),
                (
                    "CARGO_HOME".into(),
                    fixture.0.join("cargo-home").into_os_string(),
                ),
                (
                    "RUSTUP_HOME".into(),
                    std::env::var_os("RUSTUP_HOME").unwrap(),
                ),
                (
                    "RUSTUP_TOOLCHAIN".into(),
                    toolchain.file_name().unwrap().to_owned(),
                ),
                ("FIXTURE_OUTPUT".into(), output.clone().into_os_string()),
            ],
            &fixture.0,
        )
        .unwrap();
        let mut source = "[build]\nrustc = 'probe-compiler'\n".to_owned();
        if matches!(case, "proxy-force" | "proxy-nonforce") {
            source.push_str(&format!(
                "[env]\nPATH = {{value = 'forced', relative = true, force = {}}}\n",
                case == "proxy-force"
            ));
        }
        fs::write(fixture.0.join(".cargo/config.toml"), &source).unwrap();
        let settings = CargoToolSettings::from_document(
            &fixture.0.join(".cargo/config.toml"),
            &toml::from_str(&source).unwrap(),
        )
        .unwrap();
        let mut binding = if case == "native" {
            ConfiguredCargoTools::capture(&settings, &environment, control).unwrap()
        } else {
            ConfiguredCargoTools::capture_for_proxy(&settings, &environment, toolchain, control)
                .unwrap()
        };
        let expected = match case {
            "native" | "proxy-preserve" => "inherited",
            "proxy-force" => "forced",
            _ => "cargo-home",
        };
        let expected_directory = if expected == "cargo-home" {
            "cargo-home/bin"
        } else {
            expected
        };
        assert_eq!(
            binding.json()["tools"]["before"][0]["path"],
            fixture
                .0
                .join(expected_directory)
                .join("probe-compiler")
                .to_str()
                .unwrap()
        );
        let mut command = environment.test_command(if case == "native" { &cargo } else { &proxy });
        command.args(["check", "--offline", "--locked"]);
        assert!(control.run(&mut command, |_| Ok(())).is_err());
        assert_eq!(
            fs::read_to_string(output).unwrap().trim(),
            expected,
            "{case}"
        );
        binding.verify(control).unwrap();
        assert!(binding.verified());
    }
}
