use super::*;
use crate::cargo_tool_settings::CargoToolSettings;
use crate::configured_cargo_tools::ConfiguredCargoTools;
use crate::test_orchestrator_test::temporary_fixture;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

fn executable(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn tool(directory: &Path, name: &str) -> PathBuf {
    directory.join(Path::new(name).with_extension(std::env::consts::EXE_EXTENSION))
}

fn environment(root: &Path, name: &str) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        [
            ("PATH".into(), root.join("proxies").into_os_string()),
            ("RUSTUP_HOME".into(), root.join("rustup").into_os_string()),
            ("CARGO_HOME".into(), root.join("cargo").into_os_string()),
            ("RUSTUP_TOOLCHAIN".into(), name.into()),
        ],
        root,
    )
    .unwrap()
}

fn settings(root: &Path, explicit: bool) -> CargoToolSettings {
    let source = format!(
        "[build]\n{}[env]\nPATH = {{ value = 'effective', relative = true, force = true }}\n",
        if explicit { "rustc = 'rustc'\n" } else { "" }
    );
    fs::create_dir_all(root.join(".cargo")).unwrap();
    fs::write(root.join(".cargo/config.toml"), &source).unwrap();
    CargoToolSettings::from_document(
        &root.join(".cargo/config.toml"),
        &toml::from_str(&source).unwrap(),
    )
    .unwrap()
}

fn populate(root: &Path) {
    for name in ["rustup", "rustc", "rustdoc"] {
        executable(&tool(&root.join("proxies"), name), "equal-size proxy");
    }
    for name in ["rustc", "rustdoc"] {
        executable(
            &tool(&root.join("rustup/toolchains/custom/bin"), name),
            "native tool",
        );
        executable(&tool(&root.join("effective"), name), "effective tool");
    }
}

#[test]
fn default_dispatch_rechecks_shadowing_missing_targets_and_bound_bytes() {
    let control = ProcessControl::new(Duration::from_secs(10));
    for mutation in [
        "shadow",
        "different-size",
        "missing-native",
        "changed-bytes",
        "missing-proxy",
    ] {
        let fixture = temporary_fixture("cargo-default-mutation");
        populate(&fixture.0);
        let settings = settings(&fixture.0, false);
        let environment = environment(&fixture.0, "custom");
        let mut binding =
            ConfiguredCargoTools::capture_selected(&settings, &environment, None, control).unwrap();
        assert_eq!(
            binding.json()["scope"],
            "selected-cargo-tool-entrypoints-v2"
        );
        let rows = binding.json()["tools"]["before"]
            .as_array()
            .unwrap()
            .clone();
        for row in rows {
            let name = row["role"].as_str().unwrap().to_ascii_lowercase();
            assert_eq!(
                row["path"],
                tool(&fixture.0.join("rustup/toolchains/custom/bin"), &name)
                    .to_str()
                    .unwrap()
            );
        }
        match mutation {
            "shadow" => executable(&tool(&fixture.0.join("target/debug"), "rustc"), "short"),
            "different-size" => executable(&tool(&fixture.0.join("proxies"), "rustc"), "short"),
            "missing-native" => fs::remove_file(tool(
                &fixture.0.join("rustup/toolchains/custom/bin"),
                "rustc",
            ))
            .unwrap(),
            "changed-bytes" => executable(
                &tool(&fixture.0.join("rustup/toolchains/custom/bin"), "rustc"),
                "native edit",
            ),
            "missing-proxy" => fs::remove_file(tool(&fixture.0.join("proxies"), "rustup")).unwrap(),
            _ => unreachable!(),
        }
        assert!(binding.verify(control).is_err(), "{mutation}");
        assert!(!binding.verified());
    }
}

#[test]
fn explicit_overrides_path_names_and_proxy_names_use_distinct_selection_rules() {
    let fixture = temporary_fixture("cargo-default-names");
    populate(&fixture.0);
    let root = fixture.0.join("rustup/toolchains/custom");
    let control = ProcessControl::new(Duration::from_secs(10));
    for (name, explicit, proxy, expected_native) in [
        ("custom", false, None, true),
        ("custom", true, None, false),
        (root.to_str().unwrap(), false, None, false),
        (
            "ignored-inherited-name",
            false,
            Some((root.as_path(), "custom")),
            true,
        ),
        (
            "custom",
            false,
            Some((root.as_path(), root.to_str().unwrap())),
            false,
        ),
    ] {
        let settings = settings(&fixture.0, explicit);
        let environment = environment(&fixture.0, name);
        let mut binding =
            ConfiguredCargoTools::capture_selected(&settings, &environment, proxy, control)
                .unwrap();
        let json = binding.json();
        let compiler = json["tools"]["before"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["role"] == "RUSTC")
            .unwrap();
        let expected = if expected_native {
            root.join("bin")
        } else {
            fixture.0.join("effective")
        };
        assert_eq!(compiler["path"], tool(&expected, "rustc").to_str().unwrap());
        binding.verify(control).unwrap();
        assert!(binding.verified());
    }
}

#[cfg(unix)]
#[test]
fn real_cargo_matches_default_dispatch_including_its_nonexecutable_size_heuristic() {
    use crate::executable_binding::resolve_program;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let control = ProcessControl::new(Duration::from_secs(20));
    let rustup = resolve_program("rustup", &std::env::var_os("PATH").unwrap()).unwrap();
    let output = control
        .capture_stdout(
            Command::new(rustup).args(["which", "cargo"]),
            64 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let cargo = PathBuf::from(std::str::from_utf8(&output).unwrap().trim());
    for case in [
        "native",
        "nonexecutable-proxy",
        "same-size-different-bytes",
        "different-size",
        "path-toolchain",
        "missing-native",
        "explicit",
        "no-toolchain",
    ] {
        let fixture = temporary_fixture("cargo-default-real");
        populate(&fixture.0);
        fs::create_dir(fixture.0.join("src")).unwrap();
        fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = 'default_tools_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n").unwrap();
        fs::write(
            fixture.0.join("Cargo.lock"),
            "version = 4\n[[package]]\nname = 'default_tools_fixture'\nversion = '0.0.0'\n",
        )
        .unwrap();
        fs::write(fixture.0.join("src/lib.rs"), "// Must not compile.\n").unwrap();
        let root = fixture.0.join("rustup/toolchains/custom");
        match case {
            "nonexecutable-proxy" => fs::set_permissions(
                tool(&fixture.0.join("proxies"), "rustc"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap(),
            "same-size-different-bytes" => executable(
                &tool(&fixture.0.join("proxies"), "rustc"),
                "different bytes!",
            ),
            "different-size" => executable(&tool(&fixture.0.join("proxies"), "rustc"), "short"),
            "missing-native" => fs::remove_file(tool(&root.join("bin"), "rustc")).unwrap(),
            _ => (),
        }
        let settings = settings(&fixture.0, case == "explicit");
        let name = if case == "path-toolchain" {
            root.to_str().unwrap()
        } else {
            "custom"
        };
        let base = environment(&fixture.0, name);
        let wrapper = fixture.0.join("wrapper");
        executable(
            &wrapper,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > \"$FIXTURE_TRACE\"\nexit 89\n",
        );
        let mut command = base.command(Path::new("environment"));
        command
            .env("RUSTC_WRAPPER", &wrapper)
            .env("FIXTURE_TRACE", fixture.0.join("trace"));
        if case == "no-toolchain" {
            command.env_remove("RUSTUP_TOOLCHAIN");
        }
        let environment = ExecutionEnvironment::from_entries(
            command
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned()))),
            &fixture.0,
        )
        .unwrap();
        let mut binding =
            ConfiguredCargoTools::capture_selected(&settings, &environment, None, control).unwrap();
        let json = binding.json();
        let selected = json["tools"]["before"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["role"] == "RUSTC")
            .unwrap()["path"]
            .as_str()
            .unwrap();
        let mut command = environment.test_command(&cargo);
        command.args(["test", "--offline", "--locked", "--no-run", "--lib"]);
        let result = control.capture_stdout(&mut command, 64 * 1024, |_| Ok(()));
        assert!(
            result.is_err(),
            "{case}: compiler probe must stop before compilation"
        );
        let trace = fs::read_to_string(fixture.0.join("trace")).unwrap();
        // Cargo passes its bare fallback to the wrapper; resolve it using the
        // forced subprocess PATH just as the eventual compiler launch would.
        let actual = resolve_program(
            trace.trim(),
            &std::env::join_paths([fixture.0.join("effective")]).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, Path::new(selected), "{case}");
        assert!(
            fixture.0.join("target/debug/deps").read_dir().is_err(),
            "{case}: no compilation"
        );
        binding.verify(control).unwrap();
        assert!(binding.verified());
    }
}

#[cfg(unix)]
#[test]
fn real_proxy_observation_distinguishes_custom_link_name_from_absolute_selection() {
    use crate::executable_binding::{resolve_program, ExecutableBinding};
    use crate::launch_ledger::LaunchLedger;
    use std::os::unix::fs::symlink;
    use std::process::Command;
    let control = ProcessControl::new(Duration::from_secs(20));
    let rustup = resolve_program("rustup", &std::env::var_os("PATH").unwrap()).unwrap();
    let output = control
        .capture_stdout(
            Command::new(&rustup).args(["which", "cargo"]),
            64 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let native_cargo = PathBuf::from(std::str::from_utf8(&output).unwrap().trim());
    let native_root = native_cargo.parent().unwrap().parent().unwrap();
    for named in [true, false] {
        let fixture = temporary_fixture("cargo-default-real-proxy");
        for name in ["rustup", "rustc", "rustdoc"] {
            executable(&tool(&fixture.0.join("proxies"), name), "equal-size proxy");
        }
        for name in ["rustc", "rustdoc"] {
            executable(&tool(&fixture.0.join("effective"), name), "effective tool");
        }
        let root = fixture.0.join("rustup/toolchains/custom");
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        symlink(native_root, &root).unwrap();
        fs::create_dir(fixture.0.join("src")).unwrap();
        fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = 'proxy_default_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n").unwrap();
        fs::write(
            fixture.0.join("Cargo.lock"),
            "version = 4\n[[package]]\nname = 'proxy_default_fixture'\nversion = '0.0.0'\n",
        )
        .unwrap();
        fs::write(fixture.0.join("src/lib.rs"), "// Must not compile.\n").unwrap();
        let settings = settings(&fixture.0, false);
        let name = if named {
            "custom"
        } else {
            root.to_str().unwrap()
        };
        let base = environment(&fixture.0, name);
        let wrapper = fixture.0.join("wrapper");
        executable(
            &wrapper,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > \"$FIXTURE_TRACE\"\nexit 89\n",
        );
        let mut command = base.command(Path::new("environment"));
        command
            .env("RUSTC_WRAPPER", &wrapper)
            .env("FIXTURE_TRACE", fixture.0.join("trace"));
        let environment = ExecutionEnvironment::from_entries(
            command
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned()))),
            &fixture.0,
        )
        .unwrap();
        let executables =
            ExecutableBinding::capture(&[("rustup", rustup.clone())], control).unwrap();
        let report = fixture.0.join("report.json");
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(20)).unwrap();
        let observed = crate::rustup_selection::observe(
            &environment,
            &executables,
            &root,
            &mut ledger,
            control,
        )
        .unwrap();
        assert_eq!(observed.name, name);
        assert_eq!(observed.source, "env");
        let mut binding = ConfiguredCargoTools::capture_selected(
            &settings,
            &environment,
            Some((&root, &observed.name)),
            control,
        )
        .unwrap();
        let proxy = fixture.0.join("cargo");
        symlink(&rustup, &proxy).unwrap();
        let mut command = environment.test_command(&proxy);
        command.args(["test", "--offline", "--locked", "--no-run", "--lib"]);
        let result = control.capture_stdout(&mut command, 64 * 1024, |_| Ok(()));
        assert!(result.is_err());
        let trace = fs::read_to_string(fixture.0.join("trace")).unwrap();
        let expected = if named {
            root.join("bin/rustc")
        } else {
            PathBuf::from("rustc")
        };
        assert_eq!(Path::new(trace.trim()), expected);
        binding.verify(control).unwrap();
        assert!(binding.verified());
        ledger.finish().unwrap();
        let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(report["direct_process_launch_count"], 1);
        assert_eq!(report["direct_cargo_launch_count"], 0);
    }
}
